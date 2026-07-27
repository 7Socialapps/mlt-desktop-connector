use anyhow::{Context, Result};
use keyring::Entry;
use tracing::{info, warn};

const SERVICE_NAME: &str = "mlt-desktop-connector";

const ACCESS_TOKEN_KEY: &str = "access_token";
const REFRESH_TOKEN_KEY: &str = "refresh_token";
const USER_ID_KEY: &str = "user_id";
const DEALERSHIP_ID_KEY: &str = "dealership_id";

fn entry(account: &str) -> Result<Entry> {
    Entry::new(SERVICE_NAME, account).context("failed to create keyring entry")
}

pub struct StoredCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub dealership_id: String,
}

pub fn store_credentials(creds: &StoredCredentials) -> Result<()> {
    entry(ACCESS_TOKEN_KEY)?.set_password(&creds.access_token)?;
    entry(REFRESH_TOKEN_KEY)?.set_password(&creds.refresh_token)?;
    entry(USER_ID_KEY)?.set_password(&creds.user_id)?;
    entry(DEALERSHIP_ID_KEY)?.set_password(&creds.dealership_id)?;
    info!("stored device credentials in OS keychain");
    Ok(())
}

pub fn load_credentials() -> Result<Option<StoredCredentials>> {
    let access = match entry(ACCESS_TOKEN_KEY)?.get_password() {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let refresh = entry(REFRESH_TOKEN_KEY)?.get_password()?;
    let user_id = entry(USER_ID_KEY)?.get_password()?;
    let dealership_id = entry(DEALERSHIP_ID_KEY)?.get_password()?;

    Ok(Some(StoredCredentials {
        access_token: access,
        refresh_token: refresh,
        user_id,
        dealership_id,
    }))
}

pub fn clear_credentials() -> Result<()> {
    for key in [
        ACCESS_TOKEN_KEY,
        REFRESH_TOKEN_KEY,
        USER_ID_KEY,
        DEALERSHIP_ID_KEY,
    ] {
        if let Err(e) = entry(key)?.delete_credential() {
            if !matches!(e, keyring::Error::NoEntry) {
                warn!(key, error = %e, "failed to delete keyring entry");
            }
        }
    }
    info!("cleared device credentials from OS keychain");
    Ok(())
}

pub fn has_access_token() -> bool {
    match entry(ACCESS_TOKEN_KEY) {
        Ok(e) => matches!(e.get_password(), Ok(_)),
        Err(_) => false,
    }
}
