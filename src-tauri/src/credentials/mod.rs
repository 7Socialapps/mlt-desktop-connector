mod atomic;
mod crypto;
mod permissions;
mod store;
mod types;

pub use store::{CredentialStore, DeviceCredentialBackend};
pub use types::{
    CredentialError, CredentialStatus, PersistedDeviceCredential, StoredCredentials,
};

use std::sync::Arc;

use anyhow::Result;
use tauri::{AppHandle, Manager};
use tracing::{info, warn};

use crate::api::ConnectorApiClient;

const RECONNECT_MESSAGE: &str =
    "Reconnect device — stored credentials are unavailable. Start pairing again.";

pub fn init(app: &AppHandle) -> Result<Arc<CredentialStore>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("failed to resolve app data directory: {e}"))?;
    let store = Arc::new(CredentialStore::new(dir));
    CredentialStore::init_global(store.clone())?;
    Ok(store)
}

pub fn store() -> Result<Arc<CredentialStore>> {
    CredentialStore::global()
}

pub fn store_credentials(creds: &StoredCredentials) -> Result<()> {
    let s = store()?;
    s.store_session(
        creds.access_token.clone(),
        creds.refresh_token.clone(),
        creds.user_id.clone(),
        creds.dealership_id.clone(),
    )
}

pub fn rotate_credentials(access_token: String, refresh_token: String) -> Result<()> {
    store()?.rotate_session(access_token, refresh_token)
}

pub fn load_credentials() -> Result<Option<StoredCredentials>> {
    store()?.load_credentials()
}

pub fn clear_credentials() -> Result<()> {
    store()?.clear_session()
}

pub fn has_access_token() -> bool {
    store()
        .ok()
        .map(|s| s.has_access_token())
        .unwrap_or(false)
}

pub fn is_paired() -> bool {
    store()
        .ok()
        .map(|s| s.has_persisted_refresh())
        .unwrap_or(false)
}

pub fn credential_status() -> CredentialStatus {
    store()
        .ok()
        .map(|s| s.status())
        .unwrap_or(CredentialStatus::Unpaired)
}

pub fn needs_reconnect_message() -> Option<String> {
    store().ok().and_then(|s| s.needs_reconnect_message())
}

pub fn bootstrap_from_disk() -> CredentialStatus {
    match store() {
        Ok(s) => s.bootstrap_from_disk(),
        Err(_) => CredentialStatus::Unpaired,
    }
}

pub fn handle_revoked_device() -> Result<()> {
    store()?.handle_revoked_device()
}

pub fn mark_needs_reconnect(message: &str) {
    if let Ok(s) = store() {
        s.mark_needs_reconnect(message);
    }
}

/// Restore in-memory access token from persisted refresh token after restart.
pub async fn ensure_access_token(client: &ConnectorApiClient) -> Result<bool> {
    let s = store()?;

    if s.has_access_token() {
        return Ok(true);
    }

    let Some(refresh_token) = s.refresh_token() else {
        return Ok(false);
    };

    match client.authenticate_device(&refresh_token).await {
        Ok(resp) if resp.ok => {
            s.rotate_session(resp.access_token, resp.refresh_token)?;
            info!("restored session from persisted refresh token");
            Ok(true)
        }
        Ok(resp) => {
            warn!(error = ?resp.error, "refresh token rejected — reconnect required");
            let _ = s.clear_session();
            s.mark_needs_reconnect(RECONNECT_MESSAGE);
            Ok(false)
        }
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("DEVICE_REVOKED") || msg.contains("revoked") {
                let _ = s.handle_revoked_device();
                s.mark_needs_reconnect("Device revoked — start pairing again from the dashboard.");
            } else {
                warn!(error = %err, "token refresh failed");
            }
            Ok(false)
        }
    }
}
