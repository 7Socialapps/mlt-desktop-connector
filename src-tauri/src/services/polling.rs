use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tracing::{error, info};

use crate::api::types::{
    ClaimJobRequest, CompleteJobRequest, PollJobsRequest, UpdateStatusRequest,
};
use crate::api::ConnectorApiClient;
use crate::credentials::{ensure_access_token, has_access_token, is_paired, load_credentials};
use crate::state::{AppState, ConnectionState};
use crate::version::CONNECTOR_VERSION;

/// Executes connector job transport for test/dummy jobs — no Facebook automation.
pub struct PollingService {
    enabled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
}

impl PollingService {
    pub fn spawn(
        app: AppHandle,
        state: Arc<Mutex<AppState>>,
        client: Arc<ConnectorApiClient>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            enabled: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
        });

        let enabled_flag = service.enabled.clone();
        let shutdown_flag = service.shutdown.clone();
        let busy_flag = service.busy.clone();

        tauri::async_runtime::spawn(async move {
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

                match poll_and_process(&client, &state, busy_flag.clone()).await {
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
    client: &ConnectorApiClient,
    state: &Arc<Mutex<AppState>>,
    busy: Arc<AtomicBool>,
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

    info!(
        job_id = %job.id,
        status = %job.status,
        jobs_available = poll.jobs_available,
        "connector job received from poll_jobs"
    );

    busy.store(true, Ordering::SeqCst);
    let result = process_job(client, &creds, &device_id, &job.id).await;
    busy.store(false, Ordering::SeqCst);
    result
}

async fn process_job(
    client: &ConnectorApiClient,
    creds: &crate::credentials::StoredCredentials,
    device_id: &str,
    job_id: &str,
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
        "job claimed — beginning simulated progress updates"
    );

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
                &scoped,
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
            &scoped,
        )
        .await
        .map_err(|e| format!("complete_job failed for {job_id}: {e}"))?;

    info!(job_id, "job completed — transport test finished (dashboard should show posted)");
    Ok(())
}
