use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::AppHandle;
use tracing::{debug, info};

use crate::credentials::has_access_token;
use crate::state::{AppState, ConnectionState};

/// Job polling framework — disabled until device authentication succeeds.
pub struct PollingService {
    enabled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

impl PollingService {
    pub fn spawn(app: AppHandle, state: Arc<Mutex<AppState>>) -> Arc<Self> {
        let service = Arc::new(Self {
            enabled: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
        });

        let enabled_flag = service.enabled.clone();
        let shutdown_flag = service.shutdown.clone();

        tauri::async_runtime::spawn(async move {
            loop {
                if shutdown_flag.load(Ordering::SeqCst) {
                    info!("polling loop stopped");
                    break;
                }

                if !enabled_flag.load(Ordering::SeqCst) || !has_access_token() {
                    debug!("polling disabled — waiting for authentication");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }

                // Milestone A: stub only — actual poll_jobs wired in Milestone B.
                debug!("polling tick (stub)");
                let _ = &app;
                let _ = &state;

                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        });

        service
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        if enabled {
            info!("polling enabled after authentication");
        } else {
            info!("polling disabled");
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
