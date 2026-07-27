mod store;

pub use store::LaunchSessionStore;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use crate::api::types::{RedeemLaunchSessionRequest, RedeemLaunchSessionResponse};
use crate::api::ConnectorApiClient;
use crate::credentials::{ensure_access_token, is_paired, load_credentials};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStatus {
    AppOpened,
    LaunchSessionRedeemed,
    LaunchSessionRejected,
    BrowserReady,
    FacebookLoggedIn,
    FacebookLoginRequired,
    MarketplaceReady,
    PairingRequired,
    DeviceRevoked,
    Error,
}

impl LaunchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AppOpened => "app_opened",
            Self::LaunchSessionRedeemed => "launch_session_redeemed",
            Self::LaunchSessionRejected => "launch_session_rejected",
            Self::BrowserReady => "browser_ready",
            Self::FacebookLoggedIn => "facebook_logged_in",
            Self::FacebookLoginRequired => "facebook_login_required",
            Self::MarketplaceReady => "marketplace_ready",
            Self::PairingRequired => "pairing_required",
            Self::DeviceRevoked => "device_revoked",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Error)]
pub enum LaunchSessionError {
    #[error("device is not paired")]
    NotPaired,
    #[error("launch session already redeemed")]
    AlreadyRedeemed,
    #[error("invalid launch session id")]
    InvalidSessionId,
    #[error("backend error: {0}")]
    Backend(String),
    #[error("{0}")]
    Other(String),
}

pub struct LaunchSessionService {
    store: LaunchSessionStore,
    client: std::sync::Arc<ConnectorApiClient>,
}

impl LaunchSessionService {
    pub fn new(client: std::sync::Arc<ConnectorApiClient>, store: LaunchSessionStore) -> Self {
        Self { store, client }
    }

    pub fn is_redeemed(&self, session_id: &str) -> bool {
        self.store.is_redeemed(session_id)
    }

    pub async fn redeem(
        &self,
        session_id: &str,
        device_id: &str,
    ) -> Result<RedeemLaunchSessionResponse, LaunchSessionError> {
        if session_id.trim().is_empty() || session_id.len() > 128 {
            return Err(LaunchSessionError::InvalidSessionId);
        }

        if self.store.is_redeemed(session_id) {
            return Err(LaunchSessionError::AlreadyRedeemed);
        }

        if !is_paired() {
            return Err(LaunchSessionError::NotPaired);
        }

        let _ = ensure_access_token(&self.client)
            .await
            .map_err(|e| LaunchSessionError::Other(e.to_string()))?;

        let creds = load_credentials()
            .map_err(|e| LaunchSessionError::Other(e.to_string()))?
            .ok_or(LaunchSessionError::NotPaired)?;

        if creds.access_token.is_empty() {
            return Err(LaunchSessionError::NotPaired);
        }

        let request = RedeemLaunchSessionRequest {
            action: "redeem_launch_session".into(),
            session_id: session_id.to_string(),
            device_id: device_id.to_string(),
        };

        match self
            .client
            .redeem_launch_session(request, &creds.access_token)
            .await
        {
            Ok(response) if response.ok => {
                if let Some(nonce) = response.nonce.as_deref().or(Some(session_id.as_ref())) {
                    if let Err(err) = self.store.mark_redeemed(nonce) {
                        warn!(error = %err, "failed to persist redeemed launch session nonce");
                    }
                }
                Ok(response)
            }
            Ok(response) => {
                let message = response
                    .error
                    .unwrap_or_else(|| "Launch session redemption failed".into());
                if response.error_code.as_deref() == Some("LAUNCH_SESSION_ALREADY_REDEEMED") {
                    let _ = self.store.mark_redeemed(session_id);
                    return Err(LaunchSessionError::AlreadyRedeemed);
                }
                Err(LaunchSessionError::Backend(message))
            }
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("404") || msg.contains("not found") || msg.contains("Unknown action") {
                    warn!(
                        session_id = %session_id,
                        "redeem_launch_session backend action unavailable — see docs/LAUNCH-SESSION-CONTRACT.md"
                    );
                }
                Err(LaunchSessionError::Backend(msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn launch_status_serializes_snake_case() {
        assert_eq!(LaunchStatus::LaunchSessionRedeemed.as_str(), "launch_session_redeemed");
    }

    #[test]
    fn store_prevents_local_replay() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("redeemed.json");
        let store = LaunchSessionStore::at_path(path);
        assert!(!store.is_redeemed("session-1"));
        store.mark_redeemed("session-1").expect("mark");
        assert!(store.is_redeemed("session-1"));
    }
}
