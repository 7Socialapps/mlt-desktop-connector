use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use parking_lot::Mutex;
use tracing::{info, warn};

use super::atomic::{atomic_write_with_backup, delete_credential_files};
use super::crypto::{decrypt, encrypt, generate_encryption_key};
use super::types::{
    CredentialError, CredentialStatus, PersistedDeviceCredential, StoredCredentials,
    CREDENTIAL_KEY_FILENAME, CREDENTIALS_BACKUP_FILENAME, CREDENTIALS_FILENAME,
    CREDENTIALS_TEMP_FILENAME,
};

/// Storage backend for revocable device credentials.
/// Future: add asymmetric signing keypair methods on this trait.
pub trait DeviceCredentialBackend: Send + Sync {
    fn save_persisted(&self, credential: &PersistedDeviceCredential) -> Result<()>;
    fn load_persisted(&self) -> Result<Option<PersistedDeviceCredential>>;
    fn delete_persisted(&self) -> Result<()>;
}

#[derive(Debug)]
enum SessionState {
    Unpaired,
    Paired {
        access_token: Option<String>,
        persisted: PersistedDeviceCredential,
    },
    NeedsReconnect {
        message: String,
    },
}

pub struct CredentialStore {
    data_dir: PathBuf,
    session: Mutex<SessionState>,
}

static GLOBAL_STORE: OnceLock<Arc<CredentialStore>> = OnceLock::new();

impl CredentialStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            session: Mutex::new(SessionState::Unpaired),
        }
    }

    pub fn init_global(store: Arc<CredentialStore>) -> Result<()> {
        GLOBAL_STORE
            .set(store)
            .map_err(|_| anyhow::anyhow!("credential store already initialized"))
    }

    pub fn global() -> Result<Arc<CredentialStore>> {
        GLOBAL_STORE
            .get()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(CredentialError::NotInitialized))
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn credentials_path(&self) -> PathBuf {
        self.data_dir.join(CREDENTIALS_FILENAME)
    }

    fn credentials_backup_path(&self) -> PathBuf {
        self.data_dir.join(CREDENTIALS_BACKUP_FILENAME)
    }

    fn credentials_temp_path(&self) -> PathBuf {
        self.data_dir.join(CREDENTIALS_TEMP_FILENAME)
    }

    fn credentials_restore_temp_path(&self) -> PathBuf {
        self.data_dir.join("credentials.enc.restore.tmp")
    }

    fn key_path(&self) -> PathBuf {
        self.data_dir.join(CREDENTIAL_KEY_FILENAME)
    }

    fn ensure_data_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("failed to create {}", self.data_dir.display()))
    }

    fn load_or_create_key(&self) -> Result<[u8; 32]> {
        self.ensure_data_dir()?;
        let path = self.key_path();
        if path.exists() {
            let bytes = std::fs::read(&path).with_context(|| {
                format!("failed to read encryption key at {}", path.display())
            })?;
            if bytes.len() != 32 {
                anyhow::bail!("encryption key file has invalid length");
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }

        let key = generate_encryption_key();
        let temp = self.data_dir.join("credentials.key.tmp");
        super::atomic::atomic_replace(&path, &temp, &key)
            .with_context(|| format!("failed to write encryption key to {}", path.display()))?;
        Ok(key)
    }

    fn try_load_encrypted_file(&self, path: &Path) -> Result<Option<PersistedDeviceCredential>> {
        if !path.exists() {
            return Ok(None);
        }
        let encrypted = std::fs::read(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if encrypted.is_empty() {
            anyhow::bail!("credential file is empty");
        }
        self.decrypt_credential_blob(&encrypted).map(Some)
    }
    fn decrypt_credential_blob(&self, encrypted: &[u8]) -> Result<PersistedDeviceCredential> {
        let key = self.load_or_create_key()?;
        let plaintext = decrypt(&key, encrypted).context("credential decrypt failed")?;
        let credential: PersistedDeviceCredential =
            serde_json::from_slice(&plaintext).context("credential JSON invalid")?;
        Ok(credential)
    }

    fn restore_from_backup(&self) -> Result<PersistedDeviceCredential> {
        let backup_path = self.credentials_backup_path();
        let credential = self
            .try_load_encrypted_file(&backup_path)?
            .ok_or_else(|| anyhow::anyhow!("backup file missing"))?;
        // Re-persist through atomic save (rotates current primary → backup).
        self.save_persisted(&credential)?;
        info!("restored device credentials from backup file");
        Ok(credential)
    }

    pub fn bootstrap_from_disk(&self) -> CredentialStatus {
        match self.load_persisted() {
            Ok(Some(persisted)) => {
                {
                    let mut session = self.session.lock();
                    *session = SessionState::Paired {
                        access_token: None,
                        persisted,
                    };
                }
                info!("loaded persisted device credentials from encrypted store");
                CredentialStatus::Paired
            }
            Ok(None) => {
                *self.session.lock() = SessionState::Unpaired;
                CredentialStatus::Unpaired
            }
            Err(err) => {
                warn!(error = %err, "credential file unreadable — reconnect required");
                self.mark_needs_reconnect("Reconnect device — stored credentials are unreadable.");
                CredentialStatus::NeedsReconnect
            }
        }
    }

    pub fn mark_needs_reconnect(&self, message: &str) {
        *self.session.lock() = SessionState::NeedsReconnect {
            message: message.to_string(),
        };
    }

    pub fn status(&self) -> CredentialStatus {
        match &*self.session.lock() {
            SessionState::Unpaired => CredentialStatus::Unpaired,
            SessionState::Paired { .. } => CredentialStatus::Paired,
            SessionState::NeedsReconnect { .. } => CredentialStatus::NeedsReconnect,
        }
    }

    pub fn needs_reconnect_message(&self) -> Option<String> {
        match &*self.session.lock() {
            SessionState::NeedsReconnect { message } => Some(message.clone()),
            _ => None,
        }
    }

    pub fn is_paired(&self) -> bool {
        matches!(self.status(), CredentialStatus::Paired)
    }

    pub fn has_persisted_refresh(&self) -> bool {
        matches!(&*self.session.lock(), SessionState::Paired { .. })
    }

    pub fn has_access_token(&self) -> bool {
        matches!(
            &*self.session.lock(),
            SessionState::Paired {
                access_token: Some(_),
                ..
            }
        )
    }

    /// Store a new session after pairing. Access token stays in memory; refresh token encrypted on disk.
    pub fn store_session(
        &self,
        access_token: String,
        refresh_token: String,
        user_id: String,
        dealership_id: String,
    ) -> Result<()> {
        let persisted =
            PersistedDeviceCredential::new(refresh_token.clone(), user_id.clone(), dealership_id.clone());
        self.save_persisted(&persisted)?;
        *self.session.lock() = SessionState::Paired {
            access_token: Some(access_token),
            persisted,
        };
        info!("stored device credentials in encrypted app-data file");
        Ok(())
    }

    /// Rotate credentials after refresh — updates in-memory access token and persisted refresh token.
    pub fn rotate_session(&self, access_token: String, refresh_token: String) -> Result<()> {
        let session = self.session.lock();
        let (user_id, dealership_id) = match &*session {
            SessionState::Paired { persisted, .. } => {
                (persisted.user_id.clone(), persisted.dealership_id.clone())
            }
            _ => anyhow::bail!("cannot rotate credentials without an active session"),
        };
        drop(session);

        let persisted =
            PersistedDeviceCredential::new(refresh_token.clone(), user_id, dealership_id);
        self.save_persisted(&persisted)?;
        *self.session.lock() = SessionState::Paired {
            access_token: Some(access_token),
            persisted,
        };
        info!("rotated device credentials");
        Ok(())
    }

    pub fn set_access_token(&self, access_token: String) -> Result<()> {
        let mut session = self.session.lock();
        match &mut *session {
            SessionState::Paired {
                access_token: slot,
                ..
            } => {
                *slot = Some(access_token);
                Ok(())
            }
            _ => anyhow::bail!("cannot set access token without persisted session"),
        }
    }

    pub fn load_credentials(&self) -> Result<Option<StoredCredentials>> {
        let session = self.session.lock();
        match &*session {
            SessionState::Paired {
                access_token: Some(access_token),
                persisted,
            } => Ok(Some(StoredCredentials {
                access_token: access_token.clone(),
                refresh_token: persisted.refresh_token.clone(),
                user_id: persisted.user_id.clone(),
                dealership_id: persisted.dealership_id.clone(),
            })),
            SessionState::Paired {
                access_token: None,
                persisted,
            } => Ok(Some(StoredCredentials {
                access_token: String::new(),
                refresh_token: persisted.refresh_token.clone(),
                user_id: persisted.user_id.clone(),
                dealership_id: persisted.dealership_id.clone(),
            })),
            SessionState::Unpaired => Ok(None),
            SessionState::NeedsReconnect { .. } => {
                Err(anyhow::anyhow!(CredentialError::SessionUnavailable))
            }
        }
    }

    pub fn refresh_token(&self) -> Option<String> {
        match &*self.session.lock() {
            SessionState::Paired { persisted, .. } => Some(persisted.refresh_token.clone()),
            _ => None,
        }
    }

    pub fn clear_session(&self) -> Result<()> {
        self.delete_persisted()?;
        *self.session.lock() = SessionState::Unpaired;
        info!("cleared device credentials from encrypted app-data store");
        Ok(())
    }

    pub fn handle_revoked_device(&self) -> Result<()> {
        self.clear_session()?;
        Ok(())
    }
}

impl DeviceCredentialBackend for CredentialStore {
    fn save_persisted(&self, credential: &PersistedDeviceCredential) -> Result<()> {
        self.ensure_data_dir()?;
        let key = self.load_or_create_key()?;
        let plaintext = serde_json::to_vec(credential).context("failed to serialize credentials")?;
        let encrypted = encrypt(&key, &plaintext).context("failed to encrypt credentials")?;
        atomic_write_with_backup(
            &self.credentials_path(),
            &self.credentials_backup_path(),
            &self.credentials_temp_path(),
            &encrypted,
        )?;
        Ok(())
    }

    fn load_persisted(&self) -> Result<Option<PersistedDeviceCredential>> {
        let primary_path = self.credentials_path();
        let backup_path = self.credentials_backup_path();

        if let Ok(Some(credential)) = self.try_load_encrypted_file(&primary_path) {
            return Ok(Some(credential));
        }

        if let Err(err) = self.try_load_encrypted_file(&primary_path) {
            if primary_path.exists() {
                warn!(
                    path = %primary_path.display(),
                    error = %err,
                    "primary credential file unreadable — trying backup"
                );
            }
        }

        if backup_path.exists() {
            return self.restore_from_backup().map(Some);
        }

        if primary_path.exists() {
            anyhow::bail!("credential file corrupt and no backup available");
        }

        Ok(None)
    }

    fn delete_persisted(&self) -> Result<()> {
        delete_credential_files(
            &self.credentials_path(),
            &self.credentials_backup_path(),
            &self.credentials_temp_path(),
            &self.credentials_restore_temp_path(),
        )?;
        if self.key_path().exists() {
            std::fs::remove_file(self.key_path())
                .with_context(|| format!("failed to delete {}", self.key_path().display()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (TempDir, Arc<CredentialStore>) {
        let temp = TempDir::new().expect("tempdir");
        let store = Arc::new(CredentialStore::new(temp.path().to_path_buf()));
        (temp, store)
    }

    fn sample_persisted() -> PersistedDeviceCredential {
        PersistedDeviceCredential::new(
            "refresh-token-abc".into(),
            "user-1".into(),
            "dealer-1".into(),
        )
    }

    #[test]
    fn save_and_load_persisted_credentials() {
        let (_temp, store) = test_store();
        let sample = sample_persisted();
        store.save_persisted(&sample).expect("save");
        let loaded = store.load_persisted().expect("load").expect("some");
        assert_eq!(loaded.refresh_token, sample.refresh_token);
        assert_eq!(loaded.user_id, sample.user_id);
        assert_eq!(loaded.dealership_id, sample.dealership_id);
    }

    #[test]
    fn load_after_restart_simulation() {
        let (temp, store) = test_store();
        store
            .store_session(
                "access-live".into(),
                "refresh-persist".into(),
                "user-1".into(),
                "dealer-1".into(),
            )
            .expect("store session");

        let store2 = Arc::new(CredentialStore::new(temp.path().to_path_buf()));
        assert_eq!(store2.bootstrap_from_disk(), CredentialStatus::Paired);
        assert!(!store2.has_access_token());
        let creds = store2.load_credentials().expect("load").expect("creds");
        assert!(creds.access_token.is_empty());
        assert_eq!(creds.refresh_token, "refresh-persist");
    }

    #[test]
    fn rotate_updates_persisted_refresh_token() {
        let (_temp, store) = test_store();
        store
            .store_session(
                "access-1".into(),
                "refresh-1".into(),
                "user-1".into(),
                "dealer-1".into(),
            )
            .expect("store");
        store
            .rotate_session("access-2".into(), "refresh-2".into())
            .expect("rotate");

        assert!(store.has_access_token());
        let persisted = store.load_persisted().expect("load").expect("some");
        assert_eq!(persisted.refresh_token, "refresh-2");
        let creds = store.load_credentials().expect("load").expect("creds");
        assert_eq!(creds.access_token, "access-2");
    }

    #[test]
    fn missing_primary_restores_last_good_backup_on_bootstrap() {
        let (temp, store) = test_store();
        store.save_persisted(&sample_persisted()).expect("save v1");
        store
            .save_persisted(&PersistedDeviceCredential::new(
                "refresh-v2".into(),
                "user-1".into(),
                "dealer-1".into(),
            ))
            .expect("save v2");

        // Simulate crash after primary→backup rotation but before temp→primary rename:
        // backup holds the last fully-written generation (v2).
        let last_good = std::fs::read(store.credentials_path()).expect("read v2 primary");
        std::fs::write(store.credentials_backup_path(), last_good).expect("seed backup");
        std::fs::remove_file(store.credentials_path()).expect("drop primary");

        let store2 = Arc::new(CredentialStore::new(temp.path().to_path_buf()));
        assert_eq!(store2.bootstrap_from_disk(), CredentialStatus::Paired);
        let loaded = store2.load_persisted().expect("load").expect("some");
        assert_eq!(loaded.refresh_token, "refresh-v2");
    }

    #[test]
    fn corrupt_primary_bytes_falls_back_to_backup_generation() {
        let (temp, store) = test_store();
        store.save_persisted(&sample_persisted()).expect("save v1");
        store
            .save_persisted(&PersistedDeviceCredential::new(
                "refresh-v2".into(),
                "user-1".into(),
                "dealer-1".into(),
            ))
            .expect("save v2");
        std::fs::write(store.credentials_path(), b"corrupt").expect("corrupt primary");

        let store2 = Arc::new(CredentialStore::new(temp.path().to_path_buf()));
        assert_eq!(store2.bootstrap_from_disk(), CredentialStatus::Paired);
        let loaded = store2.load_persisted().expect("load").expect("some");
        // Single-generation backup retains the previous good copy (v1).
        assert_eq!(loaded.refresh_token, "refresh-token-abc");
    }

    #[test]
    fn corrupt_file_marks_unreadable_when_backup_also_bad() {
        let (temp, store) = test_store();
        store.save_persisted(&sample_persisted()).expect("save");
        std::fs::write(store.credentials_path(), b"not-valid-ciphertext").expect("corrupt");
        std::fs::write(store.credentials_backup_path(), b"also-bad").expect("corrupt backup");
        let store2 = Arc::new(CredentialStore::new(temp.path().to_path_buf()));
        assert_eq!(store2.bootstrap_from_disk(), CredentialStatus::NeedsReconnect);
    }

    #[test]
    fn delete_removes_credential_files() {
        let (_temp, store) = test_store();
        store.save_persisted(&sample_persisted()).expect("save");
        assert!(store.credentials_path().exists());
        store.delete_persisted().expect("delete");
        assert!(!store.credentials_path().exists());
        assert!(!store.credentials_backup_path().exists());
        assert!(!store.key_path().exists());
    }

    #[test]
    fn revoked_device_clears_session() {
        let (_temp, store) = test_store();
        store
            .store_session(
                "access".into(),
                "refresh".into(),
                "user".into(),
                "dealer".into(),
            )
            .expect("store");
        store.handle_revoked_device().expect("revoke");
        assert_eq!(store.status(), CredentialStatus::Unpaired);
        assert!(!store.credentials_path().exists());
    }

    #[test]
    fn second_save_creates_single_backup_generation() {
        let (_temp, store) = test_store();
        store.save_persisted(&sample_persisted()).expect("save1");
        assert!(!store.credentials_backup_path().exists());
        store
            .save_persisted(&PersistedDeviceCredential::new(
                "refresh-2".into(),
                "user-1".into(),
                "dealer-1".into(),
            ))
            .expect("save2");
        assert!(store.credentials_backup_path().exists());
    }

    #[test]
    fn credential_files_are_not_world_readable_on_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let (_temp, store) = test_store();
            store.save_persisted(&sample_persisted()).expect("save");
            let mode = std::fs::metadata(store.credentials_path())
                .expect("meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
