pub mod bus;
pub mod marketplace;
pub mod messenger;
pub mod navigation;
pub mod notifications;
pub mod recovery;
pub mod session;
pub mod status;

pub use bus::{RuntimeServiceKind, ServiceBus};
pub use marketplace::MarketplaceService;
pub use messenger::{MessengerService, MessengerState};
pub use navigation::{NavigationDestination, NavigationService};
pub use notifications::{NotificationService, NotificationState};
pub use recovery::{RecoveryAction, RecoveryOutcome, RecoveryService};
pub use session::FacebookSessionService;
pub use status::FacebookRuntimeStatus;

#[cfg(test)]
mod integration_tests;

use std::sync::Arc;

use parking_lot::Mutex;

use crate::browser::BrowserManager;
use crate::browser::SidecarDaemonLine;

pub struct FacebookRuntime {
    pub bus: Arc<ServiceBus>,
    pub session: Arc<FacebookSessionService>,
    pub navigation: Arc<NavigationService>,
    pub marketplace: Arc<MarketplaceService>,
    pub messenger: Arc<MessengerService>,
    pub notifications: Arc<NotificationService>,
    pub recovery: Arc<RecoveryService>,
    last_status: Arc<Mutex<FacebookRuntimeStatus>>,
}

impl FacebookRuntime {
    pub fn new(browser_manager: Arc<BrowserManager>) -> Arc<Self> {
        let bus = Arc::new(ServiceBus::new(browser_manager));
        let session = Arc::new(FacebookSessionService::new(bus.clone()));
        let navigation = Arc::new(NavigationService::new(bus.clone()));
        let marketplace = Arc::new(MarketplaceService::new(
            bus.clone(),
            session.clone(),
            navigation.clone(),
        ));
        let messenger = Arc::new(MessengerService::new(bus.clone(), navigation.clone()));
        let notifications = Arc::new(NotificationService::new(bus.clone(), navigation.clone()));
        let recovery = Arc::new(RecoveryService::new(
            bus.clone(),
            session.clone(),
            navigation.clone(),
        ));

        let runtime = Arc::new(Self {
            bus: bus.clone(),
            session,
            navigation,
            marketplace,
            messenger,
            notifications,
            recovery,
            last_status: Arc::new(Mutex::new(FacebookRuntimeStatus::default())),
        });

        runtime
    }

    pub fn aggregate_status(&self) -> FacebookRuntimeStatus {
        let status = FacebookRuntimeStatus::aggregate(
            &self.bus,
            &self.session,
            &self.marketplace,
            &self.messenger,
            &self.notifications,
        );
        *self.last_status.lock() = status.clone();
        status
    }

    pub fn cached_status(&self) -> FacebookRuntimeStatus {
        self.last_status.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserRuntimeService, SidecarDaemon};
    use std::path::PathBuf;

    fn test_runtime() -> Arc<FacebookRuntime> {
        let runtime_svc = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(PathBuf::new()));
        let manager = Arc::new(BrowserManager::new(runtime_svc, daemon));
        FacebookRuntime::new(manager)
    }

    #[test]
    fn runtime_initializes_all_services() {
        let rt = test_runtime();
        assert_eq!(rt.bus.current_service_name(), None);
        let status = rt.aggregate_status();
        assert!(!status.marketplace_ready);
        assert!(!status.messenger_ready);
        assert!(!status.notifications_ready);
    }
}
