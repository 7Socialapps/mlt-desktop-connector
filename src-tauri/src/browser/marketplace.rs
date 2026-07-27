use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceStatus {
    MarketplaceNotChecked,
    MarketplaceLoading,
    MarketplaceReady,
    MarketplaceLoginRequired,
    MarketplaceCheckpoint,
    MarketplaceUnavailable,
    MarketplaceError,
}

impl MarketplaceStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MarketplaceNotChecked => "Marketplace not checked",
            Self::MarketplaceLoading => "Opening Marketplace",
            Self::MarketplaceReady => "Marketplace ready",
            Self::MarketplaceLoginRequired => "Facebook login required for Marketplace",
            Self::MarketplaceCheckpoint => "Facebook checkpoint blocking Marketplace",
            Self::MarketplaceUnavailable => "Marketplace unavailable",
            Self::MarketplaceError => "Marketplace navigation error",
        }
    }

    pub fn remediation(&self) -> Option<&'static str> {
        match self {
            Self::MarketplaceLoginRequired => Some(
                "Sign into Facebook first using Open Facebook Login, then try again.",
            ),
            Self::MarketplaceCheckpoint => Some(
                "Complete Facebook's security checkpoint, then retry Open Marketplace.",
            ),
            Self::MarketplaceUnavailable => {
                Some("Facebook Marketplace may be unavailable in your region or account.")
            }
            Self::MarketplaceError => Some(
                "Check diagnostics folder for a screenshot, then restart the browser.",
            ),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarketplaceSnapshot {
    pub status: MarketplaceStatus,
    pub checked_at: Option<String>,
    pub current_url: Option<String>,
    pub reason_code: Option<String>,
    pub screenshot_path: Option<String>,
}

impl Default for MarketplaceSnapshot {
    fn default() -> Self {
        Self {
            status: MarketplaceStatus::MarketplaceNotChecked,
            checked_at: None,
            current_url: None,
            reason_code: None,
            screenshot_path: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SidecarMarketplaceResult {
    pub status: String,
    pub checked_at: String,
    pub current_url: String,
    pub reason_code: String,
    #[serde(default)]
    pub screenshot_path: Option<String>,
}

pub fn parse_marketplace_status(raw: &str) -> MarketplaceStatus {
    match raw {
        "marketplace_loading" => MarketplaceStatus::MarketplaceLoading,
        "marketplace_ready" => MarketplaceStatus::MarketplaceReady,
        "marketplace_login_required" => MarketplaceStatus::MarketplaceLoginRequired,
        "marketplace_checkpoint" => MarketplaceStatus::MarketplaceCheckpoint,
        "marketplace_unavailable" => MarketplaceStatus::MarketplaceUnavailable,
        "marketplace_error" => MarketplaceStatus::MarketplaceError,
        _ => MarketplaceStatus::MarketplaceNotChecked,
    }
}

pub fn apply_marketplace_result(snapshot: &mut MarketplaceSnapshot, raw: &SidecarMarketplaceResult) {
    snapshot.status = parse_marketplace_status(&raw.status);
    snapshot.checked_at = Some(raw.checked_at.clone());
    snapshot.current_url = Some(raw.current_url.clone());
    snapshot.reason_code = Some(raw.reason_code.clone());
    snapshot.screenshot_path = raw.screenshot_path.clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_marketplace_ready() {
        assert_eq!(
            parse_marketplace_status("marketplace_ready"),
            MarketplaceStatus::MarketplaceReady
        );
    }

    #[test]
    fn parse_login_required() {
        assert_eq!(
            parse_marketplace_status("marketplace_login_required"),
            MarketplaceStatus::MarketplaceLoginRequired
        );
    }

    #[test]
    fn apply_result_updates_snapshot() {
        let mut snap = MarketplaceSnapshot::default();
        apply_marketplace_result(
            &mut snap,
            &SidecarMarketplaceResult {
                status: "marketplace_ready".into(),
                checked_at: Utc::now().to_rfc3339(),
                current_url: "https://www.facebook.com/marketplace/".into(),
                reason_code: "marketplace_loaded".into(),
                screenshot_path: None,
            },
        );
        assert_eq!(snap.status, MarketplaceStatus::MarketplaceReady);
    }
}
