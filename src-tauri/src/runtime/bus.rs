use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::browser::BrowserManager;
use crate::browser::SidecarDaemonLine;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NavigationDiagnostics {
    pub current_url: Option<String>,
    pub navigation_target: Option<String>,
    pub last_successful_url: Option<String>,
    pub navigation_started_at: Option<String>,
    pub navigation_completed_at: Option<String>,
    pub navigation_failure_reason: Option<String>,
    pub timeout_reason: Option<String>,
    pub current_destination: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeServiceKind {
    Session,
    Navigation,
    Marketplace,
    Messenger,
    Notifications,
    Recovery,
}

impl RuntimeServiceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Navigation => "navigation",
            Self::Marketplace => "marketplace",
            Self::Messenger => "messenger",
            Self::Notifications => "notifications",
            Self::Recovery => "recovery",
        }
    }
}

pub struct ServiceBus {
    browser_manager: Arc<BrowserManager>,
    access_lock: Mutex<()>,
    current_service: Mutex<Option<RuntimeServiceKind>>,
    cancel_requested: Arc<AtomicBool>,
    last_navigation_error: Mutex<Option<String>>,
    last_destination: Mutex<Option<String>>,
    navigation_diagnostics: Mutex<NavigationDiagnostics>,
}

impl ServiceBus {
    pub fn new(browser_manager: Arc<BrowserManager>) -> Self {
        Self {
            browser_manager,
            access_lock: Mutex::new(()),
            current_service: Mutex::new(None),
            cancel_requested: Arc::new(AtomicBool::new(false)),
            last_navigation_error: Mutex::new(None),
            last_destination: Mutex::new(None),
            navigation_diagnostics: Mutex::new(NavigationDiagnostics::default()),
        }
    }

    pub fn request_cancel(&self) {
        self.cancel_requested.store(true, Ordering::SeqCst);
    }

    pub fn clear_cancel(&self) {
        self.cancel_requested.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_requested.load(Ordering::SeqCst)
    }

    pub fn record_navigation_success(&self, destination: &str) {
        *self.last_destination.lock() = Some(destination.to_string());
        *self.last_navigation_error.lock() = None;
        let mut diag = self.navigation_diagnostics.lock();
        diag.current_destination = Some(destination.to_string());
        diag.navigation_failure_reason = None;
        diag.timeout_reason = None;
    }

    pub fn begin_navigation(&self, destination: &str, target_url: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let mut diag = self.navigation_diagnostics.lock();
        diag.navigation_target = Some(target_url.to_string());
        diag.current_destination = Some(destination.to_string());
        diag.navigation_started_at = Some(now);
        diag.navigation_completed_at = None;
        diag.navigation_failure_reason = None;
        diag.timeout_reason = None;
    }

    pub fn complete_navigation(&self, destination: &str, current_url: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let mut diag = self.navigation_diagnostics.lock();
        diag.current_url = Some(current_url.to_string());
        diag.last_successful_url = Some(current_url.to_string());
        diag.current_destination = Some(destination.to_string());
        diag.navigation_completed_at = Some(now);
        diag.navigation_failure_reason = None;
        diag.timeout_reason = None;
    }

    pub fn fail_navigation(&self, destination: &str, error: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let truncated = truncate_error(error);
        let timeout_reason = if truncated.to_lowercase().contains("timeout") {
            Some(truncated.clone())
        } else {
            None
        };
        let mut diag = self.navigation_diagnostics.lock();
        diag.current_destination = Some(destination.to_string());
        diag.navigation_completed_at = Some(now);
        diag.navigation_failure_reason = Some(truncated.clone());
        diag.timeout_reason = timeout_reason;
    }

    pub fn navigation_diagnostics(&self) -> NavigationDiagnostics {
        self.navigation_diagnostics.lock().clone()
    }

    pub fn set_navigation_current_url(&self, url: Option<String>) {
        self.navigation_diagnostics.lock().current_url = url;
    }

    pub fn record_navigation_error(&self, destination: &str, error: &str) {
        *self.last_destination.lock() = Some(destination.to_string());
        *self.last_navigation_error.lock() = Some(truncate_error(error));
        self.fail_navigation(destination, error);
    }

    pub fn last_navigation_error(&self) -> Option<String> {
        self.last_navigation_error.lock().clone()
    }

    pub fn last_destination(&self) -> Option<String> {
        self.last_destination.lock().clone()
    }

    fn check_cancel(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("Runtime operation cancelled".into())
        } else {
            Ok(())
        }
    }

    pub fn browser_manager(&self) -> &Arc<BrowserManager> {
        &self.browser_manager
    }

    pub fn current_service(&self) -> Option<RuntimeServiceKind> {
        *self.current_service.lock()
    }

    pub fn current_service_name(&self) -> Option<String> {
        self.current_service()
            .map(|kind| kind.as_str().to_string())
    }

    fn set_current_service(&self, kind: RuntimeServiceKind) {
        *self.current_service.lock() = Some(kind);
    }

    fn clear_current_service(&self) {
        *self.current_service.lock() = None;
    }

    pub fn with_service<T, F>(&self, kind: RuntimeServiceKind, f: F) -> Result<T, String>
    where
        F: FnOnce(&Arc<BrowserManager>) -> Result<T, String>,
    {
        self.check_cancel()?;
        let _guard = self.access_lock.lock();
        self.check_cancel()?;
        self.set_current_service(kind);
        debug!(service = kind.as_str(), "runtime coordinator acquired browser lock");
        let result = f(&self.browser_manager);
        self.clear_current_service();
        self.clear_cancel();
        result
    }

    pub fn sidecar_request(
        &self,
        kind: RuntimeServiceKind,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<SidecarDaemonLine, String> {
        self.with_service(kind, |manager| manager.sidecar_request(method, params, timeout))
    }

    pub fn ensure_browser_ready(&self, kind: RuntimeServiceKind) -> Result<(), String> {
        self.with_service(kind, |manager| {
            manager.ensure_browser_for_navigation()?;
            Ok(())
        })
    }

    pub fn record_health_check(&self) {
        self.browser_manager.touch_health_check();
    }

    pub fn record_restart(&self) {
        self.browser_manager.touch_restart();
    }
}

fn truncate_error(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() > 240 {
        format!("{}…", &trimmed[..240])
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserRuntimeService, SidecarDaemon};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_bus() -> Arc<ServiceBus> {
        let runtime = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(PathBuf::new()));
        let manager = Arc::new(BrowserManager::new(runtime, daemon));
        Arc::new(ServiceBus::new(manager))
    }

    #[test]
    fn routes_service_through_browser_manager() {
        let bus = test_bus();
        assert!(bus.current_service().is_none());
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let result = bus.with_service(RuntimeServiceKind::Navigation, |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(bus.current_service().is_none());
    }

    #[test]
    fn serializes_concurrent_service_access() {
        let bus = test_bus();
        let bus_a = bus.clone();
        let bus_b = bus.clone();
        let t1 = std::thread::spawn(move || {
            bus_a.with_service(RuntimeServiceKind::Marketplace, |_| {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok(())
            })
        });
        std::thread::sleep(std::time::Duration::from_millis(10));
        let start = std::time::Instant::now();
        let _ = bus_b.with_service(RuntimeServiceKind::Messenger, |_| Ok(()));
        let elapsed = start.elapsed();
        assert!(elapsed >= std::time::Duration::from_millis(30));
        t1.join().unwrap().unwrap();
    }

    #[test]
    fn service_kind_serializes_snake_case() {
        let json = serde_json::to_value(RuntimeServiceKind::Marketplace).unwrap();
        assert_eq!(json, "marketplace");
    }

    #[test]
    fn navigation_diagnostics_track_success_and_failure() {
        let bus = test_bus();
        bus.begin_navigation("facebook_home", "https://www.facebook.com/");
        bus.complete_navigation("facebook_home", "https://www.facebook.com/");
        let diag = bus.navigation_diagnostics();
        assert_eq!(
            diag.last_successful_url.as_deref(),
            Some("https://www.facebook.com/")
        );
        assert!(diag.navigation_started_at.is_some());
        assert!(diag.navigation_completed_at.is_some());

        bus.fail_navigation("marketplace", "Navigation timeout of 45000 ms exceeded");
        let failed = bus.navigation_diagnostics();
        assert_eq!(failed.timeout_reason.as_deref(), Some("Navigation timeout of 45000 ms exceeded"));
    }

    #[test]
    fn cancellation_aborts_before_service_acquires_lock() {
        let bus = test_bus();
        bus.request_cancel();
        let result = bus.with_service(RuntimeServiceKind::Navigation, |_| Ok(()));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cancelled"));
    }

    #[test]
    fn service_bus_clears_cancel_after_successful_service() {
        let bus = test_bus();
        bus.request_cancel();
        bus.clear_cancel();
        let result = bus.with_service(RuntimeServiceKind::Navigation, |_| Ok(()));
        assert!(result.is_ok());
        assert!(!bus.is_cancelled());
    }

    #[test]
    fn cancellation_persists_when_aborted_before_lock() {
        let bus = test_bus();
        bus.request_cancel();
        let result = bus.with_service(RuntimeServiceKind::Navigation, |_| Ok(()));
        assert!(result.is_err());
        assert!(bus.is_cancelled());
    }
}
