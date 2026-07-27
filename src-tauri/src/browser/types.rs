use serde::{Deserialize, Serialize};

use super::profile::ProfileStatus;
use super::facebook::FacebookSessionSnapshot;
use super::marketplace::MarketplaceSnapshot;

pub const MAX_AUTO_RESTART_ATTEMPTS: u32 = 5;

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

    pub fn is_operational(&self) -> bool {
        matches!(
            self,
            Self::BrowserReady | Self::BrowserStarting | Self::BrowserRestarting
        )
    }

    pub fn is_terminal_error(&self) -> bool {
        matches!(self, Self::BrowserError)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BrowserManagerSnapshot {
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
    pub sidecar_running: bool,
    pub browser_pid: Option<u32>,
    pub active_page_url: Option<String>,
    pub active_page_title: Option<String>,
    pub restart_attempts: u32,
    pub max_restart_attempts: u32,
    pub last_health_check_at: Option<String>,
    pub auto_restart_enabled: bool,
    pub profile_status: ProfileStatus,
    pub profile_path: Option<String>,
    pub facebook_session: FacebookSessionSnapshot,
    pub marketplace: MarketplaceSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BrowserActivePage {
    pub url: String,
    pub title: String,
    pub pid: Option<u32>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SidecarStatusResult {
    #[serde(default)]
    pub browser_state: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub browser_connected: Option<bool>,
    #[serde(default)]
    pub process_alive: Option<bool>,
    #[serde(default)]
    pub profile_status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SidecarPageResult {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidecarDaemonLine {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, rename = "error_code")]
    pub error_code: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

impl BrowserManagerSnapshot {
    pub fn from_runtime(runtime: &BrowserRuntimeSnapshot) -> Self {
        Self {
            status: runtime.status,
            enabled: runtime.enabled,
            playwright_installed: runtime.playwright_installed,
            playwright_version: runtime.playwright_version.clone(),
            chromium_installed: runtime.chromium_installed,
            chromium_path: runtime.chromium_path.clone(),
            node_version: runtime.node_version.clone(),
            last_error: runtime.last_error.clone(),
            last_error_code: runtime.last_error_code.clone(),
            checked_at: runtime.checked_at.clone(),
            sidecar_running: false,
            browser_pid: None,
            active_page_url: None,
            active_page_title: None,
            restart_attempts: 0,
            max_restart_attempts: MAX_AUTO_RESTART_ATTEMPTS,
            last_health_check_at: None,
            auto_restart_enabled: true,
            profile_status: ProfileStatus::ProfileMissing,
            profile_path: None,
            facebook_session: FacebookSessionSnapshot::default(),
            marketplace: MarketplaceSnapshot::default(),
        }
    }
}
