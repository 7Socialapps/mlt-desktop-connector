use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::bus::{RuntimeServiceKind, ServiceBus};
use super::navigation::NavigationService;

const NOTIFICATIONS_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationState {
    NotificationsNotChecked,
    NotificationsReady,
    NotificationsLoginRequired,
    NotificationsUnavailable,
    NotificationsError,
}

impl NotificationState {
    pub fn from_sidecar(raw: &str) -> Self {
        match raw {
            "notifications_ready" => Self::NotificationsReady,
            "notifications_login_required" => Self::NotificationsLoginRequired,
            "notifications_unavailable" => Self::NotificationsUnavailable,
            "notifications_error" => Self::NotificationsError,
            _ => Self::NotificationsNotChecked,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationSnapshot {
    pub state: NotificationState,
    pub checked_at: Option<String>,
    pub current_url: Option<String>,
    pub reason_code: Option<String>,
    pub unread_count: Option<u32>,
}

impl Default for NotificationSnapshot {
    fn default() -> Self {
        Self {
            state: NotificationState::NotificationsNotChecked,
            checked_at: None,
            current_url: None,
            reason_code: None,
            unread_count: None,
        }
    }
}

pub struct NotificationService {
    bus: Arc<ServiceBus>,
    #[allow(dead_code)]
    navigation: Arc<NavigationService>,
    snapshot: Arc<Mutex<NotificationSnapshot>>,
}

impl NotificationService {
    pub fn new(bus: Arc<ServiceBus>, navigation: Arc<NavigationService>) -> Self {
        Self {
            bus,
            navigation,
            snapshot: Arc::new(Mutex::new(NotificationSnapshot::default())),
        }
    }

    pub fn snapshot(&self) -> NotificationSnapshot {
        self.snapshot.lock().clone()
    }

    pub fn is_ready(&self) -> bool {
        self.snapshot.lock().state == NotificationState::NotificationsReady
    }

    pub fn unread_count(&self) -> Option<u32> {
        self.snapshot.lock().unread_count
    }

    pub fn open_notifications(&self) -> Result<NotificationSnapshot, String> {
        self.bus
            .ensure_browser_ready(RuntimeServiceKind::Notifications)?;

        let line = self.bus.sidecar_request(
            RuntimeServiceKind::Notifications,
            "open_notifications",
            serde_json::json!({}),
            NOTIFICATIONS_TIMEOUT,
        )?;

        if line.ok == Some(false) {
            {
                let mut snap = self.snapshot.lock();
                snap.state = NotificationState::NotificationsError;
                snap.checked_at = Some(Utc::now().to_rfc3339());
            }
            return Err(line
                .error
                .unwrap_or_else(|| "Notifications navigation failed".into()));
        }

        if let Some(result) = line.result {
            self.apply_sidecar_result(result)?;
        }

        Ok(self.snapshot())
    }

    fn apply_sidecar_result(&self, result: serde_json::Value) -> Result<(), String> {
        let notifications = result
            .get("notifications")
            .cloned()
            .unwrap_or(result);
        let status = notifications
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("notifications_not_checked");
        let mut snap = self.snapshot.lock();
        snap.state = NotificationState::from_sidecar(status);
        snap.checked_at = notifications
            .get("checked_at")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(Utc::now().to_rfc3339()));
        snap.current_url = notifications
            .get("current_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        snap.reason_code = notifications
            .get("reason_code")
            .and_then(|v| v.as_str())
            .map(String::from);
        snap.unread_count = notifications
            .get("unread_count")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notification_states() {
        assert_eq!(
            NotificationState::from_sidecar("notifications_ready"),
            NotificationState::NotificationsReady
        );
        assert_eq!(
            NotificationState::from_sidecar("notifications_login_required"),
            NotificationState::NotificationsLoginRequired
        );
        assert_eq!(
            NotificationState::from_sidecar("notifications_unavailable"),
            NotificationState::NotificationsUnavailable
        );
        assert_eq!(
            NotificationState::from_sidecar("notifications_error"),
            NotificationState::NotificationsError
        );
    }

    #[test]
    fn unread_count_defaults_to_none() {
        let svc = NotificationService::new(
            Arc::new(ServiceBus::new(Arc::new(
                crate::browser::BrowserManager::new(
                    Arc::new(crate::browser::BrowserRuntimeService::new(false)),
                    Arc::new(crate::browser::SidecarDaemon::new(std::path::PathBuf::new())),
                ),
            ))),
            Arc::new(NavigationService::new(Arc::new(ServiceBus::new(
                Arc::new(crate::browser::BrowserManager::new(
                    Arc::new(crate::browser::BrowserRuntimeService::new(false)),
                    Arc::new(crate::browser::SidecarDaemon::new(std::path::PathBuf::new())),
                )),
            )))),
        );
        assert!(svc.unread_count().is_none());
    }
}
