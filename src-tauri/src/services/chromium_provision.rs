use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use crate::browser::{
    run_sidecar_command, resolve_sidecar_cli, BrowserRuntimeService, SidecarSimpleResponse,
};

/// First-run Chromium download must never leave the UI on "Setting up…" forever.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(45);
/// Absolute cap on `active=true` even if the install task misbehaves.
const ACTIVE_UI_CAP: Duration = Duration::from_secs(50);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChromiumProvisionState {
    pub active: bool,
    pub progress: u8,
    pub message: String,
    pub error: Option<String>,
}

pub struct ChromiumProvisionService {
    runtime: Arc<BrowserRuntimeService>,
    state: Arc<Mutex<ChromiumProvisionState>>,
}

impl ChromiumProvisionService {
    pub fn new(runtime: Arc<BrowserRuntimeService>) -> Self {
        Self {
            runtime,
            state: Arc::new(Mutex::new(ChromiumProvisionState {
                active: false,
                progress: 0,
                message: "Checking browser components…".into(),
                error: None,
            })),
        }
    }

    pub fn snapshot(&self) -> ChromiumProvisionState {
        self.state.lock().clone()
    }

    /// Ensure Chromium is installed (dealer Open Facebook / Connect path).
    /// Returns Ok when ready; Err with a short dealer-facing message otherwise.
    pub async fn ensure_ready(self: &Arc<Self>, app: AppHandle) -> Result<(), String> {
        // If a first-run install is already running, wait for it (capped).
        if self.state.lock().active {
            let deadline = tokio::time::Instant::now() + ACTIVE_UI_CAP;
            while self.state.lock().active && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        }

        let runtime = self.runtime.clone();
        let detect = tauri::async_runtime::spawn_blocking(move || runtime.detect());
        let snap = match tokio::time::timeout(Duration::from_secs(15), detect).await {
            Ok(Ok(Ok(s))) => s,
            Ok(Ok(Err(err))) => {
                return Err(format!("Couldn’t check the Facebook browser: {err}"));
            }
            Ok(Err(err)) => {
                return Err(format!("Couldn’t check the Facebook browser: {err}"));
            }
            Err(_) => {
                return Err("Checking the Facebook browser timed out. Try Open Facebook again.".into());
            }
        };

        if !snap.enabled {
            return Err("Facebook helper is turned off on this computer. Contact support.".into());
        }
        if snap.chromium_installed {
            {
                let mut guard = self.state.lock();
                guard.active = false;
                guard.error = None;
                guard.message = "Chromium is ready.".into();
                guard.progress = 100;
            }
            emit(&app, self.snapshot());
            return Ok(());
        }
        if !snap.playwright_installed {
            let msg = "Browser components are missing from this install. Quit and reinstall the latest Desktop Connector.".to_string();
            self.fail(&app, msg.clone());
            return Err(msg);
        }

        self.run_install(app.clone()).await;

        let after = self.runtime.detect().unwrap_or_else(|_| self.runtime.snapshot());
        if after.chromium_installed {
            Ok(())
        } else {
            let msg = self
                .state
                .lock()
                .error
                .clone()
                .unwrap_or_else(|| {
                    "Couldn’t download the Facebook browser. Check your network, then try Open Facebook again."
                        .into()
                });
            Err(msg)
        }
    }

    pub fn start_if_needed(self: &Arc<Self>, app: AppHandle) {
        let runtime = self.runtime.clone();
        let svc = self.clone();
        tauri::async_runtime::spawn(async move {
            let detect = tauri::async_runtime::spawn_blocking(move || runtime.detect());
            let snap = match tokio::time::timeout(Duration::from_secs(15), detect).await {
                Ok(Ok(Ok(s))) => s,
                Ok(Ok(Err(err))) => {
                    warn!(error = %err, "chromium provision: detect failed");
                    return;
                }
                Ok(Err(err)) => {
                    warn!(error = %err, "chromium provision: detect task failed");
                    return;
                }
                Err(_) => {
                    warn!("chromium provision: detect timed out");
                    return;
                }
            };

            if snap.chromium_installed || !snap.enabled {
                return;
            }
            if !snap.playwright_installed {
                warn!("chromium provision skipped — playwright package missing from bundle");
                svc.fail(
                    &app,
                    "Browser components are missing from this install. Reinstall the Desktop Connector."
                        .into(),
                );
                return;
            }

            svc.run_install(app).await;
        });
    }

    async fn run_install(&self, app: AppHandle) {
        {
            let mut guard = self.state.lock();
            guard.active = true;
            guard.progress = 10;
            guard.message = "Downloading Chromium for Facebook posting…".into();
            guard.error = None;
        }
        emit(&app, self.snapshot());

        // Safety net: clear active even if install never returns.
        let watchdog_state = self.state.clone();
        let watchdog_app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(ACTIVE_UI_CAP).await;
            let mut guard = watchdog_state.lock();
            if guard.active {
                guard.active = false;
                guard.progress = 0;
                guard.message = "Setup is taking too long.".into();
                if guard.error.is_none() {
                    guard.error = Some(
                        "Browser setup timed out. You can still click Connect in MLT, then Try again."
                            .into(),
                    );
                }
                let snap = guard.clone();
                drop(guard);
                emit(&watchdog_app, snap);
            }
        });

        let cli_path = match resolve_sidecar_cli() {
            Ok(path) => path,
            Err(err) => {
                self.fail(&app, err.to_string());
                return;
            }
        };

        info!("starting first-run chromium provisioning");
        let install = tauri::async_runtime::spawn_blocking(move || {
            run_sidecar_command::<SidecarSimpleResponse>(&cli_path, "install-chromium")
        });

        let result = match tokio::time::timeout(INSTALL_TIMEOUT, install).await {
            Ok(join) => join,
            Err(_) => {
                warn!(
                    timeout_secs = INSTALL_TIMEOUT.as_secs(),
                    "chromium provisioning timed out"
                );
                self.fail(
                    &app,
                    "Chromium download timed out. Check your network, then Try again.".into(),
                );
                return;
            }
        };

        match result {
            Ok(Ok(resp)) if resp.ok => {
                let mut guard = self.state.lock();
                guard.active = false;
                guard.progress = 100;
                guard.message = "Chromium is ready.".into();
                guard.error = None;
                let snap = guard.clone();
                drop(guard);
                let _ = self.runtime.detect();
                emit(&app, snap);
                let _ = app.emit("connector://browser-changed", self.runtime.snapshot());
            }
            Ok(Ok(resp)) => {
                let msg = resp
                    .error
                    .unwrap_or_else(|| "Chromium download failed".into());
                self.fail(&app, msg);
            }
            Ok(Err(err)) => {
                warn!(error = %err, "chromium provisioning failed");
                self.fail(&app, err.to_string());
            }
            Err(err) => {
                warn!(error = %err, "chromium provisioning task failed");
                self.fail(&app, err.to_string());
            }
        }
    }

    fn fail(&self, app: &AppHandle, message: String) {
        let mut guard = self.state.lock();
        guard.active = false;
        guard.progress = 0;
        guard.message = "Browser setup didn’t finish.".into();
        // Keep dealer-facing text short; dump details only in logs.
        let short = if message.contains("ERR_MODULE_NOT_FOUND") || message.contains("playwright") {
            "Browser components are missing. Quit the app and reinstall the latest Desktop Connector."
                .into()
        } else if message.len() > 180 {
            "Browser setup failed. Try again, or reinstall the Desktop Connector.".into()
        } else {
            message
        };
        guard.error = Some(short);
        emit(app, self.snapshot());
    }
}

fn emit(app: &AppHandle, state: ChromiumProvisionState) {
    let _ = app.emit("connector://chromium-provision", state);
}
