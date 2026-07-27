use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacebookSessionState {
    FacebookNotChecked,
    FacebookLoggedOut,
    FacebookLoginInProgress,
    FacebookLoggedIn,
    FacebookCheckpoint,
    FacebookMfaRequired,
    FacebookSessionExpired,
    FacebookError,
}

impl FacebookSessionState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::FacebookNotChecked => "Facebook session not checked",
            Self::FacebookLoggedOut => "Signed out of Facebook",
            Self::FacebookLoginInProgress => "Facebook login in progress",
            Self::FacebookLoggedIn => "Signed in to Facebook",
            Self::FacebookCheckpoint => "Facebook security checkpoint",
            Self::FacebookMfaRequired => "Facebook MFA required",
            Self::FacebookSessionExpired => "Facebook session expired",
            Self::FacebookError => "Facebook session error",
        }
    }

    pub fn remediation(&self) -> Option<&'static str> {
        match self {
            Self::FacebookLoggedOut | Self::FacebookSessionExpired => {
                Some("Click Open Facebook Login and sign in manually in the browser window.")
            }
            Self::FacebookCheckpoint => Some(
                "Complete Facebook's security checkpoint manually in the browser window.",
            ),
            Self::FacebookMfaRequired => {
                Some("Complete two-factor authentication manually in the browser window.")
            }
            Self::FacebookLoginInProgress => {
                Some("Finish signing in manually — do not close the browser.")
            }
            Self::FacebookError => Some("Restart the browser and try Open Facebook Login again."),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FacebookSessionSnapshot {
    pub state: FacebookSessionState,
    pub checked_at: Option<String>,
    pub current_url: Option<String>,
    pub marketplace_accessible: bool,
    pub reason_code: Option<String>,
}

impl Default for FacebookSessionSnapshot {
    fn default() -> Self {
        Self {
            state: FacebookSessionState::FacebookNotChecked,
            checked_at: None,
            current_url: None,
            marketplace_accessible: false,
            reason_code: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SidecarFacebookDetection {
    pub state: String,
    pub checked_at: String,
    pub current_url: String,
    pub marketplace_accessible: bool,
    pub reason_code: String,
}

pub fn parse_facebook_state(raw: &str) -> FacebookSessionState {
    match raw {
        "facebook_logged_out" => FacebookSessionState::FacebookLoggedOut,
        "facebook_login_in_progress" => FacebookSessionState::FacebookLoginInProgress,
        "facebook_logged_in" => FacebookSessionState::FacebookLoggedIn,
        "facebook_checkpoint" => FacebookSessionState::FacebookCheckpoint,
        "facebook_mfa_required" => FacebookSessionState::FacebookMfaRequired,
        "facebook_session_expired" => FacebookSessionState::FacebookSessionExpired,
        "facebook_error" => FacebookSessionState::FacebookError,
        _ => FacebookSessionState::FacebookNotChecked,
    }
}

pub fn apply_detection(snapshot: &mut FacebookSessionSnapshot, raw: &SidecarFacebookDetection) {
    snapshot.state = parse_facebook_state(&raw.state);
    snapshot.checked_at = Some(raw.checked_at.clone());
    snapshot.current_url = Some(raw.current_url.clone());
    snapshot.marketplace_accessible = raw.marketplace_accessible;
    snapshot.reason_code = Some(raw.reason_code.clone());
}

/// Category for heartbeat — never sends full URL.
pub fn url_category(url: Option<&str>) -> &'static str {
    let Some(url) = url else {
        return "unknown";
    };
    if url == "about:blank" || url.is_empty() {
        return "blank";
    }
    let lower = url.to_lowercase();
    if lower.contains("facebook.com/marketplace") {
        return "facebook_marketplace";
    }
    if lower.contains("facebook.com/login") || lower.contains("facebook.com/checkpoint") {
        return "facebook_auth";
    }
    if lower.contains("facebook.com") {
        return "facebook_other";
    }
    "other"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_detection(state: &str, url: &str, marketplace: bool, reason: &str) -> SidecarFacebookDetection {
        SidecarFacebookDetection {
            state: state.into(),
            checked_at: Utc::now().to_rfc3339(),
            current_url: url.into(),
            marketplace_accessible: marketplace,
            reason_code: reason.into(),
        }
    }

    #[test]
    fn parse_logged_in_state() {
        assert_eq!(
            parse_facebook_state("facebook_logged_in"),
            FacebookSessionState::FacebookLoggedIn
        );
    }

    #[test]
    fn parse_checkpoint_state() {
        assert_eq!(
            parse_facebook_state("facebook_checkpoint"),
            FacebookSessionState::FacebookCheckpoint
        );
    }

    #[test]
    fn apply_detection_updates_snapshot() {
        let mut snap = FacebookSessionSnapshot::default();
        apply_detection(
            &mut snap,
            &mock_detection(
                "facebook_logged_in",
                "https://www.facebook.com/",
                true,
                "nav_present",
            ),
        );
        assert_eq!(snap.state, FacebookSessionState::FacebookLoggedIn);
        assert!(snap.marketplace_accessible);
        assert_eq!(snap.reason_code.as_deref(), Some("nav_present"));
    }

    #[test]
    fn url_category_masks_sensitive_paths() {
        assert_eq!(url_category(None), "unknown");
        assert_eq!(url_category(Some("about:blank")), "blank");
        assert_eq!(
            url_category(Some("https://www.facebook.com/marketplace/")),
            "facebook_marketplace"
        );
        assert_eq!(
            url_category(Some("https://www.facebook.com/login.php")),
            "facebook_auth"
        );
        assert_eq!(
            url_category(Some("https://www.facebook.com/groups/123")),
            "facebook_other"
        );
    }

    #[test]
    fn mocked_logged_out_page() {
        let mut snap = FacebookSessionSnapshot::default();
        apply_detection(
            &mut snap,
            &mock_detection(
                "facebook_logged_out",
                "https://www.facebook.com/login.php",
                false,
                "login_page",
            ),
        );
        assert_eq!(snap.state, FacebookSessionState::FacebookLoggedOut);
        assert!(!snap.marketplace_accessible);
    }

    #[test]
    fn mocked_mfa_page() {
        let mut snap = FacebookSessionSnapshot::default();
        apply_detection(
            &mut snap,
            &mock_detection(
                "facebook_mfa_required",
                "https://www.facebook.com/two_step_verification/",
                false,
                "mfa_prompt",
            ),
        );
        assert_eq!(snap.state, FacebookSessionState::FacebookMfaRequired);
    }

    #[test]
    fn facebook_state_labels_are_user_facing() {
        assert!(FacebookSessionState::FacebookCheckpoint.label().contains("checkpoint"));
        assert!(FacebookSessionState::FacebookMfaRequired.label().contains("MFA"));
    }
}
