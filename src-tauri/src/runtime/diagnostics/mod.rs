use std::sync::Arc;

use serde::Serialize;

use crate::browser::{url_category, BrowserRuntimeStatus};
use crate::version::CONNECTOR_VERSION;

use super::bus::ServiceBus;
use super::session::FacebookSessionService;
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

        DiagnosticsSnapshot {
            browser_health: browser_health_label(browser.status),
            session_health: session.canonical_state(),
            current_destination: self.bus.last_destination().or_else(|| {
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
            last_navigation_error: self.bus.last_navigation_error(),
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
        let session = Arc::new(FacebookSessionService::new(bus.clone()));
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
        let session = Arc::new(FacebookSessionService::new(bus.clone()));
        let diag = DiagnosticsService::new(bus, session);
        assert_eq!(
            diag.snapshot().last_navigation_error.as_deref(),
            Some("timeout")
        );
    }
}
