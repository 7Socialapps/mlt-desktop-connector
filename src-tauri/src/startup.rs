use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::api::ConnectorApiClient;
use crate::browser::BrowserManager;
use crate::credentials;
use crate::services::{
    enable_polling_if_authenticated, ChromiumProvisionService, DeepLinkCoordinator, HeartbeatService,
    PollingService,
};
use crate::state::{AppState, ConnectionState};

/// Hard cap for browser manager init so setup never blocks the dealer UI forever.
const BROWSER_INIT_TIMEOUT: Duration = Duration::from_secs(45);
/// Credential restore network calls.
const CREDENTIAL_RESTORE_TIMEOUT: Duration = Duration::from_secs(20);

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
    /// Queue deferred init on the Tauri async runtime.
    pub fn spawn(self) {
        tauri::async_runtime::spawn(async move {
            self.run().await;
        });
    }

    pub async fn run(self) {
        startup_log("deferred: ui ready — background services starting");

        // Unblock dealer UI immediately — never leave connection_state=Starting
        // while browser/network work runs (that paints infinite "Setting up…").
        self.mark_shell_ready();

        self.restore_credentials().await;
        self.initialize_browser().await;
        self.start_chromium_provision();
        self.process_deep_links();
        self.finish_startup();
    }

    /// Leave Starting as soon as the window can paint Connected / Not connected.
    fn mark_shell_ready(&self) {
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
        startup_log("deferred: shell ready for UI (Starting cleared)");
        let _ = self.app.emit("connector://startup-ready", ());
        emit_status(&self.app, &self.state);
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
        let restore = credentials::ensure_access_token(client.as_ref());
        let result = match tokio::time::timeout(CREDENTIAL_RESTORE_TIMEOUT, restore).await {
            Ok(inner) => inner,
            Err(_) => {
                tracing::warn!("credential restore timed out");
                Err(anyhow::anyhow!("Credential restore timed out"))
            }
        };
        match result {
            Ok(true) => {
                let mut guard = self.state.lock();
                guard.paired = true;
                guard.needs_reconnect = false;
                guard.last_error = None;
                if !matches!(
                    guard.connection_state,
                    ConnectionState::Connected | ConnectionState::ShuttingDown
                ) {
                    guard.connection_state = ConnectionState::Idle;
                }
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
                drop(guard);
                emit_status(&self.app, &self.state);
            }
        }
    }

    async fn initialize_browser(&self) {
        startup_log("deferred: browser manager initialize (background thread)");
        let manager = self.browser_manager.clone();
        let app = self.app.clone();
        let init = tauri::async_runtime::spawn_blocking(move || manager.initialize(app));
        match tokio::time::timeout(BROWSER_INIT_TIMEOUT, init).await {
            Ok(Ok(Ok(()))) => startup_log("deferred: browser manager ready"),
            Ok(Ok(Err(err))) => {
                tracing::warn!(error = %err, "browser manager initialization failed");
                startup_log("deferred: browser manager failed (non-fatal)");
            }
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "browser manager init task failed");
            }
            Err(_) => {
                tracing::warn!(
                    timeout_secs = BROWSER_INIT_TIMEOUT.as_secs(),
                    "browser manager init timed out (non-fatal)"
                );
                startup_log("deferred: browser manager timed out (non-fatal)");
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

    fn finish_startup(&self) {
        startup_log("deferred: startup complete");
        let _ = self.app.emit("connector://startup-ready", ());
        emit_status(&self.app, &self.state);
        self.heartbeat.trigger_now();
    }
}

fn emit_status(app: &AppHandle, state: &Arc<Mutex<AppState>>) {
    let _ = app.emit("connector://status-changed", state.lock().status_snapshot());
}
