//! Integration-style tests for shared Facebook runtime startup paths (M4).

use std::sync::Arc;

use crate::browser::{BrowserManager, BrowserRuntimeService, SidecarDaemon};
use crate::runtime::{
    FacebookRuntime, MessengerState, NavigationDestination, NotificationState,
};

fn test_facebook_runtime() -> Arc<FacebookRuntime> {
    let runtime_svc = Arc::new(BrowserRuntimeService::new(false));
    let daemon = Arc::new(SidecarDaemon::new(std::path::PathBuf::new()));
    let manager = Arc::new(BrowserManager::new(runtime_svc, daemon));
    FacebookRuntime::new(manager)
}

#[test]
fn marketplace_startup_path_defaults_to_not_checked() {
    let rt = test_facebook_runtime();
    let snap = rt.marketplace.snapshot();
    assert!(!rt.marketplace.is_ready());
    assert_eq!(
        snap.status,
        crate::browser::MarketplaceStatus::MarketplaceNotChecked
    );
}

#[test]
fn messenger_startup_path_defaults_to_not_checked() {
    let rt = test_facebook_runtime();
    assert!(!rt.messenger.is_ready());
    assert_eq!(
        rt.messenger.snapshot().state,
        MessengerState::MessengerNotChecked
    );
}

#[test]
fn notification_startup_path_defaults_to_not_checked() {
    let rt = test_facebook_runtime();
    assert!(!rt.notifications.is_ready());
    assert_eq!(
        rt.notifications.snapshot().state,
        NotificationState::NotificationsNotChecked
    );
    assert!(rt.notifications.unread_count().is_none());
}

#[test]
fn shared_browser_session_concept_via_runtime_status() {
    let rt = test_facebook_runtime();
    let status = rt.aggregate_status();
    assert_eq!(status.session_state, "facebook_not_checked");
    assert!(!status.marketplace_ready);
    assert!(!status.messenger_ready);
    assert!(!status.notifications_ready);
}

#[test]
fn navigation_destination_mapping_matches_sidecar_keys() {
    assert_eq!(
        NavigationDestination::Marketplace.sidecar_key(),
        "marketplace"
    );
    assert_eq!(
        NavigationDestination::Messenger.url(),
        "https://www.facebook.com/messages/"
    );
}
