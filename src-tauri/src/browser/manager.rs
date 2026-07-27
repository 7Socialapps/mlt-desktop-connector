use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use super::runtime::BrowserRuntimeService;
use super::sidecar::{SidecarDaemon, SidecarEvent};
use super::types::{
    BrowserActivePage, BrowserManagerSnapshot, BrowserRuntimeStatus, SidecarPageResult,
    SidecarStatusResult, MAX_AUTO_RESTART_ATTEMPTS,
};
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const BASE_BACKOFF_MS: u64 = 1_000;
const MAX_BACKOFF_MS: u64 = 30_000;

pub fn restart_backoff_ms(attempt: u32) -> u64 {
    if attempt == 0 {
        return 0;
    }
    let shift = attempt.saturating_sub(1).min(5);
    let backoff = BASE_BACKOFF_MS.saturating_mul(1_u64 << shift);
    backoff.min(MAX_BACKOFF_MS)
}

pub fn should_enter_terminal_error(attempts: u32) -> bool {
    attempts >= MAX_AUTO_RESTART_ATTEMPTS
}

pub fn status_after_crash(
    current: BrowserRuntimeStatus,
    restart_attempts: u32,
    auto_restart_enabled: bool,
) -> BrowserRuntimeStatus {
    if !auto_restart_enabled || should_enter_terminal_error(restart_attempts) {
        BrowserRuntimeStatus::BrowserError
    } else if current == BrowserRuntimeStatus::BrowserRestarting {
        BrowserRuntimeStatus::BrowserRestarting
    } else {
        BrowserRuntimeStatus::BrowserCrashed
    }
}

pub struct BrowserManager {
    runtime: Arc<BrowserRuntimeService>,
    daemon: Arc<SidecarDaemon>,
    state: Arc<Mutex<BrowserManagerSnapshot>>,
    lifecycle_lock: Arc<Mutex<()>>,
    app_handle: Mutex<Option<AppHandle>>,
    monitor_started: Mutex<bool>,
}

impl BrowserManager {
    pub fn new(runtime: Arc<BrowserRuntimeService>, daemon: Arc<SidecarDaemon>) -> Self {
        let snapshot = BrowserManagerSnapshot::from_runtime(&runtime.snapshot());
        Self {
            runtime,
            daemon,
            state: Arc::new(Mutex::new(snapshot)),
            lifecycle_lock: Arc::new(Mutex::new(())),
            app_handle: Mutex::new(None),
            monitor_started: Mutex::new(false),
        }
    }

    pub fn initialize(&self, app_handle: AppHandle) -> Result<(), String> {
        *self.app_handle.lock() = Some(app_handle);

        if !self.state.lock().enabled {
            self.set_status(BrowserRuntimeStatus::BrowserStopped);
            self.emit_changed();
            return Ok(());
        }

        if self.runtime.cli_path().as_os_str().is_empty() {
            self.record_error(
                "Browser sidecar CLI not found",
                Some("SIDECAR_NOT_FOUND".into()),
            );
            self.set_status(BrowserRuntimeStatus::BrowserError);
            self.emit_changed();
            return Ok(());
        }

        let detect = self.runtime.detect().map_err(|e| e.to_string())?;
        self.merge_runtime(&detect);

        if self.daemon.server_path().as_os_str().is_empty() {
            self.record_error(
                "Browser sidecar server not found",
                Some("SIDECAR_SERVER_NOT_FOUND".into()),
            );
            self.set_status(BrowserRuntimeStatus::BrowserError);
            self.emit_changed();
            return Ok(());
        }

        if let Err(err) = self.daemon.start() {
            warn!(error = %err, "failed to start browser sidecar daemon");
            self.record_error("Failed to start browser sidecar", Some("SIDECAR_START_FAILED".into()));
            self.set_status(BrowserRuntimeStatus::BrowserError);
            self.emit_changed();
            return Ok(());
        }

        if self
            .daemon
            .request("ping", serde_json::json!({}), HEALTH_CHECK_TIMEOUT)
            .is_err()
        {
            warn!("browser sidecar daemon did not respond to initial ping");
            self.record_error(
                "Browser sidecar failed readiness check",
                Some("SIDECAR_NOT_READY".into()),
            );
            self.set_status(BrowserRuntimeStatus::BrowserError);
            self.emit_changed();
            return Ok(());
        }

        {
            let mut guard = self.state.lock();
            guard.sidecar_running = true;
        }
        self.start_event_monitor();
        self.emit_changed();
        Ok(())
    }

    pub fn snapshot(&self) -> BrowserManagerSnapshot {
        self.state.lock().clone()
    }

    pub fn get_status(&self) -> BrowserManagerSnapshot {
        self.snapshot()
    }

    pub fn launch(&self) -> Result<BrowserManagerSnapshot, String> {
        let _guard = self.lifecycle_lock.lock();
        self.launch_inner(false)
    }

    pub fn stop(&self) -> Result<BrowserManagerSnapshot, String> {
        let _guard = self.lifecycle_lock.lock();
        self.stop_inner(false)
    }

    pub fn restart(&self) -> Result<BrowserManagerSnapshot, String> {
        let _guard = self.lifecycle_lock.lock();
        self.restart_inner(false)
    }

    pub fn health_check(&self) -> Result<BrowserManagerSnapshot, String> {
        if !self.state.lock().enabled {
            return Ok(self.snapshot());
        }

        if !self.daemon.is_running() {
            self.record_error("Sidecar daemon not running", Some("SIDECAR_NOT_RUNNING".into()));
            self.set_status(BrowserRuntimeStatus::BrowserError);
            self.emit_changed();
            return Ok(self.snapshot());
        }

        match self
            .daemon
            .request("ping", serde_json::json!({}), HEALTH_CHECK_TIMEOUT)
        {
            Ok(line) => {
                let mut guard = self.state.lock();
                guard.last_health_check_at = Some(Utc::now().to_rfc3339());
                guard.sidecar_running = true;
                if let Some(result) = line.result {
                    if let Ok(status) = serde_json::from_value::<SidecarStatusResult>(result) {
                        self.apply_sidecar_status(&mut guard, &status);
                    }
                }
                drop(guard);
                self.emit_changed();
                Ok(self.snapshot())
            }
            Err(err) => {
                warn!(error = %err, "browser health check failed");
                self.record_error("Browser health check failed", Some("HEALTH_CHECK_FAILED".into()));
                {
                    let mut guard = self.state.lock();
                    guard.last_health_check_at = Some(Utc::now().to_rfc3339());
                }
                self.emit_changed();
                Ok(self.snapshot())
            }
        }
    }

    pub fn get_active_page(&self) -> Result<BrowserActivePage, String> {
        if self.snapshot().status != BrowserRuntimeStatus::BrowserReady {
            return Err("Browser is not ready".into());
        }

        let line = self
            .daemon
            .request(
                "get_active_page",
                serde_json::json!({}),
                REQUEST_TIMEOUT,
            )
            .map_err(|e| e.to_string())?;

        let result = line
            .result
            .ok_or_else(|| "Missing active page result".to_string())?;
        let page: SidecarPageResult =
            serde_json::from_value(result).map_err(|e| e.to_string())?;

        {
            let mut guard = self.state.lock();
            guard.active_page_url = Some(page.url.clone());
            guard.active_page_title = Some(page.title.clone());
        }

        Ok(BrowserActivePage {
            url: page.url,
            title: page.title,
            pid: page.pid,
        })
    }

    pub fn ensure_browser_ready(&self) -> Result<BrowserManagerSnapshot, String> {
        let _guard = self.lifecycle_lock.lock();
        let status = self.snapshot().status;

        if !self.state.lock().enabled {
            return Err("Browser subsystem disabled".into());
        }

        if status.is_terminal_error() {
            return Err(self
                .snapshot()
                .last_error
                .clone()
                .unwrap_or_else(|| "Browser is in terminal error state".into()));
        }

        match status {
            BrowserRuntimeStatus::BrowserReady => Ok(self.snapshot()),
            BrowserRuntimeStatus::BrowserStarting | BrowserRuntimeStatus::BrowserRestarting => {
                thread::sleep(Duration::from_millis(250));
                if self.snapshot().status == BrowserRuntimeStatus::BrowserReady {
                    Ok(self.snapshot())
                } else {
                    Err("Browser is still starting".into())
                }
            }
            BrowserRuntimeStatus::BrowserNotInstalled => {
                Err("Chromium is not installed".into())
            }
            _ => self.launch_inner(false),
        }
    }

    pub fn shutdown(&self) {
        let _guard = self.lifecycle_lock.lock();
        let _ = self.stop_inner(true);
        if self.daemon.is_running() {
            let _ = self.daemon.stop();
        }
        {
            let mut guard = self.state.lock();
            guard.sidecar_running = false;
            guard.auto_restart_enabled = false;
        }
        self.set_status(BrowserRuntimeStatus::BrowserStopped);
        self.emit_changed();
        info!("browser manager shutdown complete");
    }

    fn launch_inner(&self, auto: bool) -> Result<BrowserManagerSnapshot, String> {
        if !self.state.lock().enabled {
            return Err("Browser subsystem disabled".into());
        }

        let status = self.snapshot().status;
        if status == BrowserRuntimeStatus::BrowserReady {
            return Ok(self.snapshot());
        }
        if status.is_terminal_error() {
            return Err(self
                .snapshot()
                .last_error
                .clone()
                .unwrap_or_else(|| "Browser is in terminal error state".into()));
        }
        if !self.state.lock().chromium_installed {
            return Err("Chromium is not installed".into());
        }
        if !self.daemon.is_running() {
            return Err("Browser sidecar daemon is not running".into());
        }

        self.set_status(BrowserRuntimeStatus::BrowserStarting);
        self.clear_error();
        self.emit_changed();

        match self
            .daemon
            .request("launch", serde_json::json!({}), REQUEST_TIMEOUT)
        {
            Ok(line) => {
                if let Some(result) = line.result {
                    if let Ok(sidecar_status) =
                        serde_json::from_value::<SidecarStatusResult>(result)
                    {
                        let mut guard = self.state.lock();
                        self.apply_sidecar_status(&mut guard, &sidecar_status);
                        if sidecar_status.browser_state.as_deref() == Some("ready") {
                            guard.status = BrowserRuntimeStatus::BrowserReady;
                            if auto {
                                guard.restart_attempts = 0;
                            }
                        }
                    } else {
                        self.set_status(BrowserRuntimeStatus::BrowserReady);
                    }
                } else {
                    self.set_status(BrowserRuntimeStatus::BrowserReady);
                }
                self.emit_changed();
                Ok(self.snapshot())
            }
            Err(err) => {
                warn!(error = %err, auto, "browser launch failed");
                self.handle_launch_failure(&err.to_string(), if auto { "AUTO_LAUNCH_FAILED" } else { "LAUNCH_FAILED" });
                Err(err.to_string())
            }
        }
    }

    fn stop_inner(&self, shutting_down: bool) -> Result<BrowserManagerSnapshot, String> {
        if !self.daemon.is_running() {
            self.set_status(BrowserRuntimeStatus::BrowserStopped);
            self.emit_changed();
            return Ok(self.snapshot());
        }

        if shutting_down {
            let mut guard = self.state.lock();
            guard.auto_restart_enabled = false;
        }

        match self
            .daemon
            .request("stop", serde_json::json!({}), REQUEST_TIMEOUT)
        {
            Ok(_) => {
                {
                    let mut guard = self.state.lock();
                    guard.browser_pid = None;
                    guard.active_page_url = None;
                    guard.active_page_title = None;
                }
                self.set_status(BrowserRuntimeStatus::BrowserStopped);
                self.emit_changed();
                Ok(self.snapshot())
            }
            Err(err) => {
                warn!(error = %err, "browser stop failed");
                self.record_error("Failed to stop browser", Some("STOP_FAILED".into()));
                self.emit_changed();
                Err(err.to_string())
            }
        }
    }

    fn restart_inner(&self, auto: bool) -> Result<BrowserManagerSnapshot, String> {
        if !self.state.lock().enabled {
            return Err("Browser subsystem disabled".into());
        }
        if self.snapshot().status.is_terminal_error() {
            return Err("Browser is in terminal error state".into());
        }

        self.set_status(BrowserRuntimeStatus::BrowserRestarting);
        self.emit_changed();

        let _ = self.daemon.request("stop", serde_json::json!({}), REQUEST_TIMEOUT);

        if auto {
            let attempts = {
                let mut guard = self.state.lock();
                guard.restart_attempts = guard.restart_attempts.saturating_add(1);
                guard.restart_attempts
            };
            let delay = restart_backoff_ms(attempts);
            if delay > 0 {
                thread::sleep(Duration::from_millis(delay));
            }
        }

        self.launch_inner(auto)
    }

    fn handle_launch_failure(&self, message: &str, code: &str) {
        let attempts = self.state.lock().restart_attempts;
        if should_enter_terminal_error(attempts) {
            self.record_error(
                "Browser failed repeatedly; manual intervention required",
                Some("BROWSER_TERMINAL_ERROR".into()),
            );
            self.set_status(BrowserRuntimeStatus::BrowserError);
        } else {
            self.record_error(message, Some(code.into()));
            self.set_status(BrowserRuntimeStatus::BrowserError);
        }
        self.emit_changed();
    }

    fn handle_unexpected_disconnect(&self) {
        let (attempts, auto_restart) = {
            let mut guard = self.state.lock();
            guard.browser_pid = None;
            guard.active_page_url = None;
            guard.active_page_title = None;
            let attempts = guard.restart_attempts;
            let auto_restart = guard.auto_restart_enabled;
            guard.status = status_after_crash(guard.status, attempts, auto_restart);
            (attempts, auto_restart)
        };
        self.emit_changed();

        warn!(
            restart_attempts = attempts,
            auto_restart,
            "browser disconnected unexpectedly"
        );

        if !auto_restart || should_enter_terminal_error(attempts) {
            self.record_error(
                "Browser crashed repeatedly; automatic restart limit reached",
                Some("BROWSER_CRASH_LIMIT".into()),
            );
            self.set_status(BrowserRuntimeStatus::BrowserError);
            self.emit_changed();
            return;
        }

        self.set_status(BrowserRuntimeStatus::BrowserRestarting);
        self.emit_changed();

        let manager = self.clone_for_background();
        thread::spawn(move || {
            let delay = restart_backoff_ms(attempts.saturating_add(1));
            if delay > 0 {
                thread::sleep(Duration::from_millis(delay));
            }
            let _guard = manager.lifecycle_lock.lock();
            let _ = manager.restart_inner(true);
        });
    }

    fn start_event_monitor(&self) {
        let mut started = self.monitor_started.lock();
        if *started {
            return;
        }
        *started = true;

        let Some(rx) = self.daemon.take_event_receiver() else {
            return;
        };

        let manager = self.clone_for_background();
        thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                match event {
                    SidecarEvent::BrowserStarting => {
                        manager.set_status(BrowserRuntimeStatus::BrowserStarting);
                        manager.emit_changed();
                    }
                    SidecarEvent::BrowserReady { pid } => {
                        {
                            let mut guard = manager.state.lock();
                            guard.browser_pid = pid;
                            guard.status = BrowserRuntimeStatus::BrowserReady;
                        }
                        manager.emit_changed();
                    }
                    SidecarEvent::BrowserStopped { .. } => {
                        {
                            let mut guard = manager.state.lock();
                            guard.browser_pid = None;
                            guard.active_page_url = None;
                            guard.active_page_title = None;
                            if guard.status != BrowserRuntimeStatus::BrowserRestarting {
                                guard.status = BrowserRuntimeStatus::BrowserStopped;
                            }
                        }
                        manager.emit_changed();
                    }
                    SidecarEvent::BrowserDisconnected { .. } => {
                        manager.handle_unexpected_disconnect();
                    }
                    SidecarEvent::DaemonShutdown => {
                        {
                            let mut guard = manager.state.lock();
                            guard.sidecar_running = false;
                        }
                        manager.emit_changed();
                    }
                    SidecarEvent::Ready => {}
                }
            }
        });
    }

    fn clone_for_background(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            daemon: self.daemon.clone(),
            state: self.state.clone(),
            lifecycle_lock: self.lifecycle_lock.clone(),
            app_handle: Mutex::new(self.app_handle.lock().clone()),
            monitor_started: Mutex::new(true),
        }
    }

    fn merge_runtime(&self, runtime: &super::types::BrowserRuntimeSnapshot) {
        let mut guard = self.state.lock();
        guard.enabled = runtime.enabled;
        guard.playwright_installed = runtime.playwright_installed;
        guard.playwright_version = runtime.playwright_version.clone();
        guard.chromium_installed = runtime.chromium_installed;
        guard.chromium_path = runtime.chromium_path.clone();
        guard.node_version = runtime.node_version.clone();
        guard.checked_at = runtime.checked_at.clone();
        if runtime.last_error.is_some() {
            guard.last_error = runtime.last_error.clone();
            guard.last_error_code = runtime.last_error_code.clone();
        }
        if !runtime.status.is_operational() && guard.status == BrowserRuntimeStatus::BrowserStopped
        {
            guard.status = runtime.status;
        }
    }

    fn apply_sidecar_status(&self, guard: &mut BrowserManagerSnapshot, status: &SidecarStatusResult) {
        guard.browser_pid = status.pid;
        match status.browser_state.as_deref() {
            Some("ready") => guard.status = BrowserRuntimeStatus::BrowserReady,
            Some("starting") => guard.status = BrowserRuntimeStatus::BrowserStarting,
            Some("crashed") => guard.status = BrowserRuntimeStatus::BrowserCrashed,
            Some("stopped") | None => {
                if guard.status != BrowserRuntimeStatus::BrowserRestarting {
                    guard.status = BrowserRuntimeStatus::BrowserStopped;
                }
            }
            _ => {}
        }
    }

    fn set_status(&self, status: BrowserRuntimeStatus) {
        self.state.lock().status = status;
    }

    fn record_error(&self, message: &str, code: Option<String>) {
        let mut guard = self.state.lock();
        guard.last_error = Some(message.to_string());
        guard.last_error_code = code;
    }

    fn clear_error(&self) {
        let mut guard = self.state.lock();
        guard.last_error = None;
        guard.last_error_code = None;
    }

    fn emit_changed(&self) {
        if let Some(app) = self.app_handle.lock().as_ref() {
            let snapshot = self.snapshot();
            let _ = app.emit("connector://browser-changed", snapshot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_backoff_is_exponential_with_cap() {
        assert_eq!(restart_backoff_ms(0), 0);
        assert_eq!(restart_backoff_ms(1), 1_000);
        assert_eq!(restart_backoff_ms(2), 2_000);
        assert_eq!(restart_backoff_ms(3), 4_000);
        assert_eq!(restart_backoff_ms(4), 8_000);
        assert_eq!(restart_backoff_ms(10), MAX_BACKOFF_MS);
    }

    #[test]
    fn status_after_crash_before_limit_schedules_restart() {
        assert_eq!(
            status_after_crash(BrowserRuntimeStatus::BrowserReady, 0, true),
            BrowserRuntimeStatus::BrowserCrashed
        );
        assert_eq!(
            status_after_crash(BrowserRuntimeStatus::BrowserReady, 4, true),
            BrowserRuntimeStatus::BrowserCrashed
        );
    }

    #[test]
    fn launch_failure_enters_terminal_error_at_limit() {
        let attempts = MAX_AUTO_RESTART_ATTEMPTS;
        assert!(should_enter_terminal_error(attempts));
    }

    #[test]
    fn status_after_crash_respects_auto_restart() {
        assert_eq!(
            status_after_crash(BrowserRuntimeStatus::BrowserReady, 1, true),
            BrowserRuntimeStatus::BrowserCrashed
        );
        assert_eq!(
            status_after_crash(BrowserRuntimeStatus::BrowserReady, MAX_AUTO_RESTART_ATTEMPTS, true),
            BrowserRuntimeStatus::BrowserError
        );
        assert_eq!(
            status_after_crash(BrowserRuntimeStatus::BrowserReady, 1, false),
            BrowserRuntimeStatus::BrowserError
        );
    }

    #[test]
    fn lifecycle_lock_serializes_concurrent_access() {
        let lock = Arc::new(Mutex::new(()));
        let lock_a = lock.clone();
        let lock_b = lock.clone();

        let t1 = thread::spawn(move || {
            let _g = lock_a.lock();
            thread::sleep(Duration::from_millis(50));
            true
        });
        thread::sleep(Duration::from_millis(10));
        let start = std::time::Instant::now();
        let _g = lock_b.lock();
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(30));
        assert!(t1.join().unwrap());
    }
}
