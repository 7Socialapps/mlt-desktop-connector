use std::sync::Arc;

use tauri::AppHandle;
use tracing::{info, warn};

use crate::api::types::FailJobRequest;
use crate::api::ConnectorApiClient;
use crate::marketplace::payload::VehicleJobPayload;
use crate::marketplace::{download_job_assets, vehicle_fill_payload_from_job, AssetError};
use crate::runtime::FacebookRuntime;

use super::errors::{JobErrorCode, JobExecutionError};
use super::evidence::store_job_screenshot;
use super::phases::JobPhase;
use super::progress::update_status;
use super::session_precheck::{session_error_code, session_error_message};
use super::tracker::JobProgressTracker;

pub struct PrepareForReviewExecutor {
    runtime: Arc<FacebookRuntime>,
    progress: Option<JobProgressTracker>,
}

impl PrepareForReviewExecutor {
    pub fn new(runtime: Arc<FacebookRuntime>) -> Self {
        Self {
            runtime,
            progress: None,
        }
    }

    pub fn with_progress_tracker(mut self, tracker: JobProgressTracker) -> Self {
        self.progress = Some(tracker);
        self
    }

    fn tracker(&self) -> Option<&JobProgressTracker> {
        self.progress.as_ref()
    }

    pub async fn execute(
        &self,
        app: &AppHandle,
        client: &ConnectorApiClient,
        job_id: &str,
        scoped: &str,
        payload: &VehicleJobPayload,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.runtime.bus.clear_cancel();

        match self.execute_inner(app, client, job_id, scoped, payload).await {
            Ok(()) => Ok(()),
            Err(err) => {
                self.fail_job(client, job_id, scoped, &err).await?;
                Err(format!("{}: {}", err.code.as_str(), err.message).into())
            }
        }
    }

    async fn execute_inner(
        &self,
        app: &AppHandle,
        client: &ConnectorApiClient,
        job_id: &str,
        scoped: &str,
        payload: &VehicleJobPayload,
    ) -> Result<(), JobExecutionError> {
        self.check_cancelled(JobPhase::Claimed)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::PreparingAssets,
            None,
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::PreparingAssets,
            )
        })?;

        info!(
            job_id,
            image_count = payload.ordered_image_urls.len(),
            "prepare_for_review — downloading listing photos"
        );

        let (manifest, workspace) = download_job_assets(app, payload)
            .await
            .map_err(|err| asset_error_to_job_error(err, JobPhase::PreparingAssets))?;

        let image_count = manifest.images.len() as u32;
        let fill_payload = vehicle_fill_payload_from_job(payload);

        self.check_cancelled(JobPhase::PreparingAssets)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::StartingRuntime,
            Some("Listing photos prepared; starting browser runtime"),
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::StartingRuntime,
            )
        })?;

        self.runtime
            .bus
            .ensure_browser_ready(crate::runtime::RuntimeServiceKind::Marketplace)
            .map_err(|e| {
                JobExecutionError::new(
                    JobErrorCode::BrowserNotReady,
                    e,
                    JobPhase::StartingRuntime,
                )
            })?;

        self.check_cancelled(JobPhase::StartingRuntime)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::CheckingFacebookSession,
            None,
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::CheckingFacebookSession,
            )
        })?;

        let session = self
            .runtime
            .session
            .check_session()
            .map_err(|e| {
                JobExecutionError::new(
                    JobErrorCode::RuntimeError,
                    e,
                    JobPhase::CheckingFacebookSession,
                )
            })?;

        if let Some(code) = session_error_code(&session.state) {
            let message = session_error_message(&code, session.reason_code.as_deref());
            return Err(JobExecutionError::new(
                code,
                message,
                JobPhase::CheckingFacebookSession,
            ));
        }

        self.check_cancelled(JobPhase::CheckingFacebookSession)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::OpeningMarketplace,
            None,
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::OpeningMarketplace,
            )
        })?;

        self.runtime
            .marketplace
            .open_marketplace()
            .map_err(|e| {
                JobExecutionError::new(
                    JobErrorCode::MarketplaceNavFailed,
                    e,
                    JobPhase::OpeningMarketplace,
                )
            })?;

        if !self.runtime.marketplace.is_ready() {
            let snap = self.runtime.marketplace.snapshot();
            return Err(
                JobExecutionError::new(
                    JobErrorCode::MarketplaceNotReady,
                    format!(
                        "Marketplace not ready (status={}, reason={:?})",
                        serde_json::to_string(&snap.status)
                            .unwrap_or_else(|_| "unknown".into()),
                        snap.reason_code
                    ),
                    JobPhase::OpeningMarketplace,
                )
                .with_screenshot(snap.screenshot_path),
            );
        }

        self.check_cancelled(JobPhase::OpeningMarketplace)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::OpeningVehicleCreate,
            None,
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::OpeningVehicleCreate,
            )
        })?;

        let create_snap = self
            .runtime
            .marketplace
            .open_vehicle_create_route()
            .map_err(|e| {
                JobExecutionError::new(
                    JobErrorCode::VehicleCreateRouteNotReady,
                    e,
                    JobPhase::OpeningVehicleCreate,
                )
            })?;

        self.check_cancelled(JobPhase::OpeningVehicleCreate)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::VerifyingVehicleCreate,
            None,
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::VerifyingVehicleCreate,
            )
        })?;

        let verification = self
            .runtime
            .marketplace
            .verify_vehicle_create_form()
            .map_err(|e| {
                JobExecutionError::new(
                    JobErrorCode::VehicleCreateVerificationFailed,
                    e,
                    JobPhase::VerifyingVehicleCreate,
                )
            })?;

        if !verification.ready {
            return Err(
                JobExecutionError::new(
                    JobErrorCode::VehicleCreateVerificationFailed,
                    format!(
                        "Vehicle create form not ready (reason={})",
                        verification.reason_code
                    ),
                    JobPhase::VerifyingVehicleCreate,
                )
                .with_screenshot(verification.screenshot_path),
            );
        }

        if let Some(ref screenshot) = create_snap.screenshot_path {
            let _ = store_job_screenshot(app, job_id, screenshot, "create-route");
        }

        self.check_cancelled(JobPhase::VerifyingVehicleCreate)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::UploadingImages,
            Some(&format!("Uploading {image_count} listing photos")),
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::UploadingImages,
            )
        })?;

        let upload_report = if image_count > 0 {
            self.runtime
                .marketplace
                .upload_vehicle_images(&manifest.images, workspace.path())
                .map_err(|e| {
                    JobExecutionError::new(
                        JobErrorCode::ImageUploadFailed,
                        e,
                        JobPhase::UploadingImages,
                    )
                })?
        } else {
            crate::marketplace::form::ImageUploadReport {
                uploaded: vec![],
                thumbnail_count: 0,
                expected_count: 0,
                primary_preserved: true,
            }
        };

        if image_count > 0 && !upload_report.primary_preserved {
            return Err(JobExecutionError::new(
                JobErrorCode::ImageUploadFailed,
                format!(
                    "Primary image not uploaded (thumbnails={}, expected={})",
                    upload_report.thumbnail_count, upload_report.expected_count
                ),
                JobPhase::UploadingImages,
            ));
        }

        self.check_cancelled(JobPhase::UploadingImages)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::FillingFields,
            None,
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(JobErrorCode::RuntimeError, e, JobPhase::FillingFields)
        })?;

        let fill_report = self
            .runtime
            .marketplace
            .fill_vehicle_form(&fill_payload)
            .map_err(|e| {
                JobExecutionError::new(JobErrorCode::FormFillFailed, e, JobPhase::FillingFields)
            })?;

        let required_failures = fill_report.required_failures();
        if !required_failures.is_empty() {
            return Err(JobExecutionError::new(
                JobErrorCode::FormFillFailed,
                format!("Required fields failed: {}", required_failures.join(", ")),
                JobPhase::FillingFields,
            ));
        }

        self.check_cancelled(JobPhase::FillingFields)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::VerifyingFields,
            None,
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(JobErrorCode::RuntimeError, e, JobPhase::VerifyingFields)
        })?;

        let form_verification = self
            .runtime
            .marketplace
            .verify_filled_form(&fill_payload, image_count)
            .map_err(|e| {
                JobExecutionError::new(
                    JobErrorCode::FormVerificationFailed,
                    e,
                    JobPhase::VerifyingFields,
                )
            })?;

        if !form_verification.ready {
            return Err(
                JobExecutionError::new(
                    JobErrorCode::FormVerificationFailed,
                    format!(
                        "Form verification failed (reason={}, missing={:?})",
                        form_verification.reason_code, form_verification.fields_missing
                    ),
                    JobPhase::VerifyingFields,
                )
                .with_screenshot(form_verification.screenshot_path.clone()),
            );
        }

        if let Some(ref screenshot) = form_verification.screenshot_path {
            let _ = store_job_screenshot(app, job_id, screenshot, "form-verified");
        }

        let _ = workspace.cleanup();

        self.check_cancelled(JobPhase::VerifyingFields)?;

        let verification_summary = format!(
            "filled={}, images={}/{}, next={}, publish={}",
            fill_report.filled.len(),
            form_verification.image_count,
            form_verification.expected_image_count,
            form_verification.has_next_button,
            form_verification.has_publish_button
        );

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::ReadyForReview,
            Some(&format!(
                "Listing prepared. Review the Facebook form and publish manually. ({verification_summary})"
            )),
            Some(&self.runtime),
            self.tracker(),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(JobErrorCode::RuntimeError, e, JobPhase::ReadyForReview)
        })?;

        let _ = self.runtime.marketplace.bring_browser_forward();

        info!(
            job_id,
            filled = fill_report.filled.len(),
            images = upload_report.thumbnail_count,
            "prepare_for_review — ready_for_review (human review mode)"
        );
        self.runtime.bus.clear_cancel();
        Ok(())
    }

    fn check_cancelled(&self, _phase: JobPhase) -> Result<(), JobExecutionError> {
        if self.runtime.bus.is_cancelled() {
            Err(JobExecutionError::cancelled())
        } else {
            Ok(())
        }
    }

    async fn fail_job(
        &self,
        client: &ConnectorApiClient,
        job_id: &str,
        scoped: &str,
        err: &JobExecutionError,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        warn!(
            job_id,
            code = err.code.as_str(),
            phase = err.phase.status_str(),
            "prepare_for_review job failed"
        );

        let _ = update_status(
            client,
            scoped,
            job_id,
            err.phase,
            Some(&err.message),
            Some(&self.runtime),
            self.tracker(),
        )
        .await;

        client
            .fail_job(
                FailJobRequest {
                    action: "fail_job".into(),
                    job_id: job_id.to_string(),
                    error_code: err.code.as_str().to_string(),
                    error_message: err.message.clone(),
                    user_message: Some(err.message.clone()),
                },
                scoped,
            )
            .await
            .map_err(|e| format!("fail_job failed for {job_id}: {e}"))?;

        self.runtime.bus.clear_cancel();
        Ok(())
    }
}

fn asset_error_to_job_error(err: AssetError, phase: JobPhase) -> JobExecutionError {
    JobExecutionError::new(
        JobErrorCode::ImageDownloadFailed,
        err.user_message(),
        phase,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserManager, BrowserRuntimeService, SidecarDaemon};
    use crate::runtime::FacebookRuntime;

    fn test_executor() -> PrepareForReviewExecutor {
        let runtime_svc = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(std::path::PathBuf::new()));
        let manager = Arc::new(BrowserManager::new(runtime_svc, daemon));
        PrepareForReviewExecutor::new(FacebookRuntime::new(manager))
    }

    #[test]
    fn cancellation_detected_between_phases() {
        let exec = test_executor();
        exec.runtime.bus.request_cancel();
        let result = exec.check_cancelled(JobPhase::OpeningMarketplace);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, JobErrorCode::OperationCancelled);
    }

    #[test]
    fn asset_error_maps_to_image_download_failed() {
        let err = asset_error_to_job_error(
            AssetError::Validation {
                index: 0,
                message: "empty".into(),
            },
            JobPhase::PreparingAssets,
        );
        assert_eq!(err.code, JobErrorCode::ImageDownloadFailed);
        assert_eq!(err.phase, JobPhase::PreparingAssets);
    }

    #[test]
    fn clear_cancel_on_new_execution() {
        let exec = test_executor();
        exec.runtime.bus.request_cancel();
        assert!(exec.runtime.bus.is_cancelled());
        exec.runtime.bus.clear_cancel();
        assert!(!exec.runtime.bus.is_cancelled());
    }
}
