use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Starting,
    Idle,
    Connected,
    Reconnecting,
    Offline,
    ShuttingDown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StatusSnapshot {
    pub device_id: String,
    pub connector_version: String,
    pub environment: String,
    pub paired: bool,
    pub needs_reconnect: bool,
    pub connection_state: ConnectionState,
    pub last_heartbeat_at: Option<String>,
    pub last_error: Option<String>,
}

pub struct AppState {
    pub device_id: Uuid,
    pub environment: String,
    pub paired: bool,
    pub needs_reconnect: bool,
    pub connection_state: ConnectionState,
    pub last_heartbeat_at: Option<String>,
    pub last_error: Option<String>,
}

impl AppState {
    pub fn status_snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            device_id: self.device_id.to_string(),
            connector_version: crate::version::CONNECTOR_VERSION.to_string(),
            environment: self.environment.clone(),
            paired: self.paired,
            needs_reconnect: self.needs_reconnect,
            connection_state: self.connection_state,
            last_heartbeat_at: self.last_heartbeat_at.clone(),
            last_error: self.last_error.clone(),
        }
    }
}
