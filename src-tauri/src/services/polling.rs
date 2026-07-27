use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tracing::{error, info, warn};

use crate::api::types::{
    ClaimJobRequest, CompleteJobRequest, FailJobRequest, GetPayloadRequest, PollJobsRequest,
    UpdateStatusRequest,
};
use crate::api::ConnectorApiClient;
use crate::credentials::{ensure_access_token, has_access_token, is_paired, load_credentials};
use crate::marketplace::jobs::PrepareForReviewExecutor;
use crate::marketplace::payload::{
    reject_unsupported_execution_mode, validate_and_normalize, ExecutionMode, VehicleJobPayload,
};
use crate::runtime::FacebookRuntime;
use crate::state::{AppState, ConnectionState};
use crate::version::CONNECTOR_VERSION;

/// Job transport polls the backend; browser automation uses `FacebookRuntime` (M3.3+).
pub struct PollingService {
    enabled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    active_job_id: Arc<Mutex<Option<String>>>,
}

impl PollingService {
    pub fn spawn(
        app: AppHandle,
        state: Arc<Mutex<AppState>>,
        client: Arc<ConnectorApiClient>,
        facebook_runtime: Arc<FacebookRuntime>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            enabled: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
            active_job_id: Arc::new(Mutex::new(None)),
        });

        let enabled_flag = service.enabled.clone();
        let shutdown_flag = service.shutdown.clone();
        let busy_flag = service.busy.clone();
        let active_job_id = service.active_job_id.clone();

        tauri::async_runtime::spawn(async move {
            let app_handle = app.clone();
            loop {
                if shutdown_flag.load(Ordering::SeqCst) {
                    info!("polling loop stopped");
                    break;
                }

                if !enabled_flag.load(Ordering::SeqCst) || !is_paired() {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }

                if busy_flag.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }

                match poll_and_process(
                    &app_handle,
                    &client,
                    &state,
                    busy_flag.clone(),
                    active_job_id.clone(),
                    facebook_runtime.clone(),
                )
                .await
                {
                    Ok(()) => {}
                    Err(err) => {
                        error!(error = %err, "job transport handler failed");
                        state.lock().last_error = Some(err.to_string());
                        let _ = app.emit("connector://status-changed", state.lock().status_snapshot());
                    }
                }

                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });

        service
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        if enabled {
            info!("job polling enabled");
        }
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub fn active_job_id(&self) -> Option<String> {
        self.active_job_id.lock().clone()
    }
}

pub fn enable_polling_if_authenticated(polling: &PollingService, state: &mut AppState) {
    if is_paired() {
        polling.set_enabled(true);
        state.paired = true;
        state.needs_reconnect = false;
        state.connection_state = ConnectionState::Idle;
    }
}

async fn poll_and_process(
    app: &AppHandle,
    client: &ConnectorApiClient,
    state: &Arc<Mutex<AppState>>,
    busy: Arc<AtomicBool>,
    active_job_id: Arc<Mutex<Option<String>>>,
    facebook_runtime: Arc<FacebookRuntime>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !has_access_token() {
        if !ensure_access_token(client).await? {
            return Ok(());
        }
    }

    let creds = load_credentials()?.ok_or("missing credentials")?;
    if creds.access_token.is_empty() {
        return Ok(());
    }
    let device_id = state.lock().device_id.to_string();

    let poll = client
        .poll_jobs(
            PollJobsRequest {
                action: "poll_jobs".into(),
                device_id: device_id.clone(),
                user_id: creds.user_id.clone(),
                dealership_id: creds.dealership_id.clone(),
                connector_version: CONNECTOR_VERSION.to_string(),
            },
            &creds.access_token,
        )
        .await
        .map_err(|e| format!("poll_jobs failed: {e}"))?;

    let job = poll.jobs.first().cloned();
    let Some(job) = job else {
        return Ok(());
    };

    {
        let active = active_job_id.lock();
        if active.as_deref() == Some(&job.id) {
            info!(job_id = %job.id, "skipping duplicate job execution — already active");
            return Ok(());
        }
    }

    info!(
        job_id = %job.id,
        status = %job.status,
        jobs_available = poll.jobs_available,
        "connector job received from poll_jobs"
    );

    busy.store(true, Ordering::SeqCst);
    {
        let mut guard = active_job_id.lock();
        *guard = Some(job.id.clone());
    }
    {
        let mut guard = state.lock();
        guard.current_job_id = Some(job.id.clone());
        guard.connection_state = ConnectionState::Connected;
    }
    let result = process_job(
        app,
        client,
        &creds,
        &device_id,
        &job.id,
        facebook_runtime,
    )
    .await;
    busy.store(false, Ordering::SeqCst);
    {
        let mut guard = active_job_id.lock();
        *guard = None;
    }
    {
        let mut guard = state.lock();
        guard.current_job_id = None;
    }
    result
}

async fn process_job(
    app: &AppHandle,
    client: &ConnectorApiClient,
    creds: &crate::credentials::StoredCredentials,
    device_id: &str,
    job_id: &str,
    facebook_runtime: Arc<FacebookRuntime>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(job_id, "job started — claiming via transport layer");

    let claim = client
        .claim_job(
            ClaimJobRequest {
                action: "claim_job".into(),
                job_id: job_id.to_string(),
                device_id: device_id.to_string(),
                user_id: creds.user_id.clone(),
                dealership_id: creds.dealership_id.clone(),
                connector_version: CONNECTOR_VERSION.to_string(),
            },
            &creds.access_token,
        )
        .await
        .map_err(|e| format!("claim_job failed for {job_id}: {e}"))?;

    if !claim.ok {
        let msg = claim
            .error
            .unwrap_or_else(|| "claim_job returned ok=false".into());
        return Err(format!(
            "claim_job rejected for {job_id}: {msg} (code={:?})",
            claim.error_code
        )
        .into());
    }

    let scoped = claim.scoped_job_token.ok_or_else(|| {
        format!("claim_job succeeded but scopedJobToken missing for {job_id}")
    })?;

    info!(
        job_id,
        scoped_token_len = scoped.len(),
        "job claimed — fetching payload"
    );

    let mut payload = client
        .get_payload(
            GetPayloadRequest {
                action: "get_payload".into(),
                job_id: job_id.to_string(),
            },
            &scoped,
        )
        .await
        .map_err(|e| format!("get_payload failed for {job_id}: {e}"))?;

    payload.scoped_job_token = Some(scoped.clone());

    if let Some(reject) = reject_unsupported_execution_mode(payload.execution_mode) {
        warn!(
            job_id,
            execution_mode = payload.execution_mode.as_str(),
            code = %reject.code,
            "rejecting job before browser — execution mode not supported in M3"
        );
        fail_job(client, job_id, &scoped, &reject.code, &reject.message).await?;
        return Ok(());
    }

    let validation_errors = validate_and_normalize(&mut payload);
    if !validation_errors.is_empty() {
        let summary = validation_errors
            .iter()
            .map(|e| format!("{}:{}", e.field, e.code))
            .collect::<Vec<_>>()
            .join(", ");
        warn!(job_id, errors = %summary, "payload validation failed before browser");
        fail_job(
            client,
            job_id,
            &scoped,
            "PAYLOAD_VALIDATION_FAILED",
            &format!("Payload validation failed: {summary}"),
        )
        .await?;
        return Ok(());
    }

    if payload.is_transport_test() {
        return run_transport_test_flow(client, job_id, &scoped).await;
    }

    if matches!(payload.execution_mode, ExecutionMode::PrepareForReview) {
        let executor = PrepareForReviewExecutor::new(facebook_runtime);
        return executor
            .execute(app, client, job_id, &scoped, &payload)
            .await;
    }

    warn!(job_id, "unexpected execution mode after validation");
    Ok(())
}

async fn run_transport_test_flow(
    client: &ConnectorApiClient,
    job_id: &str,
    scoped: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(job_id, "transport_test — beginning simulated progress updates");

    let steps = [
        ("browser_opening", 10u8, "Opening browser (simulated)"),
        ("checking_facebook_session", 25u8, "Checking Facebook session (simulated)"),
        ("ready_for_review", 90u8, "Ready for review (transport test)"),
    ];

    for (status, progress, step) in steps {
        info!(job_id, status, progress, step, "job progress update");
        client
            .update_status(
                UpdateStatusRequest {
                    action: "update_status".into(),
                    job_id: job_id.to_string(),
                    status: status.into(),
                    progress,
                    current_step: step.into(),
                },
                scoped,
            )
            .await
            .map_err(|e| format!("update_status failed for {job_id} at {status}: {e}"))?;
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    info!(job_id, "job completing via transport layer");
    client
        .complete_job(
            CompleteJobRequest {
                action: "complete_job".into(),
                job_id: job_id.to_string(),
                listing_url: "https://www.facebook.com/marketplace/item/9999999999".into(),
            },
            scoped,
        )
        .await
        .map_err(|e| format!("complete_job failed for {job_id}: {e}"))?;

    info!(job_id, "job completed — transport test finished (dashboard should show posted)");
    Ok(())
}

async fn fail_job(
    client: &ConnectorApiClient,
    job_id: &str,
    scoped: &str,
    error_code: &str,
    error_message: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    client
        .fail_job(
            FailJobRequest {
                action: "fail_job".into(),
                job_id: job_id.to_string(),
                error_code: error_code.to_string(),
                error_message: error_message.to_string(),
                user_message: Some(error_message.to_string()),
            },
            scoped,
        )
        .await
        .map_err(|e| format!("fail_job failed for {job_id}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::payload::types::ListingOptions;

    #[test]
    fn transport_test_payload_routes_to_simulated_flow_flag() {
        let payload = VehicleJobPayload {
            contract_version: 1,
            job_id: "job-test".into(),
            user_id: String::new(),
            dealership_id: String::new(),
            inventory_id: String::new(),
            inventory_source: String::new(),
            year: String::new(),
            make: String::new(),
            model: String::new(),
            trim: String::new(),
            body_style: String::new(),
            vehicle_type: String::new(),
            condition: String::new(),
            price: String::new(),
            mileage: String::new(),
            vin: String::new(),
            stock_number: String::new(),
            exterior_color: String::new(),
            interior_color: String::new(),
            transmission: String::new(),
            drivetrain: String::new(),
            fuel_type: String::new(),
            title: String::new(),
            description: String::new(),
            location: String::new(),
            ordered_image_urls: vec![],
            listing_options: ListingOptions::default(),
            posting_preferences: serde_json::json!({}),
            execution_mode: ExecutionMode::TransportTest,
            idempotency_key: String::new(),
            source_metadata: serde_json::json!({}),
            expires_at: String::new(),
            test: true,
            label: Some("Transport test".into()),
            scoped_job_token: None,
        };
        assert!(payload.is_transport_test());
    }

    #[test]
    fn active_job_id_tracks_in_flight_job() {
        let service = Arc::new(PollingService {
            enabled: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
            active_job_id: Arc::new(Mutex::new(Some("job-123".into()))),
        });
        assert_eq!(service.active_job_id().as_deref(), Some("job-123"));
    }
}
