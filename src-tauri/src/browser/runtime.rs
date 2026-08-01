use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::Mutex;
use tracing::{info, warn};

use super::sidecar::{close_test, detect_runtime, launch_test, resolve_sidecar_cli};
use super::types::{BrowserRuntimeSnapshot, BrowserRuntimeStatus, SidecarDetectResponse};

pub struct BrowserRuntimeService {
    cli_path: PathBuf,
    state: Arc<Mutex<BrowserRuntimeSnapshot>>,
}

impl BrowserRuntimeService {
    pub fn new(enabled: bool) -> Self {
        let cli_path = resolve_sidecar_cli().unwrap_or_else(|err| {
            warn!(error = %err, "browser sidecar CLI unavailable at startup");
            PathBuf::new()
        });

        Self {
            cli_path,
            state: Arc::new(Mutex::new(BrowserRuntimeSnapshot {
                status: if enabled {
                    BrowserRuntimeStatus::BrowserNotInstalled
                } else {
                    BrowserRuntimeStatus::BrowserStopped
                },
                enabled,
                playwright_installed: false,
                playwright_version: None,
                chromium_installed: false,
                chromium_path: None,
                node_version: None,
                last_error: None,
                last_error_code: None,
                checked_at: None,
            })),
        }
    }

    pub fn snapshot(&self) -> BrowserRuntimeSnapshot {
        self.state.lock().clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.state.lock().enabled
    }

    pub fn cli_path(&self) -> &PathBuf {
        &self.cli_path
    }

    fn set_status(&self, status: BrowserRuntimeStatus) {
        self.state.lock().status = status;
    }

    fn apply_detect(&self, detect: SidecarDetectResponse) {
        let mut guard = self.state.lock();
        guard.checked_at = Some(Utc::now().to_rfc3339());
        guard.playwright_installed = detect.playwright_installed;
        guard.playwright_version = detect.playwright_version;
        guard.chromium_installed = detect.chromium_installed;
        guard.chromium_path = detect.chromium_path;
        guard.node_version = detect.node_version;
        guard.last_error = detect.detect_error.or(detect.error);
        guard.last_error_code = detect.error_code;

        guard.status = if !guard.enabled {
            BrowserRuntimeStatus::BrowserStopped
        } else if !detect.playwright_installed {
            BrowserRuntimeStatus::BrowserNotInstalled
        } else if !detect.chromium_installed {
            BrowserRuntimeStatus::BrowserNotInstalled
        } else {
            BrowserRuntimeStatus::BrowserInstalled
        };
    }

    pub fn detect(&self) -> Result<BrowserRuntimeSnapshot, String> {
        if !self.state.lock().enabled {
            self.set_status(BrowserRuntimeStatus::BrowserStopped);
            return Ok(self.snapshot());
        }

        if self.cli_path.as_os_str().is_empty() {
            self.state.lock().last_error = Some("Browser sidecar CLI not found".into());
            self.state.lock().last_error_code = Some("SIDECAR_NOT_FOUND".into());
            self.set_status(BrowserRuntimeStatus::BrowserError);
            return Ok(self.snapshot());
        }

        info!("browser runtime detection started");
        match detect_runtime(&self.cli_path) {
            Ok(detect) => {
                if let Some(path) = &detect.chromium_path {
                    info!(chromium_path = %path, "chromium binary located");
                }
                info!(
                    playwright_installed = detect.playwright_installed,
                    chromium_installed = detect.chromium_installed,
                    playwright_version = ?detect.playwright_version,
                    "browser runtime detection complete"
                );
                self.apply_detect(detect);
                Ok(self.snapshot())
            }
            Err(err) => {
                warn!(error = %err, "browser runtime detection failed");
                {
                    let mut guard = self.state.lock();
                    guard.last_error = Some(err.to_string());
                    guard.last_error_code = Some("RUNTIME_DETECT_FAILED".into());
                    guard.status = BrowserRuntimeStatus::BrowserError;
                    guard.checked_at = Some(Utc::now().to_rfc3339());
                }
                Ok(self.snapshot())
            }
        }
    }

    pub fn test_launch(&self) -> Result<BrowserRuntimeSnapshot, String> {
        if !self.state.lock().enabled {
            return Err("Browser subsystem disabled".into());
        }
        if self.cli_path.as_os_str().is_empty() {
            return Err("Browser sidecar CLI not found".into());
        }

        self.set_status(BrowserRuntimeStatus::BrowserStarting);
        info!("playwright test browser launch requested");

        match launch_test(&self.cli_path) {
            Ok(resp) if resp.ok => {
                info!("playwright test browser launched");
                self.set_status(BrowserRuntimeStatus::BrowserReady);
                Ok(self.snapshot())
            }
            Ok(resp) => {
                let msg = resp.error.unwrap_or_else(|| "Launch failed".into());
                warn!(error = %msg, "playwright test browser launch failed");
                self.state.lock().last_error = Some(msg.clone());
                self.state.lock().last_error_code = resp.error_code;
                self.set_status(BrowserRuntimeStatus::BrowserError);
                Err(msg)
            }
            Err(err) => {
                warn!(error = %err, "playwright test browser launch failed");
                self.state.lock().last_error = Some(err.to_string());
                self.state.lock().last_error_code = Some("LAUNCH_FAILED".into());
                self.set_status(BrowserRuntimeStatus::BrowserError);
                Err(err.to_string())
            }
        }
    }

    pub fn test_close(&self) -> Result<BrowserRuntimeSnapshot, String> {
        if self.cli_path.as_os_str().is_empty() {
            return Err("Browser sidecar CLI not found".into());
        }

        info!("playwright test browser close requested");
        match close_test(&self.cli_path) {
            Ok(resp) if resp.ok => {
                info!("playwright test browser closed");
                let snapshot = self.detect()?;
                Ok(snapshot)
            }
            Ok(resp) => {
                let msg = resp.error.unwrap_or_else(|| "Close failed".into());
                self.state.lock().last_error = Some(msg.clone());
                self.set_status(BrowserRuntimeStatus::BrowserError);
                Err(msg)
            }
            Err(err) => {
                self.state.lock().last_error = Some(err.to_string());
                self.set_status(BrowserRuntimeStatus::BrowserError);
                Err(err.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_status_labels_are_user_facing() {
        assert!(BrowserRuntimeStatus::BrowserNotInstalled
            .label()
            .contains("not installed"));
        assert!(BrowserRuntimeStatus::BrowserReady.label().contains("ready"));
    }

    #[test]
    fn apply_detect_maps_installed_state() {
        let svc = BrowserRuntimeService::new(true);
        svc.apply_detect(SidecarDetectResponse {
            ok: true,
            playwright_installed: true,
            playwright_version: Some("1.52.0".into()),
            chromium_installed: true,
            chromium_path: Some("/tmp/chromium".into()),
            node_version: Some("v20.0.0".into()),
            browsers_path: None,
            detect_error: None,
            error: None,
            error_code: None,
        });
        let snap = svc.snapshot();
        assert_eq!(snap.status, BrowserRuntimeStatus::BrowserInstalled);
        assert!(snap.chromium_installed);
    }

    #[test]
    fn apply_detect_maps_missing_chromium() {
        let svc = BrowserRuntimeService::new(true);
        svc.apply_detect(SidecarDetectResponse {
            ok: true,
            playwright_installed: true,
            playwright_version: Some("1.52.0".into()),
            chromium_installed: false,
            chromium_path: None,
            node_version: Some("v20.0.0".into()),
            browsers_path: None,
            detect_error: Some("Executable doesn't exist".into()),
            error: None,
            error_code: None,
        });
        assert_eq!(
            svc.snapshot().status,
            BrowserRuntimeStatus::BrowserNotInstalled
        );
    }

    #[test]
    fn apply_detect_when_disabled_stays_stopped() {
        let svc = BrowserRuntimeService::new(false);
        svc.apply_detect(SidecarDetectResponse {
            ok: true,
            playwright_installed: true,
            playwright_version: Some("1.52.0".into()),
            chromium_installed: true,
            chromium_path: Some("/tmp/chromium".into()),
            node_version: Some("v20.0.0".into()),
            browsers_path: None,
            detect_error: None,
            error: None,
            error_code: None,
        });
        assert_eq!(svc.snapshot().status, BrowserRuntimeStatus::BrowserStopped);
    }

    #[test]
    fn apply_detect_missing_playwright_is_not_installed() {
        let svc = BrowserRuntimeService::new(true);
        svc.apply_detect(SidecarDetectResponse {
            ok: true,
            playwright_installed: false,
            playwright_version: None,
            chromium_installed: false,
            chromium_path: None,
            node_version: Some("v20.0.0".into()),
            browsers_path: None,
            detect_error: None,
            error: None,
            error_code: None,
        });
        assert_eq!(
            svc.snapshot().status,
            BrowserRuntimeStatus::BrowserNotInstalled
        );
        assert!(!svc.snapshot().playwright_installed);
    }
}
