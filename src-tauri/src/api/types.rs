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
    Unknown,
    SignedIn,
    SignedOut,
    Expired,
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
#[serde(rename_all = "camelCase")]
pub struct HeartbeatRequest {
    pub action: String,
    pub device_id: String,
    pub user_id: String,
    pub dealership_id: String,
    pub connector_version: String,
    pub os: ConnectorOs,
    pub capabilities: Vec<String>,
    pub facebook_session_state: FacebookSessionState,
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

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    pub error: Option<String>,
    pub error_code: Option<String>,
}
