use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::api::ConnectorApiClient;
use crate::browser::{
    url_category, BrowserManager, BrowserRuntimeStatus, MarketplaceStatus, ProfileStatus,
};
use crate::credentials::{ensure_access_token, has_access_token, is_paired};
use crate::state::{AppState, ConnectionState};
use crate::version::CONNECTOR_VERSION;

fn status_snake<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "unknown".into())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionCheck {
    pub id: String,
    pub status: String,
    pub label: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionTestReport {
    pub checks: Vec<ConnectionCheck>,
    pub overall_status: String,
    pub checked_at: String,
}

fn check_pass(id: &str, label: &str, detail: &str) -> ConnectionCheck {
    ConnectionCheck {
        id: id.into(),
        status: "pass".into(),
        label: label.into(),
        detail: detail.into(),
        error_code: None,
        checked_at: Utc::now().to_rfc3339(),
    }
}

fn check_fail(id: &str, label: &str, detail: &str, code: &str) -> ConnectionCheck {
    ConnectionCheck {
        id: id.into(),
        status: "fail".into(),
        label: label.into(),
        detail: detail.into(),
        error_code: Some(code.into()),
        checked_at: Utc::now().to_rfc3339(),
    }
}

fn check_warn(id: &str, label: &str, detail: &str, code: Option<&str>) -> ConnectionCheck {
    ConnectionCheck {
        id: id.into(),
        status: "warn".into(),
        label: label.into(),
        detail: detail.into(),
        error_code: code.map(str::to_string),
        checked_at: Utc::now().to_rfc3339(),
    }
}

pub fn build_heartbeat_browser_fields(
    state: &AppState,
    browser: &crate::browser::BrowserManagerSnapshot,
) -> HeartbeatBrowserPayload {
    let connector_status = match state.connection_state {
        ConnectionState::Connected | ConnectionState::Idle => "connector_ready",
        ConnectionState::Reconnecting | ConnectionState::Starting => "connector_reconnecting",
        ConnectionState::ShuttingDown => "connector_shutting_down",
        ConnectionState::Offline => "connector_offline",
    };

    HeartbeatBrowserPayload {
        connector_status: connector_status.into(),
        browser_status: status_snake(&browser.status),
        browser_version: browser.playwright_version.clone(),
        profile_status: status_snake(&browser.profile_status),
        facebook_session_state: status_snake(&browser.facebook_session.state),
        marketplace_status: status_snake(&browser.marketplace.status),
        current_browser_url_category: url_category(
            browser
                .facebook_session
                .current_url
                .as_deref()
                .or(browser.active_page_url.as_deref()),
        )
        .to_string(),
        last_browser_check_at: browser
            .facebook_session
            .checked_at
            .clone()
            .or(browser.last_health_check_at.clone()),
        last_browser_error_code: browser.last_error_code.clone(),
        connector_version: CONNECTOR_VERSION.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct HeartbeatBrowserPayload {
    pub connector_status: String,
    pub browser_status: String,
    pub browser_version: Option<String>,
    pub profile_status: String,
    pub facebook_session_state: String,
    pub marketplace_status: String,
    pub current_browser_url_category: String,
    pub last_browser_check_at: Option<String>,
    pub last_browser_error_code: Option<String>,
    pub connector_version: String,
}

pub async fn run_connection_tests(
    client: &ConnectorApiClient,
    state: &Arc<Mutex<AppState>>,
    browser_manager: &BrowserManager,
    browser_runtime: &crate::browser::BrowserRuntimeService,
) -> ConnectionTestReport {
    let mut checks = Vec::new();

    // Backend auth
    if !is_paired() {
        checks.push(check_fail(
            "backend_auth",
            "Backend authentication",
            "Device is not paired",
            "NOT_PAIRED",
        ));
    } else if !has_access_token() {
        match ensure_access_token(client).await {
            Ok(true) => checks.push(check_pass(
                "backend_auth",
                "Backend authentication",
                "Access token available",
            )),
            Ok(false) => checks.push(check_fail(
                "backend_auth",
                "Backend authentication",
                "Reconnect required — stored credentials unavailable",
                "NEEDS_RECONNECT",
            )),
            Err(_err) => checks.push(check_fail(
                "backend_auth",
                "Backend authentication",
                "Token refresh failed",
                "TOKEN_REFRESH_FAILED",
            )),
        }
    } else {
        checks.push(check_pass(
            "backend_auth",
            "Backend authentication",
            "Credentials valid",
        ));
    }

    // Heartbeat recency
    {
        let guard = state.lock();
        if let Some(at) = &guard.last_heartbeat_at {
            checks.push(check_pass(
                "heartbeat",
                "Heartbeat",
                &format!("Last heartbeat at {at}"),
            ));
        } else if guard.paired {
            checks.push(check_warn(
                "heartbeat",
                "Heartbeat",
                "No heartbeat recorded yet",
                Some("HEARTBEAT_PENDING"),
            ));
        } else {
            checks.push(check_warn(
                "heartbeat",
                "Heartbeat",
                "Skipped — device not paired",
                None,
            ));
        }
    }

    // Browser runtime
    let runtime = browser_runtime.snapshot();
    if !runtime.enabled {
        checks.push(check_warn(
            "browser_runtime",
            "Browser runtime",
            "Browser subsystem disabled",
            Some("BROWSER_DISABLED"),
        ));
    } else if runtime.playwright_installed && runtime.chromium_installed {
        checks.push(check_pass(
            "browser_runtime",
            "Browser runtime",
            "Playwright and Chromium detected",
        ));
    } else {
        checks.push(check_fail(
            "browser_runtime",
            "Browser runtime",
            "Playwright or Chromium not installed — run npm run browser:install",
            "RUNTIME_NOT_INSTALLED",
        ));
    }

    // Chromium launch / browser ready
    let browser = browser_manager.snapshot();
    if browser.status == BrowserRuntimeStatus::BrowserReady {
        checks.push(check_pass(
            "chromium_launch",
            "Chromium launch",
            "Browser is running",
        ));
    } else if browser.chromium_installed {
        checks.push(check_warn(
            "chromium_launch",
            "Chromium launch",
            "Chromium installed but browser not launched",
            Some("BROWSER_NOT_LAUNCHED"),
        ));
    } else {
        checks.push(check_fail(
            "chromium_launch",
            "Chromium launch",
            "Chromium is not installed",
            "CHROMIUM_NOT_INSTALLED",
        ));
    }

    // Persistent profile
    match browser.profile_status {
        ProfileStatus::ProfileReady => checks.push(check_pass(
            "persistent_profile",
            "Persistent profile",
            "Browser profile is ready",
        )),
        ProfileStatus::ProfileMissing => checks.push(check_warn(
            "persistent_profile",
            "Persistent profile",
            "Profile not created — launch browser to initialize",
            Some("PROFILE_MISSING"),
        )),
        ProfileStatus::ProfileLocked => checks.push(check_fail(
            "persistent_profile",
            "Persistent profile",
            "Profile locked by another process",
            "PROFILE_LOCKED",
        )),
        ProfileStatus::ProfileCorrupt | ProfileStatus::ProfileResetRequired => {
            checks.push(check_fail(
                "persistent_profile",
                "Persistent profile",
                "Profile corrupt — reset required",
                "PROFILE_CORRUPT",
            ))
        }
        ProfileStatus::ProfileInitializing => checks.push(check_warn(
            "persistent_profile",
            "Persistent profile",
            "Profile is initializing",
            Some("PROFILE_INITIALIZING"),
        )),
    }

    // Facebook session
    let fb = &browser.facebook_session.state;
    match fb {
        crate::browser::FacebookSessionState::FacebookLoggedIn => checks.push(check_pass(
            "facebook_session",
            "Facebook session",
            "Signed in to Facebook",
        )),
        crate::browser::FacebookSessionState::FacebookNotChecked => checks.push(check_warn(
            "facebook_session",
            "Facebook session",
            "Not checked — use Open Facebook Login",
            Some("FACEBOOK_NOT_CHECKED"),
        )),
        crate::browser::FacebookSessionState::FacebookLoggedOut
        | crate::browser::FacebookSessionState::FacebookSessionExpired => checks.push(check_fail(
            "facebook_session",
            "Facebook session",
            "Sign in required",
            "FACEBOOK_LOGIN_REQUIRED",
        )),
        crate::browser::FacebookSessionState::FacebookLoginInProgress => checks.push(check_warn(
            "facebook_session",
            "Facebook session",
            "Login in progress",
            Some("FACEBOOK_LOGIN_IN_PROGRESS"),
        )),
        crate::browser::FacebookSessionState::FacebookCheckpoint
        | crate::browser::FacebookSessionState::FacebookMfaRequired => checks.push(check_warn(
            "facebook_session",
            "Facebook session",
            "Manual Facebook action required",
            Some("FACEBOOK_ACTION_REQUIRED"),
        )),
        crate::browser::FacebookSessionState::FacebookError => checks.push(check_fail(
            "facebook_session",
            "Facebook session",
            "Facebook session error",
            "FACEBOOK_SESSION_ERROR",
        )),
    }

    // Marketplace access
    match browser.marketplace.status {
        MarketplaceStatus::MarketplaceReady => checks.push(check_pass(
            "marketplace_access",
            "Marketplace access",
            "Marketplace is accessible",
        )),
        MarketplaceStatus::MarketplaceNotChecked => checks.push(check_warn(
            "marketplace_access",
            "Marketplace access",
            "Not checked — use Open Marketplace",
            Some("MARKETPLACE_NOT_CHECKED"),
        )),
        MarketplaceStatus::MarketplaceLoginRequired => checks.push(check_fail(
            "marketplace_access",
            "Marketplace access",
            "Facebook login required",
            "MARKETPLACE_LOGIN_REQUIRED",
        )),
        MarketplaceStatus::MarketplaceCheckpoint => checks.push(check_warn(
            "marketplace_access",
            "Marketplace access",
            "Facebook checkpoint blocking Marketplace",
            Some("MARKETPLACE_CHECKPOINT"),
        )),
        MarketplaceStatus::MarketplaceLoading => checks.push(check_warn(
            "marketplace_access",
            "Marketplace access",
            "Marketplace navigation in progress",
            Some("MARKETPLACE_LOADING"),
        )),
        MarketplaceStatus::MarketplaceUnavailable | MarketplaceStatus::MarketplaceError => {
            checks.push(check_fail(
                "marketplace_access",
                "Marketplace access",
                "Marketplace unavailable or navigation failed",
                "MARKETPLACE_UNAVAILABLE",
            ))
        }
    }

    let overall = if checks.iter().any(|c| c.status == "fail") {
        "fail"
    } else if checks.iter().any(|c| c.status == "warn") {
        "warn"
    } else {
        "pass"
    };

    ConnectionTestReport {
        checks,
        overall_status: overall.into(),
        checked_at: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserManagerSnapshot, BrowserRuntimeSnapshot, BrowserRuntimeStatus};

    #[test]
    fn heartbeat_payload_uses_snake_case_facebook_state() {
        let state = AppState {
            device_id: uuid::Uuid::new_v4(),
            environment: "staging".into(),
            paired: true,
            needs_reconnect: false,
            connection_state: ConnectionState::Connected,
            last_heartbeat_at: None,
            last_error: None,
        };
        let browser = BrowserManagerSnapshot::from_runtime(&BrowserRuntimeSnapshot {
                status: BrowserRuntimeStatus::BrowserReady,
                enabled: true,
                playwright_installed: true,
                playwright_version: Some("1.52.0".into()),
                chromium_installed: true,
                chromium_path: None,
                node_version: None,
                last_error: None,
                last_error_code: None,
                checked_at: None,
            });
        let payload = build_heartbeat_browser_fields(&state, &browser);
        assert_eq!(
            payload.facebook_session_state,
            "facebook_not_checked"
        );
        assert!(!payload.current_browser_url_category.is_empty());
    }
}
