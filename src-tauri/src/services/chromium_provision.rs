use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use crate::browser::{run_sidecar_command, resolve_sidecar_cli, BrowserRuntimeService, SidecarSimpleResponse};

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

    pub fn start_if_needed(self: &Arc<Self>, app: AppHandle) {
        let _ = self.runtime.detect();
        let snap = self.runtime.snapshot();
        if snap.chromium_installed || !snap.enabled {
            return;
        }

        let svc = self.clone();
        tauri::async_runtime::spawn(async move {
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

        let cli_path = match resolve_sidecar_cli() {
            Ok(path) => path,
            Err(err) => {
                self.fail(&app, err.to_string());
                return;
            }
        };

        info!("starting first-run chromium provisioning");
        let result = tauri::async_runtime::spawn_blocking(move || {
            run_sidecar_command::<SidecarSimpleResponse>(&cli_path, "install-chromium")
        })
        .await;

        match result {
            Ok(Ok(resp)) if resp.ok => {
                let mut guard = self.state.lock();
                guard.active = false;
                guard.progress = 100;
                guard.message = "Chromium is ready.".into();
                let _ = self.runtime.detect();
                emit(&app, self.snapshot());
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
        guard.message = "Chromium download failed.".into();
        guard.error = Some(message);
        emit(app, self.snapshot());
    }
}

fn emit(app: &AppHandle, state: ChromiumProvisionState) {
    let _ = app.emit("connector://chromium-provision", state);
}
