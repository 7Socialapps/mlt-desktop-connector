use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::browser::{
    is_blank_page_url, parse_marketplace_status, MarketplaceStatus, SidecarMarketplaceResult,
};

use super::bus::{RuntimeServiceKind, ServiceBus};
use super::navigation::{NavigationDestination, NavigationService};
use super::session::FacebookSessionService;

const MARKETPLACE_TIMEOUT: Duration = Duration::from_secs(90);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(45);

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VehicleCreateVerification {
    pub ready: bool,
    pub reason_code: String,
    pub current_url: Option<String>,
    pub page_title: Option<String>,
    pub checked_at: Option<String>,
    pub screenshot_path: Option<String>,
    pub signals_met: Vec<String>,
    pub signals_missing: Vec<String>,
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

    /// Navigate to vehicle create route via NavigationService, then verify form readiness.
    pub fn open_vehicle_create_route(&self) -> Result<MarketplaceServiceSnapshot, String> {
        {
            let mut snap = self.snapshot.lock();
            snap.status = MarketplaceStatus::MarketplaceLoading;
            snap.checked_at = Some(Utc::now().to_rfc3339());
        }
        self.bus
            .browser_manager()
            .set_marketplace_loading();

        self.bus.ensure_browser_ready(RuntimeServiceKind::Marketplace)?;

        let nav = self
            .navigation
            .navigate_with_recovery(NavigationDestination::MarketplaceCreateVehicle)?;

        if !nav.ready {
            return Err(format!(
                "Vehicle create navigation not ready (url={:?})",
                nav.current_url
            ));
        }

        self.bus
            .record_navigation_success(NavigationDestination::MarketplaceCreateVehicle.sidecar_key());

        let verification = self.verify_vehicle_create_form()?;
        if !verification.ready {
            let mut snap = self.snapshot.lock();
            snap.status = MarketplaceStatus::MarketplaceError;
            snap.reason_code = Some(verification.reason_code.clone());
            snap.current_url = verification.current_url.clone();
            snap.screenshot_path = verification.screenshot_path.clone();
            snap.checked_at = verification.checked_at.clone();
            return Err(format!(
                "Vehicle create form not ready: {}",
                verification.reason_code
            ));
        }

        {
            let mut snap = self.snapshot.lock();
            snap.status = MarketplaceStatus::MarketplaceReady;
            snap.reason_code = Some(verification.reason_code.clone());
            snap.current_url = verification.current_url.clone();
            snap.checked_at = verification.checked_at.clone();
            snap.screenshot_path = verification.screenshot_path.clone();
        }

        self.bus
            .browser_manager()
            .update_marketplace_from_sidecar(&crate::browser::SidecarMarketplaceResult {
                status: "marketplace_ready".into(),
                checked_at: Utc::now().to_rfc3339(),
                current_url: verification
                    .current_url
                    .clone()
                    .unwrap_or_else(|| NavigationDestination::MarketplaceCreateVehicle.url().into()),
                reason_code: verification.reason_code.clone(),
                screenshot_path: verification.screenshot_path.clone(),
            });

        Ok(self.snapshot())
    }

    pub fn verify_vehicle_create_form(&self) -> Result<VehicleCreateVerification, String> {
        self.bus.ensure_browser_ready(RuntimeServiceKind::Marketplace)?;

        let line = self.bus.sidecar_request(
            RuntimeServiceKind::Marketplace,
            "verify_vehicle_create",
            serde_json::json!({}),
            VERIFY_TIMEOUT,
        )?;

        if line.ok == Some(false) {
            if let Some(result) = line.result.clone() {
                if let Some(vc) = result.get("vehicle_create") {
                    return parse_vehicle_create_verification(vc.clone());
                }
            }
            return Err(line
                .error
                .unwrap_or_else(|| "Vehicle create verification failed".into()));
        }

        let result = line.result.ok_or_else(|| {
            "verify_vehicle_create returned no result".to_string()
        })?;

        let vc_value = result
            .get("vehicle_create")
            .cloned()
            .unwrap_or(result);

        parse_vehicle_create_verification(vc_value)
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

        let nav = self.navigation.navigate_with_recovery(destination)?;
        if is_blank_page_url(nav.current_url.as_deref()) {
            return Err(format!(
                "Marketplace navigation left browser on blank page (destination={})",
                destination.sidecar_key()
            ));
        }

        let line = self.bus.sidecar_request(
            RuntimeServiceKind::Marketplace,
            "open_marketplace",
            serde_json::json!({
                "create_vehicle": create_vehicle,
                "skip_navigation": true,
            }),
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

fn parse_vehicle_create_verification(
    value: serde_json::Value,
) -> Result<VehicleCreateVerification, String> {
    serde_json::from_value(value)
        .map_err(|e| format!("invalid vehicle create verification payload: {e}"))
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
        let navigation = Arc::new(NavigationService::new(bus.clone()));
        let session = Arc::new(FacebookSessionService::new(bus.clone(), navigation.clone()));
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

    #[test]
    fn parse_vehicle_create_verification_payload() {
        let vc = parse_vehicle_create_verification(serde_json::json!({
            "ready": true,
            "reason_code": "vehicle_create_ready",
            "current_url": "https://www.facebook.com/marketplace/create/vehicle",
            "signals_met": ["vehicle_create_url"],
            "signals_missing": []
        }))
        .unwrap();
        assert!(vc.ready);
        assert_eq!(vc.reason_code, "vehicle_create_ready");
    }
}
