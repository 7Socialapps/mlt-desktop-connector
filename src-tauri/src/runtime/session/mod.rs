use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::browser::{
    parse_facebook_state, FacebookSessionState, SidecarFacebookDetection,
};

use super::bus::{RuntimeServiceKind, ServiceBus};

const SESSION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionSnapshot {
    pub state: FacebookSessionState,
    pub checked_at: Option<String>,
    pub current_url: Option<String>,
    pub marketplace_accessible: bool,
    pub reason_code: Option<String>,
    pub display_name: Option<String>,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            state: FacebookSessionState::FacebookNotChecked,
            checked_at: None,
            current_url: None,
            marketplace_accessible: false,
            reason_code: None,
            display_name: None,
        }
    }
}

impl SessionSnapshot {
    pub fn state_label(&self) -> String {
        serde_json::to_value(self.state)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "facebook_not_checked".into())
    }
}

pub struct FacebookSessionService {
    bus: Arc<ServiceBus>,
    snapshot: Arc<Mutex<SessionSnapshot>>,
}

impl FacebookSessionService {
    pub fn new(bus: Arc<ServiceBus>) -> Self {
        Self {
            bus,
            snapshot: Arc::new(Mutex::new(SessionSnapshot::default())),
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.snapshot.lock().clone()
    }

    pub fn is_ready_for_services(&self) -> bool {
        matches!(
            self.snapshot.lock().state,
            FacebookSessionState::FacebookLoggedIn
        )
    }

    pub fn check_session(&self) -> Result<SessionSnapshot, String> {
        let browser = self.bus.browser_manager().snapshot();
        if !browser.enabled {
            return Ok(self.snapshot());
        }
        if browser.status != crate::browser::BrowserRuntimeStatus::BrowserReady {
            return Ok(self.snapshot());
        }

        let line = self.bus.sidecar_request(
            RuntimeServiceKind::Session,
            "detect_facebook_session",
            serde_json::json!({}),
            SESSION_TIMEOUT,
        )?;

        if let Some(result) = line.result {
            self.apply_sidecar_result(result)?;
        }

        Ok(self.snapshot())
    }

    fn apply_sidecar_result(&self, result: serde_json::Value) -> Result<(), String> {
        let fb_value = result
            .get("facebook")
            .cloned()
            .unwrap_or(result);
        let detection: SidecarFacebookDetection = serde_json::from_value(fb_value)
            .map_err(|e| format!("invalid facebook detection payload: {e}"))?;
        self.apply_detection(&detection);
        self.sync_browser_manager_session(&detection);
        Ok(())
    }

    fn apply_detection(&self, raw: &SidecarFacebookDetection) {
        let mut snap = self.snapshot.lock();
        snap.state = parse_facebook_state(&raw.state);
        snap.checked_at = Some(raw.checked_at.clone());
        snap.current_url = Some(raw.current_url.clone());
        snap.marketplace_accessible = raw.marketplace_accessible;
        snap.reason_code = Some(raw.reason_code.clone());
        snap.display_name = raw.display_name.clone();
    }

    fn sync_browser_manager_session(&self, raw: &SidecarFacebookDetection) {
        let snap = self.snapshot.lock().clone();
        self.bus.browser_manager().update_facebook_session(
            snap.state,
            snap.checked_at,
            snap.current_url,
            snap.marketplace_accessible,
            snap.reason_code,
        );
        let _ = raw;
    }
}

pub fn parse_session_state_label(raw: &str) -> FacebookSessionState {
    parse_facebook_state(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserRuntimeService, SidecarDaemon};
    use std::path::PathBuf;

    fn mock_detection(state: &str) -> SidecarFacebookDetection {
        SidecarFacebookDetection {
            state: state.into(),
            checked_at: Utc::now().to_rfc3339(),
            current_url: "https://www.facebook.com/".into(),
            marketplace_accessible: state == "facebook_logged_in",
            reason_code: "test".into(),
            display_name: Some("Test Dealer".into()),
        }
    }

    fn test_service() -> FacebookSessionService {
        let runtime = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(PathBuf::new()));
        let manager = Arc::new(crate::browser::BrowserManager::new(runtime, daemon));
        let bus = Arc::new(ServiceBus::new(manager));
        FacebookSessionService::new(bus)
    }

    #[test]
    fn parses_temporary_restriction_state() {
        assert_eq!(
            parse_session_state_label("facebook_temporary_restriction"),
            FacebookSessionState::FacebookTemporaryRestriction
        );
    }

    #[test]
    fn parses_disabled_account_state() {
        assert_eq!(
            parse_session_state_label("facebook_disabled_account"),
            FacebookSessionState::FacebookDisabledAccount
        );
    }

    #[test]
    fn is_ready_only_when_logged_in() {
        let svc = test_service();
        assert!(!svc.is_ready_for_services());
        {
            let mut snap = svc.snapshot.lock();
            snap.state = FacebookSessionState::FacebookLoggedIn;
        }
        assert!(svc.is_ready_for_services());
    }

    #[test]
    fn apply_detection_updates_display_name() {
        let svc = test_service();
        svc.apply_detection(&mock_detection("facebook_logged_in"));
        let snap = svc.snapshot();
        assert_eq!(snap.state, FacebookSessionState::FacebookLoggedIn);
        assert_eq!(snap.display_name.as_deref(), Some("Test Dealer"));
    }

    #[test]
    fn session_state_label_is_snake_case() {
        let svc = test_service();
        {
            let mut snap = svc.snapshot.lock();
            snap.state = FacebookSessionState::FacebookCheckpoint;
        }
        assert_eq!(snap_state_label(&svc), "facebook_checkpoint");
    }

    fn snap_state_label(svc: &FacebookSessionService) -> String {
        svc.snapshot().state_label()
    }
}
