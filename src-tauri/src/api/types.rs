use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectorOs {
    Windows,
    Macos,
    Linux,
    Unknown,
}

impl ConnectorOs {
    pub fn detect() -> Self {
        match std::env::consts::OS {
            "windows" => Self::Windows,
            "macos" => Self::Macos,
            "linux" => Self::Linux,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    ConnectorReady,
    ConnectorOffline,
    UpdateRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FacebookSessionState {
    FacebookNotChecked,
    FacebookLoggedOut,
    FacebookLoginInProgress,
    FacebookLoggedIn,
    FacebookCheckpoint,
    FacebookMfaRequired,
    FacebookSessionExpired,
    FacebookTemporaryRestriction,
    FacebookDisabledAccount,
    FacebookError,
    /// Legacy values retained for backward-compatible deserialization.
    #[serde(alias = "unknown")]
    Unknown,
    #[serde(alias = "signed_in")]
    SignedIn,
    #[serde(alias = "signed_out")]
    SignedOut,
    #[serde(alias = "expired")]
    Expired,
}

impl FacebookSessionState {
    pub fn as_heartbeat_str(&self) -> &'static str {
        match self {
            Self::FacebookNotChecked | Self::Unknown => "facebook_not_checked",
            Self::FacebookLoggedOut | Self::SignedOut => "facebook_logged_out",
            Self::FacebookLoginInProgress => "facebook_login_in_progress",
            Self::FacebookLoggedIn | Self::SignedIn => "facebook_logged_in",
            Self::FacebookCheckpoint => "facebook_checkpoint",
            Self::FacebookMfaRequired => "facebook_mfa_required",
            Self::FacebookSessionExpired | Self::Expired => "facebook_session_expired",
            Self::FacebookTemporaryRestriction => "facebook_temporary_restriction",
            Self::FacebookDisabledAccount => "facebook_disabled_account",
            Self::FacebookError => "facebook_error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserUrlCategory {
    Unknown,
    Blank,
    FacebookMarketplace,
    FacebookAuth,
    FacebookOther,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceRequest {
    pub action: String,
    pub device_id: String,
    pub connector_version: String,
    pub user_id: String,
    pub dealership_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceResponse {
    pub ok: bool,
    pub device_id: String,
    pub status: DeviceStatus,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub access_expires_in: Option<u64>,
    pub refresh_expires_in: Option<u64>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticateDeviceRequest {
    pub action: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticateDeviceResponse {
    pub ok: bool,
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_in: u64,
    pub refresh_expires_in: u64,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeartbeatRequest {
    pub action: String,
    #[serde(rename = "deviceId")]
    pub device_id: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "dealershipId")]
    pub dealership_id: String,
    #[serde(rename = "connectorVersion")]
    pub connector_version: String,
    pub os: ConnectorOs,
    pub capabilities: Vec<String>,
    pub connector_status: String,
    pub browser_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_version: Option<String>,
    pub profile_status: String,
    pub facebook_session_state: String,
    pub marketplace_status: String,
    pub current_browser_url_category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_browser_check_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_browser_error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_facebook_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace_ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messenger_ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications_ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_health_check: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_restart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facebook_account_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messenger_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_navigation_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_health_check_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_restart_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedeemLaunchSessionRequest {
    pub action: String,
    pub session_id: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedeemLaunchSessionResponse {
    pub ok: bool,
    pub nonce: Option<String>,
    pub expires_at: Option<String>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResponse {
    pub ok: bool,
    pub status: DeviceStatus,
    pub last_heartbeat_at: String,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollJobsRequest {
    pub action: String,
    pub device_id: String,
    pub user_id: String,
    pub dealership_id: String,
    pub connector_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorJobSummary {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollJobsResponse {
    pub ok: bool,
    pub connector_status: DeviceStatus,
    pub jobs_available: u32,
    pub jobs: Vec<ConnectorJobSummary>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePairingSessionRequest {
    pub action: String,
    pub device_id: String,
    pub connector_version: String,
    pub os: ConnectorOs,
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePairingSessionResponse {
    pub ok: bool,
    pub session_id: Option<String>,
    pub session_secret: Option<String>,
    pub pairing_code: Option<String>,
    pub expires_at: Option<String>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollPairingSessionRequest {
    pub action: String,
    pub session_id: String,
    pub session_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorBody {
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollPairingSessionResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub status: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub access_expires_in: Option<u64>,
    pub refresh_expires_in: Option<u64>,
    pub device_id: Option<String>,
    pub user_id: Option<String>,
    pub dealership_id: Option<String>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimJobRequest {
    pub action: String,
    pub job_id: String,
    pub device_id: String,
    pub user_id: String,
    pub dealership_id: String,
    pub connector_version: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimJobResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub job_id: String,
    pub scoped_job_token: Option<String>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPayloadRequest {
    pub action: String,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailJobRequest {
    pub action: String,
    pub job_id: String,
    pub error_code: String,
    pub error_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusRequest {
    pub action: String,
    pub job_id: String,
    pub status: String,
    pub progress: u8,
    pub current_step: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteJobRequest {
    pub action: String,
    pub job_id: String,
    pub listing_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePairingCodeRequest {
    pub action: String,
    pub device_id: String,
    pub connector_version: String,
    pub os: ConnectorOs,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePairingCodeResponse {
    pub ok: bool,
    pub pairing_code: String,
    pub expires_at: String,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangePairingCodeRequest {
    pub action: String,
    pub pairing_code: String,
    pub user_id: String,
    pub dealership_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangePairingCodeResponse {
    pub ok: bool,
    pub device_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_in: u64,
    pub refresh_expires_in: u64,
    pub error: Option<String>,
    pub error_code: Option<String>,
}
