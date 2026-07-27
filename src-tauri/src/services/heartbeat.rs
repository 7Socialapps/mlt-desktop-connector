use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use crate::api::types::{ConnectorOs, HeartbeatRequest};
use crate::api::ConnectorApiClient;
use crate::browser::BrowserManager;
use crate::credentials::{
    self, ensure_access_token, handle_revoked_device, has_access_token, is_paired,
    load_credentials,
};
use crate::runtime::FacebookRuntime;
use crate::services::connection_test::build_heartbeat_browser_fields;
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
        browser_manager: Arc<BrowserManager>,
        facebook_runtime: Arc<FacebookRuntime>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Notify::new()),
        });

        let shutdown_flag = service.shutdown.clone();
        let wake_flag = service.wake.clone();
        let fb_runtime = facebook_runtime.clone();

        tauri::async_runtime::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;
            loop {
                if shutdown_flag.load(Ordering::SeqCst) {
                    info!("heartbeat loop stopped");
                    break;
                }

                if !is_paired() {
                    debug!("heartbeat skipped — device not paired");
                    {
                        let mut guard = state.lock();
                        guard.paired = false;
                        if credentials::credential_status()
                            == credentials::CredentialStatus::NeedsReconnect
                        {
                            guard.needs_reconnect = true;
                            if guard.last_error.is_none() {
                                guard.last_error = credentials::needs_reconnect_message();
                            }
                        }
                        guard.connection_state = ConnectionState::Offline;
                    }
                    emit_status(&app, &state);
                    tokio::select! {
                        _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {},
                        _ = wake_flag.notified() => {},
                    }
                    continue;
                }

                if !has_access_token() {
                    match ensure_access_token(&client).await {
                        Ok(true) => {
                            state.lock().paired = true;
                            state.lock().needs_reconnect = false;
                            state.lock().last_error = None;
                        }
                        Ok(false) => {
                            {
                                let mut guard = state.lock();
                                guard.paired = false;
                                guard.needs_reconnect = true;
                                if guard.last_error.is_none() {
                                    guard.last_error = credentials::needs_reconnect_message();
                                }
                                guard.connection_state = ConnectionState::Offline;
                            }
                            emit_status(&app, &state);
                            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
                            continue;
                        }
                        Err(err) => {
                            warn!(error = %err, "heartbeat token refresh failed");
                        }
                    }
                }

                match send_heartbeat(&client, &state, &browser_manager, &fb_runtime).await {
                    Ok(at) => {
                        backoff = INITIAL_BACKOFF;
                        {
                            let mut guard = state.lock();
                            guard.last_heartbeat_at = Some(at);
                            guard.last_error = None;
                            guard.paired = true;
                            guard.needs_reconnect = false;
                            guard.connection_state = ConnectionState::Connected;
                        }
                        emit_status(&app, &state);
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        if msg.contains("DEVICE_REVOKED") || msg.contains("revoked") {
                            let _ = handle_revoked_device();
                            {
                                let mut guard = state.lock();
                                guard.paired = false;
                                guard.needs_reconnect = true;
                                guard.connection_state = ConnectionState::Offline;
                                guard.last_error =
                                    Some("Device revoked — start pairing again from the dashboard.".into());
                            }
                            emit_status(&app, &state);
                            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
                            continue;
                        }
                        warn!(error = %err, "heartbeat failed");
                        {
                            let mut guard = state.lock();
                            guard.last_error = Some(sanitize_error(&msg));
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
    browser_manager: &Arc<BrowserManager>,
    facebook_runtime: &Arc<FacebookRuntime>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let creds = load_credentials()?.ok_or("missing credentials")?;
    if creds.access_token.is_empty() {
        return Err("access token unavailable — reconnect required".into());
    }
    let (device_id, browser_fields) = {
        let guard = state.lock();
        let browser_snapshot = browser_manager.snapshot();
        let fields = build_heartbeat_browser_fields(&guard, &browser_snapshot, facebook_runtime);
        (guard.device_id.to_string(), fields)
    };

    let request = HeartbeatRequest {
        action: "heartbeat".into(),
        device_id,
        user_id: creds.user_id,
        dealership_id: creds.dealership_id,
        connector_version: browser_fields.connector_version.clone(),
        os: ConnectorOs::detect(),
        capabilities: DEFAULT_CAPABILITIES.iter().map(|s| s.to_string()).collect(),
        connector_status: browser_fields.connector_status,
        browser_status: browser_fields.browser_status,
        browser_version: browser_fields.browser_version,
        profile_status: browser_fields.profile_status,
        facebook_session_state: browser_fields.facebook_session_state,
        marketplace_status: browser_fields.marketplace_status,
        current_browser_url_category: browser_fields.current_browser_url_category,
        last_browser_check_at: browser_fields.last_browser_check_at,
        last_browser_error_code: browser_fields.last_browser_error_code,
        current_facebook_account: browser_fields.current_facebook_account,
        session_state: browser_fields.session_state,
        marketplace_ready: browser_fields.marketplace_ready,
        messenger_ready: browser_fields.messenger_ready,
        notifications_ready: browser_fields.notifications_ready,
        browser_pid: browser_fields.browser_pid,
        profile_version: browser_fields.profile_version,
        current_service: browser_fields.current_service,
        last_health_check: browser_fields.last_health_check,
        last_restart: browser_fields.last_restart,
        browser_state: browser_fields.browser_state,
        facebook_account_label: browser_fields.facebook_account_label,
        marketplace_state: browser_fields.marketplace_state,
        messenger_state: browser_fields.messenger_state,
        notifications_state: browser_fields.notifications_state,
        current_destination: browser_fields.current_destination,
        last_navigation_error: browser_fields.last_navigation_error,
        last_health_check_at: browser_fields.last_health_check_at,
        last_restart_at: browser_fields.last_restart_at,
    };

    let response = client
        .heartbeat(request, &creds.access_token)
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    Ok(response.last_heartbeat_at)
}

fn sanitize_error(message: &str) -> String {
    if message.contains("Bearer ") || message.len() > 300 {
        "Connection error — retrying".to_string()
    } else {
        message.to_string()
    }
}

fn emit_status(app: &AppHandle, state: &Arc<Mutex<AppState>>) {
    let _ = app.emit("connector://status-changed", state.lock().status_snapshot());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_error_redacts_bearer_tokens() {
        assert_eq!(
            sanitize_error("Request failed: Bearer secret-token-value"),
            "Connection error — retrying"
        );
    }

    #[test]
    fn sanitize_error_redacts_overlong_messages() {
        let long = "x".repeat(301);
        assert_eq!(sanitize_error(&long), "Connection error — retrying");
    }

    #[test]
    fn sanitize_error_preserves_short_safe_messages() {
        assert_eq!(
            sanitize_error("Network unreachable"),
            "Network unreachable"
        );
    }
}
