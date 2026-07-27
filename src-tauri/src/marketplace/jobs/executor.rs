use std::sync::Arc;

use tauri::AppHandle;
use tracing::{info, warn};

use crate::api::types::FailJobRequest;
use crate::api::ConnectorApiClient;
use crate::marketplace::payload::VehicleJobPayload;
use crate::marketplace::{download_job_assets, AssetError};
use crate::runtime::FacebookRuntime;

use super::errors::{JobErrorCode, JobExecutionError};
use super::evidence::store_job_screenshot;
use super::phases::JobPhase;
use super::progress::update_status;
use super::session_precheck::{session_error_code, session_error_message};

pub struct PrepareForReviewExecutor {
    runtime: Arc<FacebookRuntime>,
}

impl PrepareForReviewExecutor {
    pub fn new(runtime: Arc<FacebookRuntime>) -> Self {
        Self { runtime }
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

        let (_manifest, workspace) = download_job_assets(app, payload)
            .await
            .map_err(|err| asset_error_to_job_error(err, JobPhase::PreparingAssets))?;

        self.check_cancelled(JobPhase::PreparingAssets)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::StartingRuntime,
            Some("Listing photos prepared; starting browser runtime"),
            Some(&self.runtime),
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

        let _ = workspace.cleanup();

        self.check_cancelled(JobPhase::VerifyingVehicleCreate)?;

        update_status(
            client,
            scoped,
            job_id,
            JobPhase::CreateRouteReady,
            Some("Vehicle create route validated — ready for dealer review"),
            Some(&self.runtime),
        )
        .await
        .map_err(|e| {
            JobExecutionError::new(
                JobErrorCode::RuntimeError,
                e,
                JobPhase::CreateRouteReady,
            )
        })?;

        info!(job_id, "prepare_for_review — create route ready (M3.3 terminal success)");
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
