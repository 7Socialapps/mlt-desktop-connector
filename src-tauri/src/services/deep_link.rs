use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::browser::{FacebookSessionState, MarketplaceStatus};
use crate::credentials::{self, CredentialStatus};
use crate::launch_session::{LaunchSessionService, LaunchStatus};
use crate::protocol::{parse_deep_link, DeepLinkRoute, ProtocolError};
use crate::runtime::FacebookRuntime;
use crate::services::{HeartbeatService, PairingCoordinator, PollingService};
use crate::state::AppState;

pub struct DeepLinkCoordinator {
    app: AppHandle,
    state: Arc<Mutex<AppState>>,
    pairing: Arc<PairingCoordinator>,
    facebook_runtime: Arc<FacebookRuntime>,
    launch_sessions: Arc<LaunchSessionService>,
    heartbeat: Arc<HeartbeatService>,
    pending: Mutex<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeepLinkUiState {
    pub last_route: Option<String>,
    pub message: Option<String>,
    pub launch_session_id: Option<String>,
    pub launch_status: Option<String>,
}

impl DeepLinkCoordinator {
    pub fn new(
        app: AppHandle,
        state: Arc<Mutex<AppState>>,
        pairing: Arc<PairingCoordinator>,
        facebook_runtime: Arc<FacebookRuntime>,
        launch_sessions: Arc<LaunchSessionService>,
        heartbeat: Arc<HeartbeatService>,
    ) -> Self {
        Self {
            app,
            state,
            pairing,
            facebook_runtime,
            launch_sessions,
            heartbeat,
            pending: Mutex::new(Vec::new()),
        }
    }

    pub fn snapshot(&self) -> DeepLinkUiState {
        let guard = self.state.lock();
        DeepLinkUiState {
            last_route: guard.deep_link_route.clone(),
            message: guard.deep_link_message.clone(),
            launch_session_id: guard.launch_session_id.clone(),
            launch_status: guard.launch_status.clone(),
        }
    }

    pub fn enqueue(&self, raw_url: String) {
        self.pending.lock().push(raw_url);
    }

    pub fn drain_pending(&self) {
        let urls: Vec<String> = self.pending.lock().drain(..).collect();
        for url in urls {
            let coordinator = self.clone_refs();
            tauri::async_runtime::spawn(async move {
                coordinator.handle_url(&url).await;
            });
        }
    }

    pub fn handle_argv(&self, argv: &[String]) {
        if let Some(url) = crate::protocol::extract_deep_link_from_argv(argv) {
            self.handle_url_sync(&url);
        }
    }

    fn handle_url_sync(&self, raw_url: &str) {
        let coordinator = self.clone_refs();
        let url = raw_url.to_string();
        tauri::async_runtime::spawn(async move {
            coordinator.handle_url(&url).await;
        });
    }

    pub async fn handle_url(&self, raw_url: &str) {
        focus_window(&self.app);

        match parse_deep_link(raw_url) {
            Ok(parsed) => {
                info!(route = ?parsed.route, "deep link accepted");
                self.set_route_label(&parsed.route);
                match parsed.route {
                    DeepLinkRoute::Open => self.handle_open().await,
                    DeepLinkRoute::ConnectFacebook => {
                        self.handle_connect_facebook(parsed.query.get("session").cloned())
                            .await;
                    }
                    DeepLinkRoute::OpenMarketplace => self.handle_open_marketplace().await,
                    DeepLinkRoute::OpenVehicleCreate => self.handle_open_vehicle_create().await,
                    DeepLinkRoute::Pair => {
                        self.handle_pair(parsed.query.get("session").cloned()).await;
                    }
                }
            }
            Err(err) => {
                warn!(error = %err, url = %raw_url, "deep link rejected");
                self.set_message(format!("Could not open link: {err}"));
                self.set_launch_status(LaunchStatus::Error);
            }
        }

        self.emit_ui();
        self.heartbeat.trigger_now();
    }

    async fn handle_open(&self) {
        self.set_message("MLT Desktop Connector is open.".into());
        self.set_launch_status(LaunchStatus::AppOpened);
    }

    async fn handle_pair(&self, _session: Option<String>) {
        let cred_status = credentials::credential_status();
        if cred_status == CredentialStatus::NeedsReconnect {
            self.set_message(
                "This device was revoked. Pair again from the MLT Dashboard.".into(),
            );
            self.set_launch_status(LaunchStatus::DeviceRevoked);
            return;
        }

        if credentials::is_paired() {
            self.set_message("This device is already paired.".into());
            return;
        }

        self.set_message("Starting pairing — enter the code in your MLT Dashboard.".into());
        self.set_launch_status(LaunchStatus::PairingRequired);

        let device_id = self.state.lock().device_id.to_string();
        if let Err(err) = self.pairing.start(device_id, None).await {
            self.set_message(format!("Pairing could not start: {err}"));
            self.set_launch_status(LaunchStatus::Error);
        }
    }

    async fn handle_connect_facebook(&self, session: Option<String>) {
        self.set_message("Facebook connection requested from MLT Dashboard".into());
        self.set_launch_status(LaunchStatus::AppOpened);

        if let Some(session_id) = session.clone() {
            self.state.lock().launch_session_id = Some(session_id.clone());
            let device_id = self.state.lock().device_id.to_string();
            match self
                .launch_sessions
                .redeem(&session_id, &device_id)
                .await
            {
                Ok(_) => self.set_launch_status(LaunchStatus::LaunchSessionRedeemed),
                Err(crate::launch_session::LaunchSessionError::NotPaired) => {
                    self.set_message(
                        "Pair this device first, then try Connect Facebook again.".into(),
                    );
                    self.set_launch_status(LaunchStatus::PairingRequired);
                    let device_id = self.state.lock().device_id.to_string();
                    let _ = self.pairing.start(device_id, None).await;
                    return;
                }
                Err(crate::launch_session::LaunchSessionError::AlreadyRedeemed) => {
                    self.set_message("This dashboard link was already used.".into());
                    self.set_launch_status(LaunchStatus::LaunchSessionRejected);
                    return;
                }
                Err(err) => {
                    warn!(error = %err, "launch session redemption failed");
                    self.set_message(format!("Could not verify dashboard link: {err}"));
                    self.set_launch_status(LaunchStatus::LaunchSessionRejected);
                }
            }
        }

        if credentials::credential_status() == CredentialStatus::NeedsReconnect {
            self.set_message(
                "This device was revoked. Pair again from the MLT Dashboard.".into(),
            );
            self.set_launch_status(LaunchStatus::DeviceRevoked);
            return;
        }

        if !credentials::is_paired() {
            self.set_message("Pair this device before connecting Facebook.".into());
            self.set_launch_status(LaunchStatus::PairingRequired);
            let device_id = self.state.lock().device_id.to_string();
            let _ = self.pairing.start(device_id, None).await;
            return;
        }

        if let Err(err) = self.facebook_runtime.launch_browser() {
            warn!(error = %err, "browser launch failed during connect-facebook");
            self.set_message(format!("Browser could not start: {err}"));
            self.set_launch_status(LaunchStatus::Error);
            return;
        }
        self.set_launch_status(LaunchStatus::BrowserReady);

        let session_snap = self.facebook_runtime.session.snapshot();
        match session_snap.state {
            FacebookSessionState::FacebookLoggedIn => {
                self.set_launch_status(LaunchStatus::FacebookLoggedIn);
                if let Err(err) = self.facebook_runtime.marketplace.open_marketplace() {
                    warn!(error = %err, "marketplace readiness check failed");
                    self.set_message(format!(
                        "Signed in to Facebook. Marketplace check: {err}"
                    ));
                } else {
                    let mp = self.facebook_runtime.marketplace.snapshot();
                    if mp.status == MarketplaceStatus::MarketplaceReady {
                        self.set_launch_status(LaunchStatus::MarketplaceReady);
                        self.set_message(
                            "Facebook and Marketplace are ready. You can post from the dashboard."
                                .into(),
                        );
                    } else {
                        self.set_message(
                            "Signed in to Facebook. Finish any Marketplace prompts in the browser."
                                .into(),
                        );
                    }
                }
            }
            FacebookSessionState::FacebookLoggedOut
            | FacebookSessionState::FacebookSessionExpired
            | FacebookSessionState::FacebookNotChecked => {
                self.set_launch_status(LaunchStatus::FacebookLoginRequired);
                if let Err(err) = self.facebook_runtime.open_facebook_login() {
                    self.set_message(format!(
                        "Sign into Facebook in the browser window. ({err})"
                    ));
                } else {
                    self.set_message(
                        "Sign into Facebook in the browser window. Your password is never stored."
                            .into(),
                    );
                }
            }
            _ => {
                self.set_launch_status(LaunchStatus::FacebookLoginRequired);
                self.set_message(
                    session_snap
                        .state
                        .label()
                        .to_string(),
                );
                let _ = self.facebook_runtime.open_facebook_login();
            }
        }
    }

    async fn handle_open_marketplace(&self) {
        self.set_message("Opening Facebook Marketplace…".into());
        if let Err(err) = self.facebook_runtime.launch_browser() {
            self.set_message(format!("Browser could not start: {err}"));
            return;
        }
        if let Err(err) = self.facebook_runtime.marketplace.open_marketplace() {
            self.set_message(format!("Could not open Marketplace: {err}"));
        }
    }

    async fn handle_open_vehicle_create(&self) {
        self.set_message("Opening vehicle listing form…".into());
        if let Err(err) = self.facebook_runtime.launch_browser() {
            self.set_message(format!("Browser could not start: {err}"));
            return;
        }
        if let Err(err) = self
            .facebook_runtime
            .marketplace
            .open_vehicle_create_route()
        {
            self.set_message(format!("Could not open vehicle form: {err}"));
        }
    }

    fn set_route_label(&self, route: &DeepLinkRoute) {
        let label = match route {
            DeepLinkRoute::Open => "open",
            DeepLinkRoute::ConnectFacebook => "connect-facebook",
            DeepLinkRoute::OpenMarketplace => "open-marketplace",
            DeepLinkRoute::OpenVehicleCreate => "open-vehicle-create",
            DeepLinkRoute::Pair => "pair",
        };
        self.state.lock().deep_link_route = Some(label.into());
    }

    fn set_message(&self, message: String) {
        self.state.lock().deep_link_message = Some(message);
    }

    fn set_launch_status(&self, status: LaunchStatus) {
        self.state.lock().launch_status = Some(status.as_str().into());
    }

    fn emit_ui(&self) {
        let _ = self.app.emit("connector://deep-link-changed", self.snapshot());
        let _ = self
            .app
            .emit("connector://status-changed", self.state.lock().status_snapshot());
    }

    fn clone_refs(&self) -> DeepLinkHandle {
        DeepLinkHandle {
            app: self.app.clone(),
            state: self.state.clone(),
            pairing: self.pairing.clone(),
            facebook_runtime: self.facebook_runtime.clone(),
            launch_sessions: self.launch_sessions.clone(),
            heartbeat: self.heartbeat.clone(),
        }
    }
}

struct DeepLinkHandle {
    app: AppHandle,
    state: Arc<Mutex<AppState>>,
    pairing: Arc<PairingCoordinator>,
    facebook_runtime: Arc<FacebookRuntime>,
    launch_sessions: Arc<LaunchSessionService>,
    heartbeat: Arc<HeartbeatService>,
}

impl DeepLinkHandle {
    async fn handle_url(&self, raw_url: &str) {
        let coordinator = DeepLinkCoordinator {
            app: self.app.clone(),
            state: self.state.clone(),
            pairing: self.pairing.clone(),
            facebook_runtime: self.facebook_runtime.clone(),
            launch_sessions: self.launch_sessions.clone(),
            heartbeat: self.heartbeat.clone(),
            pending: Mutex::new(Vec::new()),
        };
        coordinator.handle_url(raw_url).await;
    }
}

fn focus_window(app: &AppHandle) {
    crate::lifecycle::focus_main_window(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_error_messages_are_safe() {
        let err = ProtocolError::UnknownRoute("bad".into());
        assert!(err.to_string().contains("unknown route"));
    }
}
