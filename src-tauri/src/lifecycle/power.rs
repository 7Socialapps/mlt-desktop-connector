use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::AppHandle;

use crate::services::{HeartbeatService, ReconnectService};
use crate::state::AppState;

const RESUME_GAP_THRESHOLD: Duration = Duration::from_secs(45);
const POWER_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Detects sleep/resume via monotonic clock gaps and triggers reconnect.
pub fn spawn_sleep_resume_monitor(
    app: AppHandle,
    state: Arc<Mutex<AppState>>,
    heartbeat: Arc<HeartbeatService>,
) {
    tauri::async_runtime::spawn(async move {
        let mut last_tick = Instant::now();

        loop {
            tokio::time::sleep(POWER_POLL_INTERVAL).await;

            let elapsed = last_tick.elapsed();
            last_tick = Instant::now();

            if elapsed > RESUME_GAP_THRESHOLD + POWER_POLL_INTERVAL {
                ReconnectService::on_system_resume(&app, &state, heartbeat.as_ref());
            }
        }
    });
}
