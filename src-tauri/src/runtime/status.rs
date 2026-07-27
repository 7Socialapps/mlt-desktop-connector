use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::browser::{BrowserRuntimeStatus, BrowserManagerSnapshot};
use crate::version::CONNECTOR_VERSION;

use super::bus::ServiceBus;
use super::marketplace::MarketplaceService;
use super::messenger::MessengerService;
use super::notifications::NotificationService;
use super::session::FacebookSessionService;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FacebookRuntimeStatus {
    pub current_browser: BrowserRuntimeStatus,
    pub current_facebook_account: Option<String>,
    pub session_state: String,
    pub marketplace_ready: bool,
    pub messenger_ready: bool,
    pub notifications_ready: bool,
    pub browser_pid: Option<u32>,
    pub browser_version: Option<String>,
    pub profile_version: String,
    pub connector_version: String,
    pub current_service: Option<String>,
    pub last_health_check: Option<String>,
    pub last_restart: Option<String>,
}

impl Default for FacebookRuntimeStatus {
    fn default() -> Self {
        Self {
            current_browser: BrowserRuntimeStatus::BrowserStopped,
            current_facebook_account: None,
            session_state: "facebook_not_checked".into(),
            marketplace_ready: false,
            messenger_ready: false,
            notifications_ready: false,
            browser_pid: None,
            browser_version: None,
            profile_version: "1".into(),
            connector_version: CONNECTOR_VERSION.to_string(),
            current_service: None,
            last_health_check: None,
            last_restart: None,
        }
    }
}

pub fn profile_version_from_path(path: Option<&str>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let Some(path) = path.filter(|p| !p.is_empty()) else {
        return "1".into();
    };
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

impl FacebookRuntimeStatus {
    pub fn aggregate(
        bus: &ServiceBus,
        session: &FacebookSessionService,
        marketplace: &MarketplaceService,
        messenger: &MessengerService,
        notifications: &NotificationService,
    ) -> Self {
        let browser = bus.browser_manager().snapshot();
        let session_snap = session.snapshot();
        Self::from_browser_and_services(
            &browser,
            &session_snap.state_label(),
            session_snap.display_name.clone(),
            marketplace.is_ready(),
            messenger.is_ready(),
            notifications.is_ready(),
            bus.current_service_name(),
        )
    }

    pub fn from_browser_and_services(
        browser: &BrowserManagerSnapshot,
        session_state: &str,
        account: Option<String>,
        marketplace_ready: bool,
        messenger_ready: bool,
        notifications_ready: bool,
        current_service: Option<String>,
    ) -> Self {
        Self {
            current_browser: browser.status,
            current_facebook_account: account,
            session_state: session_state.to_string(),
            marketplace_ready,
            messenger_ready,
            notifications_ready,
            browser_pid: browser.browser_pid,
            browser_version: browser.playwright_version.clone(),
            profile_version: profile_version_from_path(browser.profile_path.as_deref()),
            connector_version: CONNECTOR_VERSION.to_string(),
            current_service,
            last_health_check: browser.last_health_check_at.clone(),
            last_restart: browser.last_restart_at.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{
        BrowserManagerSnapshot, BrowserRuntimeSnapshot, BrowserRuntimeStatus,
        FacebookSessionSnapshot, FacebookSessionState,
    };

    fn sample_browser() -> BrowserManagerSnapshot {
        BrowserManagerSnapshot::from_runtime(&BrowserRuntimeSnapshot {
            status: BrowserRuntimeStatus::BrowserReady,
            enabled: true,
            playwright_installed: true,
            playwright_version: Some("1.52.0".into()),
            chromium_installed: true,
            chromium_path: None,
            node_version: None,
            last_error: None,
            last_error_code: None,
            checked_at: Some(Utc::now().to_rfc3339()),
        })
    }

    #[test]
    fn aggregates_runtime_status_fields() {
        let mut browser = sample_browser();
        browser.browser_pid = Some(1234);
        browser.profile_path = Some("/tmp/profile".into());
        browser.facebook_session = FacebookSessionSnapshot {
            state: FacebookSessionState::FacebookLoggedIn,
            checked_at: Some(Utc::now().to_rfc3339()),
            current_url: Some("https://www.facebook.com/".into()),
            marketplace_accessible: true,
            reason_code: Some("nav_present".into()),
        };
        browser.marketplace.status = crate::browser::MarketplaceStatus::MarketplaceReady;

        let status = FacebookRuntimeStatus::from_browser_and_services(
            &browser,
            "facebook_logged_in",
            Some("Dealer Name".into()),
            true,
            false,
            false,
            Some("marketplace".into()),
        );

        assert_eq!(status.current_browser, BrowserRuntimeStatus::BrowserReady);
        assert_eq!(
            status.current_facebook_account.as_deref(),
            Some("Dealer Name")
        );
        assert_eq!(status.session_state, "facebook_logged_in");
        assert!(status.marketplace_ready);
        assert!(!status.messenger_ready);
        assert_eq!(status.browser_pid, Some(1234));
        assert_eq!(status.current_service.as_deref(), Some("marketplace"));
        assert_ne!(status.profile_version, "1");
    }

    #[test]
    fn profile_version_defaults_when_path_missing() {
        assert_eq!(profile_version_from_path(None), "1");
        assert_eq!(profile_version_from_path(Some("")), "1");
    }

    #[test]
    fn default_status_is_conservative() {
        let status = FacebookRuntimeStatus::default();
        assert!(!status.marketplace_ready);
        assert!(!status.messenger_ready);
        assert_eq!(status.session_state, "facebook_not_checked");
    }
}
