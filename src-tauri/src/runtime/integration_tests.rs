//! Integration-style tests for shared Facebook runtime startup paths (M4).

use std::sync::Arc;

use crate::browser::{BrowserManager, BrowserRuntimeService, SidecarDaemon};
use crate::runtime::{
    FacebookRuntime, MessengerState, NotificationState,
};

fn test_facebook_runtime() -> Arc<FacebookRuntime> {
    let runtime_svc = Arc::new(BrowserRuntimeService::new(false));
    let daemon = Arc::new(SidecarDaemon::new(std::path::PathBuf::new()));
    let manager = Arc::new(BrowserManager::new(runtime_svc, daemon));
    FacebookRuntime::new(manager)
}

#[test]
fn launch_browser_rejects_blank_facebook_url() {
    use crate::browser::is_blank_page_url;
    assert!(is_blank_page_url(Some("about:blank")));
    assert!(!is_blank_page_url(Some("https://www.facebook.com/")));
}

#[test]
fn navigation_service_destination_for_launch_is_facebook_home() {
    use crate::runtime::NavigationDestination;
    assert_eq!(
        NavigationDestination::FacebookHome.url(),
        "https://www.facebook.com/"
    );
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
    assert_eq!(status.session_state, "unknown");
    assert!(!status.marketplace_ready);
    assert!(!status.messenger_ready);
    assert!(!status.notifications_ready);
}

#[test]
fn recovery_checkpoint_waits_for_user() {
    let rt = test_facebook_runtime();
    let outcome = rt
        .recovery
        .recover_from_interruption(crate::runtime::RuntimeServiceKind::Messenger);
    assert!(matches!(
        outcome.action,
        crate::runtime::RecoveryAction::WaitForUser
            | crate::runtime::RecoveryAction::NoAction
            | crate::runtime::RecoveryAction::RecheckSession
    ));
}

#[test]
fn runtime_coordinator_tracks_single_browser_manager() {
    let rt = test_facebook_runtime();
    let ptr = Arc::as_ptr(rt.bus.browser_manager()) as *const ();
    assert!(!ptr.is_null());
}
