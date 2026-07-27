use serde::{Deserialize, Serialize};

pub const CREDENTIAL_FILE_VERSION: u32 = 1;
pub const CREDENTIALS_FILENAME: &str = "credentials.enc";
pub const CREDENTIALS_BACKUP_FILENAME: &str = "credentials.enc.bak";
pub const CREDENTIALS_TEMP_FILENAME: &str = "credentials.enc.tmp";
pub const CREDENTIAL_KEY_FILENAME: &str = "credentials.key";

/// Payload persisted to disk (encrypted). Never includes access tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedDeviceCredential {
    pub version: u32,
    pub refresh_token: String,
    pub user_id: String,
    pub dealership_id: String,
    pub updated_at: String,
    /// Reserved for asymmetric refresh signing (public key registered with backend).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_public_key: Option<String>,
}

impl PersistedDeviceCredential {
    pub fn new(refresh_token: String, user_id: String, dealership_id: String) -> Self {
        Self {
            version: CREDENTIAL_FILE_VERSION,
            refresh_token,
            user_id,
            dealership_id,
            updated_at: chrono::Utc::now().to_rfc3339(),
            device_public_key: None,
        }
    }
}

/// Active connector session — access token kept in memory only.
#[derive(Debug, Clone)]
pub struct StoredCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub dealership_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStatus {
    Unpaired,
    Paired,
    NeedsReconnect,
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("credential store not initialized")]
    NotInitialized,
    #[error("failed to read credential file")]
    ReadFailed(#[source] std::io::Error),
    #[error("failed to write credential file")]
    WriteFailed(#[source] std::io::Error),
    #[error("credential file is corrupt or unreadable")]
    Corrupt,
    #[error("encryption error")]
    Crypto(String),
    #[error("session unavailable — reconnect device")]
    SessionUnavailable,
}
