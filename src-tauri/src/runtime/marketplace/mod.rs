use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::browser::{
    parse_marketplace_status, MarketplaceStatus, SidecarMarketplaceResult,
};

use super::bus::{RuntimeServiceKind, ServiceBus};
use super::navigation::{NavigationDestination, NavigationService};
use super::session::FacebookSessionService;

const MARKETPLACE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarketplaceServiceSnapshot {
    pub status: MarketplaceStatus,
    pub checked_at: Option<String>,
    pub current_url: Option<String>,
    pub reason_code: Option<String>,
    pub screenshot_path: Option<String>,
}

impl Default for MarketplaceServiceSnapshot {
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

pub struct MarketplaceService {
    bus: Arc<ServiceBus>,
    #[allow(dead_code)]
    session: Arc<FacebookSessionService>,
    navigation: Arc<NavigationService>,
    snapshot: Arc<Mutex<MarketplaceServiceSnapshot>>,
}

impl MarketplaceService {
    pub fn new(
        bus: Arc<ServiceBus>,
        session: Arc<FacebookSessionService>,
        navigation: Arc<NavigationService>,
    ) -> Self {
        Self {
            bus,
            session,
            navigation,
            snapshot: Arc::new(Mutex::new(MarketplaceServiceSnapshot::default())),
        }
    }

    pub fn snapshot(&self) -> MarketplaceServiceSnapshot {
        self.snapshot.lock().clone()
    }

    pub fn is_ready(&self) -> bool {
        self.snapshot.lock().status == MarketplaceStatus::MarketplaceReady
    }

    pub fn open_marketplace(&self) -> Result<MarketplaceServiceSnapshot, String> {
        self.open_destination(NavigationDestination::Marketplace, false)
    }

    pub fn open_create_listing(&self) -> Result<MarketplaceServiceSnapshot, String> {
        self.open_destination(NavigationDestination::MarketplaceCreateVehicle, true)
    }

    fn open_destination(
        &self,
        destination: NavigationDestination,
        create_vehicle: bool,
    ) -> Result<MarketplaceServiceSnapshot, String> {
        {
            let mut snap = self.snapshot.lock();
            snap.status = MarketplaceStatus::MarketplaceLoading;
            snap.checked_at = Some(Utc::now().to_rfc3339());
        }
        self.bus
            .browser_manager()
            .set_marketplace_loading();

        self.bus.ensure_browser_ready(RuntimeServiceKind::Marketplace)?;

        let line = self.bus.sidecar_request(
            RuntimeServiceKind::Marketplace,
            "open_marketplace",
            serde_json::json!({ "create_vehicle": create_vehicle }),
            MARKETPLACE_TIMEOUT,
        )?;

        if let Some(result) = line.result.clone() {
            self.apply_sidecar_marketplace(result)?;
        }

        if line.ok == Some(false) {
            return Err(line
                .error
                .unwrap_or_else(|| "Marketplace navigation failed".into()));
        }

        let _ = destination;
        Ok(self.snapshot())
    }

    fn apply_sidecar_marketplace(&self, result: serde_json::Value) -> Result<(), String> {
        let mp_value = result
            .get("marketplace")
            .cloned()
            .unwrap_or(result);
        let raw: SidecarMarketplaceResult = serde_json::from_value(mp_value)
            .map_err(|e| format!("invalid marketplace result: {e}"))?;

        let mut snap = self.snapshot.lock();
        snap.status = parse_marketplace_status(&raw.status);
        snap.checked_at = Some(raw.checked_at.clone());
        snap.current_url = Some(raw.current_url.clone());
        snap.reason_code = Some(raw.reason_code.clone());
        snap.screenshot_path = raw.screenshot_path.clone();

        self.bus
            .browser_manager()
            .update_marketplace_from_sidecar(&raw);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserRuntimeService, SidecarDaemon};
    use std::path::PathBuf;

    fn test_marketplace_service() -> MarketplaceService {
        let runtime = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(PathBuf::new()));
        let manager = Arc::new(crate::browser::BrowserManager::new(runtime, daemon));
        let bus = Arc::new(ServiceBus::new(manager));
        let session = Arc::new(FacebookSessionService::new(bus.clone()));
        let navigation = Arc::new(NavigationService::new(bus.clone()));
        MarketplaceService::new(bus, session, navigation)
    }

    #[test]
    fn marketplace_not_ready_by_default() {
        let svc = test_marketplace_service();
        assert!(!svc.is_ready());
        assert_eq!(
            svc.snapshot().status,
            MarketplaceStatus::MarketplaceNotChecked
        );
    }

    #[test]
    fn apply_sidecar_result_marks_ready() {
        let svc = test_marketplace_service();
        svc.apply_sidecar_marketplace(serde_json::json!({
            "status": "marketplace_ready",
            "checked_at": Utc::now().to_rfc3339(),
            "current_url": "https://www.facebook.com/marketplace/",
            "reason_code": "marketplace_loaded"
        }))
        .unwrap();
        assert!(svc.is_ready());
    }
}
