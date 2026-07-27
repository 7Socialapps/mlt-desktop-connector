use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::api::ConnectorApiClient;
use crate::credentials::{has_access_token, load_credentials};
use crate::services::heartbeat::HeartbeatService;
use crate::state::{AppState, ConnectionState};

/// Automatic reconnect framework — stub for Milestone A.
pub struct ReconnectService;

impl ReconnectService {
    pub fn on_network_restored(
        app: &AppHandle,
        state: &Arc<Mutex<AppState>>,
        heartbeat: &HeartbeatService,
    ) {
        info!("reconnect: network restored — triggering heartbeat");
        {
            let mut guard = state.lock();
            guard.connection_state = ConnectionState::Reconnecting;
        }
        let _ = app.emit("connector://status-changed", state.lock().status_snapshot());
        heartbeat.trigger_now();
    }

    pub fn on_system_resume(
        app: &AppHandle,
        state: &Arc<Mutex<AppState>>,
        heartbeat: &HeartbeatService,
    ) {
        info!("reconnect: system resumed from sleep — triggering reconnect");
        Self::on_network_restored(app, state, heartbeat);
    }

    pub async fn try_refresh_tokens(client: &ConnectorApiClient) -> bool {
        if !has_access_token() {
            return false;
        }

        let Some(creds) = load_credentials().ok().flatten() else {
            return false;
        };

        match client.authenticate_device(&creds.refresh_token).await {
            Ok(resp) if resp.ok => {
                let updated = crate::credentials::StoredCredentials {
                    access_token: resp.access_token,
                    refresh_token: resp.refresh_token,
                    user_id: creds.user_id,
                    dealership_id: creds.dealership_id,
                };
                if crate::credentials::store_credentials(&updated).is_ok() {
                    info!("reconnect: refreshed device tokens");
                    return true;
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "reconnect: token refresh failed");
            }
            _ => {}
        }

        false
    }
}
