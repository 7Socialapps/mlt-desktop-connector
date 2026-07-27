use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tracing::info;

const PROFILE_DIR_NAME: &str = "browser-profile";
const LOCK_FILE_NAME: &str = ".profile.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    ProfileMissing,
    ProfileInitializing,
    ProfileReady,
    ProfileLocked,
    ProfileCorrupt,
    ProfileResetRequired,
}

impl ProfileStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProfileMissing => "Browser profile not created yet",
            Self::ProfileInitializing => "Browser profile initializing",
            Self::ProfileReady => "Browser profile ready",
            Self::ProfileLocked => "Browser profile locked by another process",
            Self::ProfileCorrupt => "Browser profile may be corrupt",
            Self::ProfileResetRequired => "Browser profile reset required",
        }
    }

    pub fn remediation(&self) -> Option<&'static str> {
        match self {
            Self::ProfileLocked => Some(
                "Close other MLT Desktop Connector windows or restart your computer, then try again.",
            ),
            Self::ProfileCorrupt | Self::ProfileResetRequired => Some(
                "Use Reset Browser Profile after closing the browser. You will need to sign into Facebook again.",
            ),
            Self::ProfileMissing => Some("Launch the browser to create a persistent profile."),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProfileSnapshot {
    pub status: ProfileStatus,
    pub profile_path: String,
    pub checked_at: Option<String>,
}

pub fn resolve_profile_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data directory: {e}"))?;
    Ok(base.join(PROFILE_DIR_NAME))
}

pub fn resolve_diagnostics_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data directory: {e}"))?;
    Ok(base.join("diagnostics"))
}

pub fn lock_file_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(LOCK_FILE_NAME)
}

pub fn inspect_local_profile(profile_dir: &Path) -> ProfileStatus {
    if !profile_dir.exists() {
        return ProfileStatus::ProfileMissing;
    }

    let lock_path = lock_file_path(profile_dir);
    if lock_path.exists() {
        if let Ok(raw) = fs::read_to_string(&lock_path) {
            if let Ok(lock) = serde_json::from_str::<ProfileLockFile>(&raw) {
                if is_pid_alive(lock.pid) {
                    return ProfileStatus::ProfileLocked;
                }
            }
        }
        // Stale lock — sidecar will reclaim on launch.
    }

    if profile_dir.join("Default").exists() || profile_dir.join("Local State").exists() {
        ProfileStatus::ProfileReady
    } else if profile_dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
        ProfileStatus::ProfileReady
    } else {
        ProfileStatus::ProfileMissing
    }
}

pub fn reset_profile_dir(profile_dir: &Path) -> Result<(), String> {
    if profile_dir.exists() {
        fs::remove_dir_all(profile_dir)
            .map_err(|e| format!("failed to remove browser profile: {e}"))?;
        info!(path = %profile_dir.display(), "browser profile directory removed");
    }
    Ok(())
}

pub fn ensure_profile_parent(profile_dir: &Path) -> Result<(), String> {
    if let Some(parent) = profile_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create profile parent directory: {e}"))?;
    }
    Ok(())
}

pub fn snapshot_from_path(profile_dir: &Path, status: ProfileStatus) -> ProfileSnapshot {
    ProfileSnapshot {
        status,
        profile_path: profile_dir.to_string_lossy().into_owned(),
        checked_at: Some(Utc::now().to_rfc3339()),
    }
}

#[derive(Debug, Deserialize)]
struct ProfileLockFile {
    pid: u32,
}

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        use std::process::Command;
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .any(|l| l.contains(&pid.to_string()))
            })
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_profile_dir_is_profile_missing() {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("browser-profile");
        assert_eq!(inspect_local_profile(&profile), ProfileStatus::ProfileMissing);
    }

    #[test]
    fn empty_profile_dir_is_profile_missing() {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("browser-profile");
        fs::create_dir_all(&profile).unwrap();
        assert_eq!(inspect_local_profile(&profile), ProfileStatus::ProfileMissing);
    }

    #[test]
    fn profile_with_default_dir_is_ready() {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("browser-profile");
        fs::create_dir_all(profile.join("Default")).unwrap();
        assert_eq!(inspect_local_profile(&profile), ProfileStatus::ProfileReady);
    }

    #[test]
    fn reset_removes_profile_dir() {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("browser-profile");
        fs::create_dir_all(profile.join("Default")).unwrap();
        reset_profile_dir(&profile).unwrap();
        assert!(!profile.exists());
    }

    #[test]
    fn profile_status_labels_are_user_facing() {
        assert!(ProfileStatus::ProfileLocked.label().contains("locked"));
        assert!(ProfileStatus::ProfileCorrupt.label().contains("corrupt"));
    }
}
