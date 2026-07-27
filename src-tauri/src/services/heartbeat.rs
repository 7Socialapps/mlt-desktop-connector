use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use crate::api::types::{ConnectorOs, FacebookSessionState, HeartbeatRequest};
use crate::api::ConnectorApiClient;
use crate::credentials::{clear_credentials, has_access_token, load_credentials};
use crate::state::{AppState, ConnectionState};
use crate::version::{CONNECTOR_VERSION, DEFAULT_CAPABILITIES};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(120);

pub struct HeartbeatService {
    shutdown: Arc<AtomicBool>,
    wake: Arc<Notify>,
}

impl HeartbeatService {
    pub fn spawn(
        app: AppHandle,
        state: Arc<Mutex<AppState>>,
        client: Arc<ConnectorApiClient>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Notify::new()),
        });

        let shutdown_flag = service.shutdown.clone();
        let wake_flag = service.wake.clone();

        tauri::async_runtime::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;
            loop {
                if shutdown_flag.load(Ordering::SeqCst) {
                    info!("heartbeat loop stopped");
                    break;
                }

                if !has_access_token() {
                    debug!("heartbeat skipped — device not paired");
                    state.lock().connection_state = ConnectionState::Idle;
                    emit_status(&app, &state);
                    tokio::select! {
                        _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {},
                        _ = wake_flag.notified() => {},
                    }
                    continue;
                }

                match send_heartbeat(&client, &state).await {
                    Ok(at) => {
                        backoff = INITIAL_BACKOFF;
                        state.lock().last_heartbeat_at = Some(at);
                        state.lock().last_error = None;
                        state.lock().connection_state = ConnectionState::Connected;
                        emit_status(&app, &state);
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        if msg.contains("DEVICE_REVOKED") || msg.contains("revoked") {
                            let _ = clear_credentials();
                            state.lock().paired = false;
                            state.lock().connection_state = ConnectionState::Offline;
                            state.lock().last_error =
                                Some("Device revoked — pair again from dashboard".into());
                            emit_status(&app, &state);
                            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
                            continue;
                        }
                        warn!(error = %err, "heartbeat failed");
                        {
                            let mut guard = state.lock();
                            guard.last_error = Some(err.to_string());
                            guard.connection_state = ConnectionState::Reconnecting;
                        }
                        emit_status(&app, &state);
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                }

                tokio::select! {
                    _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {},
                    _ = wake_flag.notified() => {},
                }
            }
        });

        service
    }

    pub fn trigger_now(&self) {
        self.wake.notify_one();
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }
}

async fn send_heartbeat(
    client: &ConnectorApiClient,
    state: &Arc<Mutex<AppState>>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let creds = load_credentials()?.ok_or("missing credentials")?;
    let device_id = state.lock().device_id.to_string();

    let request = HeartbeatRequest {
        action: "heartbeat".into(),
        device_id,
        user_id: creds.user_id.clone(),
        dealership_id: creds.dealership_id.clone(),
        connector_version: CONNECTOR_VERSION.to_string(),
        os: ConnectorOs::detect(),
        capabilities: DEFAULT_CAPABILITIES.iter().map(|s| s.to_string()).collect(),
        facebook_session_state: FacebookSessionState::Unknown,
    };

    let response = client
        .heartbeat(request, &creds.access_token)
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    Ok(response.last_heartbeat_at)
}

fn emit_status(app: &AppHandle, state: &Arc<Mutex<AppState>>) {
    let _ = app.emit("connector://status-changed", state.lock().status_snapshot());
}
