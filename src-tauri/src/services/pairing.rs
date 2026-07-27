use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use crate::api::types::{ConnectorOs, PollPairingSessionRequest};
use crate::api::ConnectorApiClient;
use crate::credentials::{has_access_token, store_credentials, StoredCredentials};
use crate::state::{AppState, ConnectionState};
use crate::services::PollingService;
use crate::version::{CONNECTOR_VERSION, DEFAULT_CAPABILITIES};

const PAIRING_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingUiState {
    pub active: bool,
    pub pairing_code: Option<String>,
    pub expires_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

pub struct PairingCoordinator {
    app: AppHandle,
    state: Arc<Mutex<AppState>>,
    client: Arc<ConnectorApiClient>,
    polling: Arc<PollingService>,
    ui: Arc<Mutex<PairingUiState>>,
}

impl PairingCoordinator {
    pub fn new(
        app: AppHandle,
        state: Arc<Mutex<AppState>>,
        client: Arc<ConnectorApiClient>,
        polling: Arc<PollingService>,
    ) -> Self {
        Self {
            app,
            state,
            client,
            polling,
            ui: Arc::new(Mutex::new(PairingUiState {
                active: false,
                pairing_code: None,
                expires_at: None,
                status: "idle".into(),
                error: None,
            })),
        }
    }

    pub fn snapshot(&self) -> PairingUiState {
        self.ui.lock().clone()
    }

    pub async fn start(&self, device_id: String, device_name: Option<String>) -> Result<PairingUiState, String> {
        info!(device_id = %device_id, "create_pairing_session: starting");

        let request = crate::api::types::CreatePairingSessionRequest {
            action: "create_pairing_session".into(),
            device_id: device_id.clone(),
            connector_version: CONNECTOR_VERSION.to_string(),
            os: ConnectorOs::detect(),
            capabilities: DEFAULT_CAPABILITIES.iter().map(|s| s.to_string()).collect(),
            device_name,
        };

        let created = match self.client.create_pairing_session(request).await {
            Ok(response) => {
                info!(
                    ok = response.ok,
                    has_session = response.session_id.is_some(),
                    has_code = response.pairing_code.is_some(),
                    error = ?response.error,
                    "create_pairing_session: response received"
                );
                response
            }
            Err(err) => {
                let msg = err.to_string();
                warn!(error = %msg, "create_pairing_session: request failed");
                {
                    let mut ui = self.ui.lock();
                    ui.error = Some(msg.clone());
                    ui.status = "error".into();
                }
                let _ = self.app.emit("connector://pairing-changed", self.snapshot());
                return Err(msg);
            }
        };

        if !created.ok {
            let msg = created
                .error
                .unwrap_or_else(|| "Pairing session failed".into());
            warn!(error = %msg, error_code = ?created.error_code, "create_pairing_session: backend rejected");
            {
                let mut ui = self.ui.lock();
                ui.error = Some(msg.clone());
                ui.status = "error".into();
            }
            let _ = self.app.emit("connector://pairing-changed", self.snapshot());
            return Err(msg);
        }

        let session_id = created
            .session_id
            .ok_or_else(|| "Missing sessionId".to_string())?;
        let session_secret = created
            .session_secret
            .ok_or_else(|| "Missing sessionSecret".to_string())?;
        let pairing_code = created.pairing_code.clone();
        info!(
            pairing_code = ?pairing_code,
            expires_at = ?created.expires_at,
            "create_pairing_session: session created"
        );
        {
            let mut ui = self.ui.lock();
            ui.active = true;
            ui.pairing_code = pairing_code.clone();
            ui.expires_at = created.expires_at.clone();
            ui.status = "pairing_pending".into();
            ui.error = None;
        }
        let _ = self.app.emit("connector://pairing-changed", self.snapshot());

        let poll_this = self.clone_inner();
        tauri::async_runtime::spawn(async move {
            poll_this
                .poll_until_complete(session_id, session_secret)
                .await;
        });

        Ok(self.snapshot())
    }

    async fn poll_until_complete(&self, session_id: String, session_secret: String) {
        loop {
            tokio::time::sleep(PAIRING_POLL_INTERVAL).await;

            let request = PollPairingSessionRequest {
                action: "poll_pairing_session".into(),
                session_id: session_id.clone(),
                session_secret: session_secret.clone(),
            };

            match self.client.poll_pairing_session(request).await {
                Ok(resp) if resp.ok => {
                    {
                        let mut ui = self.ui.lock();
                        ui.status = resp.status.clone();
                        ui.error = None;
                    }
                    let _ = self.app.emit("connector://pairing-changed", self.snapshot());

                    if resp.status == "pairing_completed" {
                        let stored_tokens = if let (
                            Some(access),
                            Some(refresh),
                            Some(user_id),
                            Some(dealership_id),
                        ) = (
                            resp.access_token,
                            resp.refresh_token,
                            resp.user_id,
                            resp.dealership_id,
                        ) {
                            match store_credentials(&StoredCredentials {
                                access_token: access,
                                refresh_token: refresh,
                                user_id,
                                dealership_id,
                            }) {
                                Ok(()) => {
                                    info!("pairing completed — credentials stored");
                                    true
                                }
                                Err(e) => {
                                    warn!(error = %e, "failed to store paired credentials");
                                    false
                                }
                            }
                        } else if has_access_token() {
                            info!("pairing completed — credentials already stored");
                            true
                        } else {
                            warn!(
                                status = %resp.status,
                                "pairing_completed without tokens — waiting for next poll"
                            );
                            false
                        };

                        if stored_tokens {
                            self.state.lock().paired = true;
                            self.state.lock().needs_reconnect = false;
                            self.state.lock().connection_state = ConnectionState::Idle;
                            self.polling.set_enabled(true);
                            {
                                let mut ui = self.ui.lock();
                                ui.active = false;
                                ui.status = "pairing_completed".into();
                                ui.error = None;
                            }
                            let _ = self.app.emit("connector://status-changed", ());
                            let _ = self.app.emit("connector://pairing-changed", self.snapshot());
                            break;
                        }
                        continue;
                    }

                    if resp.status == "pairing_expired" {
                        let mut ui = self.ui.lock();
                        ui.active = false;
                        ui.error = Some("Pairing session expired".into());
                        let _ = self.app.emit("connector://pairing-changed", self.snapshot());
                        break;
                    }
                }
                Ok(resp) => {
                    let msg = resp
                        .error
                        .unwrap_or_else(|| format!("Pairing poll failed (status={})", resp.status));
                    warn!(error = %msg, "poll_pairing_session returned ok=false");
                    let mut ui = self.ui.lock();
                    ui.error = Some(msg);
                    let _ = self.app.emit("connector://pairing-changed", self.snapshot());
                }
                Err(err) => {
                    warn!(error = %err, "poll_pairing_session request failed — retrying");
                    let mut ui = self.ui.lock();
                    ui.error = Some(err.to_string());
                    let _ = self.app.emit("connector://pairing-changed", self.snapshot());
                }
            }
        }
    }

    fn clone_inner(&self) -> Self {
        Self {
            app: self.app.clone(),
            state: self.state.clone(),
            client: self.client.clone(),
            polling: self.polling.clone(),
            ui: self.ui.clone(),
        }
    }
}
