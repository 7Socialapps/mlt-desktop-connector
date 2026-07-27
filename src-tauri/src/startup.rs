use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tracing::info;

use crate::api::ConnectorApiClient;
use crate::browser::BrowserManager;
use crate::credentials;
use crate::services::{
    enable_polling_if_authenticated, ChromiumProvisionService, DeepLinkCoordinator, HeartbeatService,
    PollingService,
};
use crate::state::{AppState, ConnectionState};

static STARTUP_INSTANT: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

pub fn mark_startup_begin() {
    let _ = STARTUP_INSTANT.set(Instant::now());
    startup_log("startup begin");
}

pub fn startup_log(phase: &str) {
    let elapsed_ms = STARTUP_INSTANT
        .get()
        .map(|t| t.elapsed().as_millis())
        .unwrap_or(0);
    info!(target: "startup", phase, elapsed_ms, "startup phase");
}

pub struct DeferredStartup {
    pub app: AppHandle,
    pub state: Arc<Mutex<AppState>>,
    pub api_client: Arc<ConnectorApiClient>,
    pub browser_manager: Arc<BrowserManager>,
    pub heartbeat: Arc<HeartbeatService>,
    pub polling: Arc<PollingService>,
    pub deep_link: Arc<DeepLinkCoordinator>,
    pub chromium_provision: Arc<ChromiumProvisionService>,
    pub paired: bool,
    pub needs_reconnect: bool,
}

impl DeferredStartup {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(async move {
            self.run().await;
        });
    }

    async fn run(self) {
        startup_log("deferred: ui ready — background services starting");

        self.restore_credentials().await;
        self.initialize_browser().await;
        self.start_chromium_provision();
        self.process_deep_links();
        self.mark_ready();
    }

    async fn restore_credentials(&self) {
        startup_log("deferred: credential restore");
        if !credentials::is_paired() {
            return;
        }
        if credentials::has_access_token() {
            return;
        }
        let client = self.api_client.clone();
        match credentials::ensure_access_token(client.as_ref()).await {
            Ok(true) => {
                let mut guard = self.state.lock();
                guard.paired = true;
                guard.needs_reconnect = false;
                guard.last_error = None;
                guard.connection_state = ConnectionState::Idle;
                drop(guard);
                emit_status(&self.app, &self.state);
            }
            Ok(false) => {
                let mut guard = self.state.lock();
                guard.paired = false;
                guard.needs_reconnect = true;
                if guard.last_error.is_none() {
                    guard.last_error = credentials::needs_reconnect_message();
                }
                guard.connection_state = ConnectionState::Offline;
                drop(guard);
                emit_status(&self.app, &self.state);
            }
            Err(err) => {
                tracing::warn!(error = %err, "credential bootstrap failed");
                credentials::mark_needs_reconnect(
                    "Reconnect device — stored credentials are unavailable. Start pairing again.",
                );
                let mut guard = self.state.lock();
                guard.paired = false;
                guard.needs_reconnect = true;
                guard.last_error = credentials::needs_reconnect_message();
                guard.connection_state = ConnectionState::Offline;
            }
        }
    }

    async fn initialize_browser(&self) {
        startup_log("deferred: browser manager initialize (background thread)");
        let manager = self.browser_manager.clone();
        let app = self.app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || manager.initialize(app))
            .await;
        match result {
            Ok(Ok(())) => startup_log("deferred: browser manager ready"),
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "browser manager initialization failed");
                startup_log("deferred: browser manager failed (non-fatal)");
            }
            Err(err) => {
                tracing::warn!(error = %err, "browser manager init task failed");
            }
        }
        let _ = self.app.emit(
            "connector://browser-changed",
            self.browser_manager.get_status(),
        );
    }

    fn start_chromium_provision(&self) {
        startup_log("deferred: chromium provision check");
        self.chromium_provision.start_if_needed(self.app.clone());
    }

    fn process_deep_links(&self) {
        startup_log("deferred: deep link queue");
        self.deep_link.drain_pending();
    }

    fn mark_ready(&self) {
        {
            let mut guard = self.state.lock();
            if guard.connection_state == ConnectionState::Starting {
                if self.paired && !self.needs_reconnect {
                    enable_polling_if_authenticated(self.polling.as_ref(), &mut guard);
                    guard.connection_state = ConnectionState::Idle;
                } else {
                    guard.connection_state = ConnectionState::Offline;
                }
            }
        }
        startup_log("deferred: startup complete");
        let _ = self.app.emit("connector://startup-ready", ());
        emit_status(&self.app, &self.state);
        self.heartbeat.trigger_now();
    }
}

fn emit_status(app: &AppHandle, state: &Arc<Mutex<AppState>>) {
    let _ = app.emit("connector://status-changed", state.lock().status_snapshot());
}
