use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::bus::{RuntimeServiceKind, ServiceBus};
use super::navigation::NavigationService;

const MESSENGER_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessengerState {
    MessengerNotChecked,
    MessengerReady,
    MessengerLoginRequired,
    MessengerCheckpoint,
    MessengerUnavailable,
    MessengerError,
}

impl MessengerState {
    pub fn from_sidecar(raw: &str) -> Self {
        match raw {
            "messenger_ready" => Self::MessengerReady,
            "messenger_login_required" => Self::MessengerLoginRequired,
            "messenger_checkpoint" => Self::MessengerCheckpoint,
            "messenger_unavailable" => Self::MessengerUnavailable,
            "messenger_error" => Self::MessengerError,
            _ => Self::MessengerNotChecked,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessengerSnapshot {
    pub state: MessengerState,
    pub checked_at: Option<String>,
    pub current_url: Option<String>,
    pub reason_code: Option<String>,
}

impl Default for MessengerSnapshot {
    fn default() -> Self {
        Self {
            state: MessengerState::MessengerNotChecked,
            checked_at: None,
            current_url: None,
            reason_code: None,
        }
    }
}

pub struct MessengerService {
    bus: Arc<ServiceBus>,
    #[allow(dead_code)]
    navigation: Arc<NavigationService>,
    snapshot: Arc<Mutex<MessengerSnapshot>>,
}

impl MessengerService {
    pub fn new(bus: Arc<ServiceBus>, navigation: Arc<NavigationService>) -> Self {
        Self {
            bus,
            navigation,
            snapshot: Arc::new(Mutex::new(MessengerSnapshot::default())),
        }
    }

    pub fn snapshot(&self) -> MessengerSnapshot {
        self.snapshot.lock().clone()
    }

    pub fn is_ready(&self) -> bool {
        self.snapshot.lock().state == MessengerState::MessengerReady
    }

    pub fn open_messenger(&self) -> Result<MessengerSnapshot, String> {
        self.bus.ensure_browser_ready(RuntimeServiceKind::Messenger)?;

        let line = self.bus.sidecar_request(
            RuntimeServiceKind::Messenger,
            "open_messenger",
            serde_json::json!({}),
            MESSENGER_TIMEOUT,
        )?;

        if line.ok == Some(false) {
            {
                let mut snap = self.snapshot.lock();
                snap.state = MessengerState::MessengerError;
                snap.checked_at = Some(Utc::now().to_rfc3339());
            }
            return Err(line
                .error
                .unwrap_or_else(|| "Messenger navigation failed".into()));
        }

        if let Some(result) = line.result {
            self.apply_sidecar_result(result)?;
        }

        Ok(self.snapshot())
    }

    fn apply_sidecar_result(&self, result: serde_json::Value) -> Result<(), String> {
        let messenger = result
            .get("messenger")
            .cloned()
            .unwrap_or(result);
        let status = messenger
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("messenger_not_checked");
        let mut snap = self.snapshot.lock();
        snap.state = MessengerState::from_sidecar(status);
        snap.checked_at = messenger
            .get("checked_at")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(Utc::now().to_rfc3339()));
        snap.current_url = messenger
            .get("current_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        snap.reason_code = messenger
            .get("reason_code")
            .and_then(|v| v.as_str())
            .map(String::from);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_messenger_states() {
        assert_eq!(
            MessengerState::from_sidecar("messenger_ready"),
            MessengerState::MessengerReady
        );
        assert_eq!(
            MessengerState::from_sidecar("messenger_login_required"),
            MessengerState::MessengerLoginRequired
        );
        assert_eq!(
            MessengerState::from_sidecar("messenger_checkpoint"),
            MessengerState::MessengerCheckpoint
        );
        assert_eq!(
            MessengerState::from_sidecar("messenger_unavailable"),
            MessengerState::MessengerUnavailable
        );
        assert_eq!(
            MessengerState::from_sidecar("messenger_error"),
            MessengerState::MessengerError
        );
        assert_eq!(
            MessengerState::from_sidecar("unknown"),
            MessengerState::MessengerNotChecked
        );
    }
}
