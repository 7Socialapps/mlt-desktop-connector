use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeStatus {
    BrowserNotInstalled,
    BrowserInstalling,
    BrowserInstalled,
    BrowserStarting,
    BrowserReady,
    BrowserStopped,
    BrowserCrashed,
    BrowserRestarting,
    BrowserError,
}

impl BrowserRuntimeStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::BrowserNotInstalled => "Chromium is not installed",
            Self::BrowserInstalling => "Installing Chromium",
            Self::BrowserInstalled => "Chromium installed",
            Self::BrowserStarting => "Browser is starting",
            Self::BrowserReady => "Browser is ready",
            Self::BrowserStopped => "Browser stopped",
            Self::BrowserCrashed => "Browser crashed",
            Self::BrowserRestarting => "Browser restarting",
            Self::BrowserError => "Browser error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BrowserRuntimeSnapshot {
    pub status: BrowserRuntimeStatus,
    pub enabled: bool,
    pub playwright_installed: bool,
    pub playwright_version: Option<String>,
    pub chromium_installed: bool,
    pub chromium_path: Option<String>,
    pub node_version: Option<String>,
    pub last_error: Option<String>,
    pub last_error_code: Option<String>,
    pub checked_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SidecarDetectResponse {
    pub ok: bool,
    pub playwright_installed: bool,
    pub playwright_version: Option<String>,
    pub chromium_installed: bool,
    pub chromium_path: Option<String>,
    pub node_version: Option<String>,
    pub detect_error: Option<String>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidecarSimpleResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, rename = "error_code")]
    pub error_code: Option<String>,
}
