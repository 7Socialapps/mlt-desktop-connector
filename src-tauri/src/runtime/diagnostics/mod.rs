use std::sync::Arc;

use serde::Serialize;

use crate::browser::{url_category, BrowserRuntimeStatus};
use crate::version::CONNECTOR_VERSION;

use super::bus::ServiceBus;
use super::session::FacebookSessionService;
use super::navigation::NavigationService;
use super::status::profile_version_from_path;

/// Shared diagnostics snapshot — never includes cookies, tokens, or credentials.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiagnosticsSnapshot {
    pub browser_health: String,
    pub session_health: String,
    pub current_destination: Option<String>,
    pub current_service: Option<String>,
    pub browser_version: Option<String>,
    pub connector_version: String,
    pub browser_pid: Option<u32>,
    /// Opaque profile identifier — not a filesystem path.
    pub profile_version: String,
    pub last_health_check_at: Option<String>,
    pub last_restart_at: Option<String>,
    pub last_navigation_error: Option<String>,
    pub current_url: Option<String>,
    pub navigation_target: Option<String>,
    pub last_successful_url: Option<String>,
    pub navigation_started_at: Option<String>,
    pub navigation_completed_at: Option<String>,
    pub navigation_failure_reason: Option<String>,
    pub timeout_reason: Option<String>,
}

pub struct DiagnosticsService {
    bus: Arc<ServiceBus>,
    session: Arc<FacebookSessionService>,
}

impl DiagnosticsService {
    pub fn new(bus: Arc<ServiceBus>, session: Arc<FacebookSessionService>) -> Self {
        Self { bus, session }
    }

    pub fn snapshot(&self) -> DiagnosticsSnapshot {
        let browser = self.bus.browser_manager().snapshot();
        let session = self.session.snapshot();
        let nav = self.bus.navigation_diagnostics();

        DiagnosticsSnapshot {
            browser_health: browser_health_label(browser.status),
            session_health: session.canonical_state(),
            current_destination: nav
                .current_destination
                .clone()
                .or_else(|| self.bus.last_destination())
                .or_else(|| {
                    browser
                        .facebook_session
                        .current_url
                        .as_deref()
                        .or(browser.active_page_url.as_deref())
                        .map(|url| url_category(Some(url)).to_string())
                }),
            current_service: self.bus.current_service_name(),
            browser_version: browser.playwright_version.clone(),
            connector_version: CONNECTOR_VERSION.to_string(),
            browser_pid: browser.browser_pid,
            profile_version: profile_version_from_path(browser.profile_path.as_deref()),
            last_health_check_at: browser
                .last_health_check_at
                .clone()
                .or(session.checked_at.clone()),
            last_restart_at: browser.last_restart_at.clone(),
            last_navigation_error: self
                .bus
                .last_navigation_error()
                .or(nav.navigation_failure_reason.clone()),
            current_url: nav
                .current_url
                .clone()
                .or(browser.active_page_url.clone())
                .or(session.current_url.clone()),
            navigation_target: nav.navigation_target.clone(),
            last_successful_url: nav.last_successful_url.clone(),
            navigation_started_at: nav.navigation_started_at.clone(),
            navigation_completed_at: nav.navigation_completed_at.clone(),
            navigation_failure_reason: nav.navigation_failure_reason.clone(),
            timeout_reason: nav.timeout_reason.clone(),
        }
    }
}

fn browser_health_label(status: BrowserRuntimeStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserRuntimeService, SidecarDaemon};
    use crate::runtime::bus::ServiceBus;
    use std::path::PathBuf;

    fn test_diagnostics() -> DiagnosticsService {
        let runtime = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(PathBuf::new()));
        let manager = Arc::new(crate::browser::BrowserManager::new(runtime, daemon));
        let bus = Arc::new(ServiceBus::new(manager));
        let navigation = Arc::new(NavigationService::new(bus.clone()));
        let session = Arc::new(FacebookSessionService::new(bus.clone(), navigation));
        DiagnosticsService::new(bus, session)
    }

    #[test]
    fn snapshot_never_includes_raw_profile_path() {
        let diag = test_diagnostics();
        let snap = diag.snapshot();
        assert!(!snap.profile_version.contains("/Users/"));
        assert!(!snap.profile_version.contains("browser-profile"));
    }

    #[test]
    fn navigation_error_visible_in_diagnostics() {
        let runtime = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(PathBuf::new()));
        let manager = Arc::new(crate::browser::BrowserManager::new(runtime, daemon));
        let bus = Arc::new(ServiceBus::new(manager));
        bus.record_navigation_error("messenger", "timeout");
        let navigation = Arc::new(NavigationService::new(bus.clone()));
        let session = Arc::new(FacebookSessionService::new(bus.clone(), navigation));
        let diag = DiagnosticsService::new(bus, session);
        let snap = diag.snapshot();
        assert_eq!(snap.last_navigation_error.as_deref(), Some("timeout"));
        assert_eq!(snap.navigation_failure_reason.as_deref(), Some("timeout"));
        assert_eq!(snap.timeout_reason.as_deref(), Some("timeout"));
    }
}
