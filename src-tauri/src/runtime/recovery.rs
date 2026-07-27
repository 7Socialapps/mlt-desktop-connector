use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::browser::{BrowserRuntimeStatus, FacebookSessionState};

use super::bus::{RuntimeServiceKind, ServiceBus};
use super::navigation::NavigationService;
use super::session::FacebookSessionService;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    RestartBrowser,
    RecheckSession,
    NavigateHome,
    WaitForUser,
    NoAction,
    TerminalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecoveryOutcome {
    pub action: RecoveryAction,
    pub message: String,
    pub recovered: bool,
}

pub struct RecoveryService {
    bus: Arc<ServiceBus>,
    session: Arc<FacebookSessionService>,
    #[allow(dead_code)]
    navigation: Arc<NavigationService>,
}

impl RecoveryService {
    pub fn new(
        bus: Arc<ServiceBus>,
        session: Arc<FacebookSessionService>,
        navigation: Arc<NavigationService>,
    ) -> Self {
        Self {
            bus,
            session,
            navigation,
        }
    }

    pub fn recover_from_interruption(
        &self,
        service: RuntimeServiceKind,
    ) -> RecoveryOutcome {
        let browser = self.bus.browser_manager().snapshot();
        let policy = classify_interruption(&browser.status, &self.session.snapshot().state);

        match policy {
            InterruptionKind::BrowserCrash => {
                if browser.status.is_terminal_error() {
                    return RecoveryOutcome {
                        action: RecoveryAction::TerminalError,
                        message: "Browser crash limit reached".into(),
                        recovered: false,
                    };
                }
                if let Err(err) = self.bus.browser_manager().restart() {
                    return RecoveryOutcome {
                        action: RecoveryAction::RestartBrowser,
                        message: err,
                        recovered: false,
                    };
                }
                self.bus.record_restart();
                RecoveryOutcome {
                    action: RecoveryAction::RestartBrowser,
                    message: "Browser restarted after crash".into(),
                    recovered: true,
                }
            }
            InterruptionKind::Checkpoint | InterruptionKind::MfaRequired => RecoveryOutcome {
                action: RecoveryAction::WaitForUser,
                message: "Manual Facebook action required before services can continue".into(),
                recovered: false,
            },
            InterruptionKind::LoggedOut | InterruptionKind::SessionExpired => {
                let _ = self.session.check_session();
                RecoveryOutcome {
                    action: RecoveryAction::RecheckSession,
                    message: "Facebook session expired — user must sign in".into(),
                    recovered: false,
                }
            }
            InterruptionKind::TemporaryRestriction | InterruptionKind::DisabledAccount => {
                RecoveryOutcome {
                    action: RecoveryAction::WaitForUser,
                    message: "Facebook account restriction — manual resolution required".into(),
                    recovered: false,
                }
            }
            InterruptionKind::NetworkInterruption => RecoveryOutcome {
                action: RecoveryAction::RecheckSession,
                message: "Network interruption — session recheck scheduled".into(),
                recovered: false,
            },
            InterruptionKind::ConnectorRestart => {
                let _ = self.session.check_session();
                RecoveryOutcome {
                    action: RecoveryAction::RecheckSession,
                    message: "Connector restarted — session rechecked".into(),
                    recovered: true,
                }
            }
            InterruptionKind::FacebookRedirect => RecoveryOutcome {
                action: RecoveryAction::NavigateHome,
                message: format!(
                    "Unexpected Facebook redirect during {:?} — navigate home and retry",
                    service
                ),
                recovered: false,
            },
            InterruptionKind::None => RecoveryOutcome {
                action: RecoveryAction::NoAction,
                message: "No recovery action required".into(),
                recovered: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterruptionKind {
    None,
    BrowserCrash,
    Checkpoint,
    MfaRequired,
    LoggedOut,
    SessionExpired,
    TemporaryRestriction,
    DisabledAccount,
    NetworkInterruption,
    ConnectorRestart,
    FacebookRedirect,
}

fn classify_interruption(
    browser_status: &BrowserRuntimeStatus,
    session_state: &FacebookSessionState,
) -> InterruptionKind {
    if *browser_status == BrowserRuntimeStatus::BrowserCrashed {
        return InterruptionKind::BrowserCrash;
    }
    if *browser_status == BrowserRuntimeStatus::BrowserError {
        return InterruptionKind::BrowserCrash;
    }
    match session_state {
        FacebookSessionState::FacebookCheckpoint => InterruptionKind::Checkpoint,
        FacebookSessionState::FacebookMfaRequired => InterruptionKind::MfaRequired,
        FacebookSessionState::FacebookLoggedOut => InterruptionKind::LoggedOut,
        FacebookSessionState::FacebookSessionExpired => InterruptionKind::SessionExpired,
        FacebookSessionState::FacebookTemporaryRestriction => InterruptionKind::TemporaryRestriction,
        FacebookSessionState::FacebookDisabledAccount => InterruptionKind::DisabledAccount,
        FacebookSessionState::FacebookError => InterruptionKind::NetworkInterruption,
        _ => InterruptionKind::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserRuntimeService, SidecarDaemon};
    use std::path::PathBuf;

    fn test_recovery() -> RecoveryService {
        let runtime = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(PathBuf::new()));
        let manager = Arc::new(crate::browser::BrowserManager::new(runtime, daemon));
        let bus = Arc::new(ServiceBus::new(manager));
        let session = Arc::new(FacebookSessionService::new(bus.clone()));
        let navigation = Arc::new(NavigationService::new(bus));
        RecoveryService::new(
            Arc::new(ServiceBus::new(Arc::new(crate::browser::BrowserManager::new(
                Arc::new(BrowserRuntimeService::new(false)),
                Arc::new(SidecarDaemon::new(PathBuf::new())),
            )))),
            session,
            navigation,
        )
    }

    #[test]
    fn checkpoint_requires_user_action() {
        let outcome = classify_interruption(
            &BrowserRuntimeStatus::BrowserReady,
            &FacebookSessionState::FacebookCheckpoint,
        );
        assert_eq!(outcome, InterruptionKind::Checkpoint);
    }

    #[test]
    fn browser_crash_triggers_restart_policy() {
        let outcome = classify_interruption(
            &BrowserRuntimeStatus::BrowserCrashed,
            &FacebookSessionState::FacebookLoggedIn,
        );
        assert_eq!(outcome, InterruptionKind::BrowserCrash);
    }

    #[test]
    fn temporary_restriction_waits_for_user() {
        let recovery = test_recovery();
        {
            let session = recovery.session.snapshot();
            let _ = session;
        }
        let outcome = classify_interruption(
            &BrowserRuntimeStatus::BrowserReady,
            &FacebookSessionState::FacebookTemporaryRestriction,
        );
        assert_eq!(outcome, InterruptionKind::TemporaryRestriction);
    }

    #[test]
    fn logged_out_triggers_session_recheck() {
        let outcome = classify_interruption(
            &BrowserRuntimeStatus::BrowserReady,
            &FacebookSessionState::FacebookLoggedOut,
        );
        assert_eq!(outcome, InterruptionKind::LoggedOut);
    }

    #[test]
    fn healthy_state_needs_no_recovery() {
        let outcome = classify_interruption(
            &BrowserRuntimeStatus::BrowserReady,
            &FacebookSessionState::FacebookLoggedIn,
        );
        assert_eq!(outcome, InterruptionKind::None);
    }
}
