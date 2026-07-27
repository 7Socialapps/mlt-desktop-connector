use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::bus::{RuntimeServiceKind, ServiceBus};

const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(45);
const NAVIGATION_RPC_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationDestination {
    FacebookHome,
    Marketplace,
    MarketplaceCreateVehicle,
    Messenger,
    Notifications,
}

impl NavigationDestination {
    pub fn sidecar_key(&self) -> &'static str {
        match self {
            Self::FacebookHome => "facebook_home",
            Self::Marketplace => "marketplace",
            Self::MarketplaceCreateVehicle => "marketplace_create_vehicle",
            Self::Messenger => "messenger",
            Self::Notifications => "notifications",
        }
    }

    pub fn url(&self) -> &'static str {
        match self {
            Self::FacebookHome => "https://www.facebook.com/",
            Self::Marketplace => "https://www.facebook.com/marketplace/",
            Self::MarketplaceCreateVehicle => {
                "https://www.facebook.com/marketplace/create/vehicle"
            }
            Self::Messenger => "https://www.facebook.com/messages/",
            Self::Notifications => "https://www.facebook.com/notifications",
        }
    }

    pub fn from_sidecar_key(key: &str) -> Option<Self> {
        match key {
            "facebook_home" => Some(Self::FacebookHome),
            "marketplace" => Some(Self::Marketplace),
            "marketplace_create_vehicle" => Some(Self::MarketplaceCreateVehicle),
            "messenger" => Some(Self::Messenger),
            "notifications" => Some(Self::Notifications),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NavigationResult {
    pub destination: String,
    pub current_url: Option<String>,
    pub page_title: Option<String>,
    pub attempt: Option<u32>,
    pub checked_at: String,
    pub ready: bool,
}

pub struct NavigationService {
    bus: Arc<ServiceBus>,
}

impl NavigationService {
    pub fn new(bus: Arc<ServiceBus>) -> Self {
        Self { bus }
    }

    pub fn navigate(&self, destination: NavigationDestination) -> Result<NavigationResult, String> {
        self.bus.ensure_browser_ready(RuntimeServiceKind::Navigation)?;

        let dest_key = destination.sidecar_key();
        let line = self.bus.sidecar_request(
            RuntimeServiceKind::Navigation,
            "navigate",
            serde_json::json!({ "destination": dest_key }),
            NAVIGATION_RPC_TIMEOUT,
        );

        match &line {
            Ok(response) if response.ok != Some(false) => {
                self.bus.record_navigation_success(dest_key);
            }
            Ok(response) => {
                let msg = response
                    .error
                    .clone()
                    .unwrap_or_else(|| "Navigation failed".into());
                self.bus.record_navigation_error(dest_key, &msg);
            }
            Err(err) => {
                self.bus.record_navigation_error(dest_key, err);
            }
        }

        let line = line?;

        if line.ok == Some(false) {
            return Err(line
                .error
                .unwrap_or_else(|| "Navigation failed".into()));
        }

        let result = line.result.unwrap_or_default();
        let current_url = result
            .get("current_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let page_title = result
            .get("page_title")
            .and_then(|v| v.as_str())
            .map(String::from);
        let attempt = result
            .get("attempt")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let redirect_detected = result
            .get("redirect_detected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if redirect_detected {
            self.bus.record_navigation_error(
                dest_key,
                "Redirect detected — destination may require user attention",
            );
        }

        Ok(NavigationResult {
            destination: dest_key.into(),
            current_url: current_url.clone(),
            page_title,
            attempt,
            checked_at: Utc::now().to_rfc3339(),
            ready: current_url
                .as_deref()
                .map(|url| url.starts_with("https://www.facebook.com"))
                .unwrap_or(false)
                && !redirect_detected,
        })
    }

    pub fn navigate_with_recovery(
        &self,
        destination: NavigationDestination,
    ) -> Result<NavigationResult, String> {
        match self.navigate(destination) {
            Ok(result) => Ok(result),
            Err(first) => {
                tracing::warn!(
                    destination = destination.sidecar_key(),
                    error = %first,
                    "navigation failed — retrying once"
                );
                self.navigate(destination)
            }
        }
    }

    pub fn timeout(&self) -> Duration {
        NAVIGATION_TIMEOUT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_url_mapping() {
        assert_eq!(
            NavigationDestination::Marketplace.url(),
            "https://www.facebook.com/marketplace/"
        );
        assert_eq!(
            NavigationDestination::MarketplaceCreateVehicle.url(),
            "https://www.facebook.com/marketplace/create/vehicle"
        );
        assert_eq!(
            NavigationDestination::Messenger.url(),
            "https://www.facebook.com/messages/"
        );
        assert_eq!(
            NavigationDestination::Notifications.url(),
            "https://www.facebook.com/notifications"
        );
    }

    #[test]
    fn sidecar_key_round_trips() {
        for dest in [
            NavigationDestination::FacebookHome,
            NavigationDestination::Marketplace,
            NavigationDestination::MarketplaceCreateVehicle,
            NavigationDestination::Messenger,
            NavigationDestination::Notifications,
        ] {
            assert_eq!(
                NavigationDestination::from_sidecar_key(dest.sidecar_key()),
                Some(dest)
            );
        }
    }

    #[test]
    fn unknown_destination_key_returns_none() {
        assert!(NavigationDestination::from_sidecar_key("unknown").is_none());
    }

    #[test]
    fn destination_serializes_snake_case() {
        let json = serde_json::to_value(NavigationDestination::MarketplaceCreateVehicle).unwrap();
        assert_eq!(json, "marketplace_create_vehicle");
    }
}
