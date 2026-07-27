use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use crate::api::types::{
    ClaimJobRequest, CompleteJobRequest, PollJobsRequest, UpdateStatusRequest,
};
use crate::api::ConnectorApiClient;
use crate::credentials::{has_access_token, load_credentials};
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

                if !enabled_flag.load(Ordering::SeqCst) || !has_access_token() {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }

                if busy_flag.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }

                if let Err(err) = poll_and_process(&client, &state, busy_flag.clone()).await {
                    warn!(error = %err, "job transport error");
                    state.lock().last_error = Some(err.to_string());
                    let _ = app.emit("connector://status-changed", state.lock().status_snapshot());
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
    if has_access_token() {
        polling.set_enabled(true);
        state.paired = true;
        state.connection_state = ConnectionState::Idle;
    }
}

async fn poll_and_process(
    client: &ConnectorApiClient,
    state: &Arc<Mutex<AppState>>,
    busy: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let creds = load_credentials()?.ok_or("missing credentials")?;
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
        .await?;

    let job = poll.jobs.first().cloned();
    let Some(job) = job else {
        return Ok(());
    };

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
    info!(job_id, "claiming connector job");

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
        .await?;

    let scoped = claim
        .scoped_job_token
        .ok_or_else(|| "missing scoped job token".to_string())?;

    let steps = [
        ("browser_opening", 10u8, "Opening browser (simulated)"),
        ("checking_facebook_session", 25u8, "Checking Facebook session (simulated)"),
        ("ready_for_review", 90u8, "Ready for review (transport test)"),
    ];

    for (status, progress, step) in steps {
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
            .await?;
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    client
        .complete_job(
            CompleteJobRequest {
                action: "complete_job".into(),
                job_id: job_id.to_string(),
                listing_url: "https://www.facebook.com/marketplace/item/9999999999".into(),
            },
            &scoped,
        )
        .await?;

    info!(job_id, "test job completed via transport layer");
    Ok(())
}
