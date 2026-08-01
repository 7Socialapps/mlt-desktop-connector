//! Detect whether this process is running from a mounted DMG vs Applications.
//! Critical for killing the “open DMG → Updating → download again” loop.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeLocation {
    /// macOS: executable lives under `/Volumes/…` (DMG mount).
    pub from_dmg_volume: bool,
    /// macOS: executable lives under `/Applications/…`.
    pub from_applications: bool,
    pub exe_path: String,
}

impl RuntimeLocation {
    pub fn detect() -> Self {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("unknown"));
        let path = exe.to_string_lossy().to_string();
        Self {
            from_dmg_volume: is_path_on_dmg_volume(&path),
            from_applications: is_path_in_applications(&path),
            exe_path: path,
        }
    }
}

pub fn is_running_from_dmg_volume() -> bool {
    RuntimeLocation::detect().from_dmg_volume
}

pub fn is_running_from_applications() -> bool {
    RuntimeLocation::detect().from_applications
}

fn is_path_on_dmg_volume(path: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        path.starts_with("/Volumes/")
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

fn is_path_in_applications(path: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        path.starts_with("/Applications/")
    }
    #[cfg(target_os = "windows")]
    {
        // Treat any non-temp launch as “installed” for auto-update gating.
        let lower = path.to_ascii_lowercase();
        !lower.contains("\\appdata\\local\\temp\\") && !lower.contains("\\temp\\")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dmg_volume() {
        assert!(is_path_on_dmg_volume(
            "/Volumes/MLT Desktop Connector/MLT Desktop Connector.app/Contents/MacOS/mlt-desktop-connector"
        ));
        assert!(!is_path_on_dmg_volume(
            "/Applications/MLT Desktop Connector.app/Contents/MacOS/mlt-desktop-connector"
        ));
    }

    #[test]
    fn detects_applications() {
        assert!(is_path_in_applications(
            "/Applications/MLT Desktop Connector.app/Contents/MacOS/mlt-desktop-connector"
        ));
        assert!(!is_path_in_applications(
            "/Volumes/MLT Desktop Connector/MLT Desktop Connector.app/Contents/MacOS/x"
        ));
    }
}
