use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::api::ConnectorApiClient;
use crate::browser::{
    url_category, BrowserManager, BrowserRuntimeStatus, MarketplaceStatus, ProfileStatus,
};
use crate::runtime::FacebookRuntime;
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
    facebook_runtime: &FacebookRuntime,
) -> HeartbeatBrowserPayload {
    let runtime_status = facebook_runtime.aggregate_status();
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
            .or(browser.last_health_check_at.clone())
            .or(runtime_status.last_health_check.clone()),
        last_browser_error_code: browser.last_error_code.clone(),
        connector_version: CONNECTOR_VERSION.to_string(),
        current_facebook_account: runtime_status.current_facebook_account,
        session_state: Some(runtime_status.session_state),
        marketplace_ready: Some(runtime_status.marketplace_ready),
        messenger_ready: Some(runtime_status.messenger_ready),
        notifications_ready: Some(runtime_status.notifications_ready),
        browser_pid: runtime_status.browser_pid,
        profile_version: Some(runtime_status.profile_version),
        current_service: runtime_status.current_service,
        last_health_check: runtime_status.last_health_check,
        last_restart: runtime_status.last_restart,
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
    pub current_facebook_account: Option<String>,
    pub session_state: Option<String>,
    pub marketplace_ready: Option<bool>,
    pub messenger_ready: Option<bool>,
    pub notifications_ready: Option<bool>,
    pub browser_pid: Option<u32>,
    pub profile_version: Option<String>,
    pub current_service: Option<String>,
    pub last_health_check: Option<String>,
    pub last_restart: Option<String>,
}

pub async fn run_connection_tests(
    client: &ConnectorApiClient,
    state: &Arc<Mutex<AppState>>,
    browser_manager: &BrowserManager,
    browser_runtime: &crate::browser::BrowserRuntimeService,
    _facebook_runtime: &FacebookRuntime,
) -> ConnectionTestReport {
    let _ = _facebook_runtime;
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
        | crate::browser::FacebookSessionState::FacebookMfaRequired
        | crate::browser::FacebookSessionState::FacebookTemporaryRestriction
        | crate::browser::FacebookSessionState::FacebookDisabledAccount => checks.push(check_warn(
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
    use crate::api::types::{ConnectorOs, HeartbeatRequest};
    use crate::browser::{
        BrowserManagerSnapshot, BrowserRuntimeSnapshot, BrowserRuntimeStatus,
        FacebookSessionState, MarketplaceStatus, ProfileStatus,
    };

    fn sample_app_state(connection: ConnectionState) -> AppState {
        AppState {
            device_id: uuid::Uuid::new_v4(),
            environment: "staging".into(),
            paired: true,
            needs_reconnect: false,
            connection_state: connection,
            last_heartbeat_at: None,
            last_error: None,
            current_job_id: None,
        }
    }

    use crate::runtime::FacebookRuntime;
    use std::sync::Arc;

    fn sample_facebook_runtime() -> Arc<FacebookRuntime> {
        let runtime_svc = Arc::new(crate::browser::BrowserRuntimeService::new(false));
        let daemon = Arc::new(crate::browser::SidecarDaemon::new(std::path::PathBuf::new()));
        let manager = Arc::new(crate::browser::BrowserManager::new(runtime_svc, daemon));
        FacebookRuntime::new(manager)
    }

    fn payload_with_runtime(
        state: &AppState,
        browser: &BrowserManagerSnapshot,
    ) -> HeartbeatBrowserPayload {
        let rt = sample_facebook_runtime();
        build_heartbeat_browser_fields(state, browser, &rt)
    }

    fn sample_browser_snapshot(status: BrowserRuntimeStatus) -> BrowserManagerSnapshot {
        BrowserManagerSnapshot::from_runtime(&BrowserRuntimeSnapshot {
            status,
            enabled: true,
            playwright_installed: true,
            playwright_version: Some("1.52.0".into()),
            chromium_installed: true,
            chromium_path: None,
            node_version: None,
            last_error: None,
            last_error_code: None,
            checked_at: None,
        })
    }

    #[test]
    fn heartbeat_payload_uses_snake_case_facebook_state() {
        let state = sample_app_state(ConnectionState::Connected);
        let browser = sample_browser_snapshot(BrowserRuntimeStatus::BrowserReady);
        let payload = payload_with_runtime(&state, &browser);
        assert_eq!(
            payload.facebook_session_state,
            "facebook_not_checked"
        );
        assert!(!payload.current_browser_url_category.is_empty());
    }

    #[test]
    fn connector_stays_ready_when_browser_fails() {
        let state = sample_app_state(ConnectionState::Connected);
        let mut browser = sample_browser_snapshot(BrowserRuntimeStatus::BrowserError);
        browser.last_error_code = Some("BROWSER_CRASHED".into());
        let payload = payload_with_runtime(&state, &browser);
        assert_eq!(payload.connector_status, "connector_ready");
        assert_eq!(payload.browser_status, "browser_error");
        assert_eq!(
            payload.last_browser_error_code.as_deref(),
            Some("BROWSER_CRASHED")
        );
    }

    #[test]
    fn heartbeat_request_serializes_snake_case_browser_fields() {
        let payload = payload_with_runtime(
            &sample_app_state(ConnectionState::Idle),
            &sample_browser_snapshot(BrowserRuntimeStatus::BrowserReady),
        );
        let request = HeartbeatRequest {
            action: "heartbeat".into(),
            device_id: "dev-1".into(),
            user_id: "user-1".into(),
            dealership_id: "dealer-1".into(),
            connector_version: payload.connector_version.clone(),
            os: ConnectorOs::Macos,
            capabilities: vec!["posting".into()],
            connector_status: payload.connector_status,
            browser_status: payload.browser_status,
            browser_version: payload.browser_version,
            profile_status: payload.profile_status,
            facebook_session_state: payload.facebook_session_state,
            marketplace_status: payload.marketplace_status,
            current_browser_url_category: payload.current_browser_url_category,
            last_browser_check_at: payload.last_browser_check_at,
            last_browser_error_code: payload.last_browser_error_code,
            current_facebook_account: payload.current_facebook_account,
            session_state: payload.session_state,
            marketplace_ready: payload.marketplace_ready,
            messenger_ready: payload.messenger_ready,
            notifications_ready: payload.notifications_ready,
            browser_pid: payload.browser_pid,
            profile_version: payload.profile_version,
            current_service: payload.current_service,
            last_health_check: payload.last_health_check,
            last_restart: payload.last_restart,
        };
        let json = serde_json::to_value(&request).expect("serialize heartbeat");
        assert_eq!(json["facebook_session_state"], "facebook_not_checked");
        assert_eq!(json["browser_status"], "browser_ready");
        assert_eq!(json["profile_status"], "profile_missing");
        assert_eq!(json["marketplace_status"], "marketplace_not_checked");
        assert_eq!(json["connector_status"], "connector_ready");
        assert_eq!(json["current_browser_url_category"], "unknown");
        assert_eq!(json["deviceId"], "dev-1");
    }

    #[test]
    fn connection_test_report_has_expected_shape() {
        let report = ConnectionTestReport {
            checks: vec![
                check_pass("backend_auth", "Backend authentication", "Credentials valid"),
                check_warn(
                    "browser_runtime",
                    "Browser runtime",
                    "Browser subsystem disabled",
                    Some("BROWSER_DISABLED"),
                ),
            ],
            overall_status: "warn".into(),
            checked_at: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_value(&report).expect("serialize report");
        assert_eq!(json["overall_status"], "warn");
        assert!(json["checks"].is_array());
        let first = &json["checks"][0];
        assert_eq!(first["id"], "backend_auth");
        assert_eq!(first["status"], "pass");
        assert_eq!(first["error_code"], serde_json::Value::Null);
        let second = &json["checks"][1];
        assert_eq!(second["status"], "warn");
        assert_eq!(second["error_code"], "BROWSER_DISABLED");
    }

    #[test]
    fn heartbeat_url_category_masks_full_facebook_url() {
        let state = sample_app_state(ConnectionState::Connected);
        let mut browser = sample_browser_snapshot(BrowserRuntimeStatus::BrowserReady);
        browser.facebook_session.state = FacebookSessionState::FacebookLoggedIn;
        browser.facebook_session.current_url =
            Some("https://www.facebook.com/groups/secret-group-id".into());
        let payload = payload_with_runtime(&state, &browser);
        assert_eq!(payload.current_browser_url_category, "facebook_other");
        assert!(!payload.current_browser_url_category.contains("secret"));
    }

    #[test]
    fn connection_payload_maps_profile_and_marketplace_snake_case() {
        let state = sample_app_state(ConnectionState::Reconnecting);
        let mut browser = sample_browser_snapshot(BrowserRuntimeStatus::BrowserCrashed);
        browser.profile_status = ProfileStatus::ProfileLocked;
        browser.marketplace.status = MarketplaceStatus::MarketplaceLoginRequired;
        browser.facebook_session.state = FacebookSessionState::FacebookLoggedOut;
        let payload = payload_with_runtime(&state, &browser);
        assert_eq!(payload.connector_status, "connector_reconnecting");
        assert_eq!(payload.browser_status, "browser_crashed");
        assert_eq!(payload.profile_status, "profile_locked");
        assert_eq!(payload.marketplace_status, "marketplace_login_required");
        assert_eq!(payload.facebook_session_state, "facebook_logged_out");
    }
}
