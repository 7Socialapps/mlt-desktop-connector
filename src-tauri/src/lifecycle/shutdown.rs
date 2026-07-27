use std::sync::Arc;

use parking_lot::Mutex;

use crate::browser::BrowserManager;
use crate::services::{HeartbeatService, PollingService};

static SHUTTING_DOWN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(std::sync::atomic::Ordering::SeqCst)
}

pub struct ShutdownCoordinator {
    heartbeat: Arc<HeartbeatService>,
    polling: Arc<PollingService>,
    browser: Arc<BrowserManager>,
}

impl ShutdownCoordinator {
    pub fn new(
        heartbeat: Arc<HeartbeatService>,
        polling: Arc<PollingService>,
        browser: Arc<BrowserManager>,
    ) -> Self {
        Self {
            heartbeat,
            polling,
            browser,
        }
    }

    pub fn graceful_shutdown(&self, state: &Arc<Mutex<crate::state::AppState>>) {
        if SHUTTING_DOWN.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }

        tracing::info!("graceful shutdown initiated");
        state.lock().connection_state = crate::state::ConnectionState::ShuttingDown;

        self.browser.shutdown();
        self.heartbeat.stop();
        self.polling.stop();

        tracing::info!("background services stopped");
    }
}
