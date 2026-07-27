use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};
use tracing::info;
use uuid::Uuid;

const DEVICE_ID_FILE: &str = "device_id";

pub fn device_id_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("failed to resolve app data directory")?;
    std::fs::create_dir_all(&dir).context("failed to create app data directory")?;
    Ok(dir.join(DEVICE_ID_FILE))
}

pub fn load_or_create_device_id(app: &AppHandle) -> Result<Uuid> {
    let path = device_id_path(app)?;

    if path.exists() {
        let raw = std::fs::read_to_string(&path).context("failed to read device_id file")?;
        let trimmed = raw.trim();
        if let Ok(id) = Uuid::parse_str(trimmed) {
            info!(device_id = %id, "loaded persisted device id");
            return Ok(id);
        }
        tracing::warn!("invalid device_id file contents — regenerating");
    }

    let id = Uuid::new_v4();
    std::fs::write(&path, id.to_string()).context("failed to write device_id file")?;
    info!(device_id = %id, "generated new device id");
    Ok(id)
}
