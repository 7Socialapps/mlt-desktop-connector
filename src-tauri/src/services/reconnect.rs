use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::api::ConnectorApiClient;
use crate::credentials::{ensure_access_token, has_access_token, is_paired};
use crate::services::heartbeat::HeartbeatService;
use crate::state::{AppState, ConnectionState};

/// Automatic reconnect framework.
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
        if !is_paired() {
            return false;
        }

        if has_access_token() {
            return true;
        }

        ensure_access_token(client).await.unwrap_or(false)
    }
}
