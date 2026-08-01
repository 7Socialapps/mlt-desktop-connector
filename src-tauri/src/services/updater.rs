//! Best-effort auto-update via GitHub Releases (unsigned builds).
//!
//! Official `tauri-plugin-updater` requires signed updater artifacts + pubkey.
//! Until Apple/Windows code signing and Tauri updater signing are configured,
//! we poll the public Releases API, download the matching installer, and open
//! it for the user (macOS DMG → drag to Applications; Windows setup.exe/msi).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::version::CONNECTOR_VERSION;

const GITHUB_LATEST_API: &str =
    "https://api.github.com/repos/7Socialapps/mlt-desktop-connector/releases/latest";
#[cfg(not(debug_assertions))]
const CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);
#[cfg(not(debug_assertions))]
const INITIAL_DELAY: Duration = Duration::from_secs(8);
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Idle,
    Checking,
    Downloading,
    ReadyToInstall,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateUiState {
    pub active: bool,
    pub phase: UpdatePhase,
    pub message: String,
    pub available_version: Option<String>,
    pub progress: u8,
    pub error: Option<String>,
}

impl Default for UpdateUiState {
    fn default() -> Self {
        Self {
            active: false,
            phase: UpdatePhase::Idle,
            message: String::new(),
            available_version: None,
            progress: 0,
            error: None,
        }
    }
}

pub struct UpdaterService {
    state: Arc<Mutex<UpdateUiState>>,
    /// Prevent overlapping update runs.
    in_flight: Arc<Mutex<bool>>,
}

impl UpdaterService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(Mutex::new(UpdateUiState::default())),
            in_flight: Arc::new(Mutex::new(false)),
        })
    }

    pub fn snapshot(&self) -> UpdateUiState {
        self.state.lock().clone()
    }

    /// Start periodic checks (launch + every few hours). No-op in debug builds.
    pub fn spawn_periodic(self: &Arc<Self>, app: AppHandle) {
        #[cfg(debug_assertions)]
        {
            info!("updater: skipping periodic checks in debug builds");
            let _ = app;
            return;
        }
        #[cfg(not(debug_assertions))]
        {
            let svc = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(INITIAL_DELAY).await;
                svc.check_and_apply(&app, false).await;
                let mut ticker = tokio::time::interval(CHECK_INTERVAL);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // Consume the immediate first tick so the next check waits a full interval.
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    svc.check_and_apply(&app, false).await;
                }
            });
        }
    }

    /// Manual / deep-link trigger (`mlt-desktop://check-update`).
    pub fn request_check(self: &Arc<Self>, app: AppHandle) {
        let svc = self.clone();
        tauri::async_runtime::spawn(async move {
            svc.check_and_apply(&app, true).await;
        });
    }

    async fn check_and_apply(&self, app: &AppHandle, force_ui: bool) {
        {
            let mut guard = self.in_flight.lock();
            if *guard {
                return;
            }
            *guard = true;
        }

        let result = self.run_check(app, force_ui).await;

        *self.in_flight.lock() = false;

        if let Err(err) = result {
            warn!(error = %err, "updater check failed");
            if force_ui || self.state.lock().active {
                self.set_error(app, err);
            }
        }
    }

    async fn run_check(&self, app: &AppHandle, force_ui: bool) -> Result<(), String> {
        self.set_phase(
            app,
            UpdatePhase::Checking,
            "Checking for updates…",
            None,
            5,
            force_ui,
        );

        let release = fetch_latest_release().await?;
        let remote_version = normalize_version(&release.tag_name);
        if remote_version.is_empty() {
            return Err("Latest release has no version tag".into());
        }

        if !is_newer_version(&remote_version, CONNECTOR_VERSION) {
            info!(
                local = CONNECTOR_VERSION,
                remote = %remote_version,
                "updater: already up to date"
            );
            self.clear_to_idle(app, force_ui.then_some("You’re up to date."));
            return Ok(());
        }

        let asset = select_platform_asset(&release.assets).ok_or_else(|| {
            format!("Update {remote_version} is available, but no installer for this computer was found.")
        })?;

        info!(
            local = CONNECTOR_VERSION,
            remote = %remote_version,
            asset = %asset.name,
            "updater: newer release found — downloading"
        );

        self.set_phase(
            app,
            UpdatePhase::Downloading,
            format!("Updating to {remote_version}…"),
            Some(remote_version.clone()),
            20,
            true,
        );
        focus_main_window(app);

        let dest = download_dir(app)?.join(&asset.name);
        download_file(&asset.browser_download_url, &dest, app, self).await?;

        self.set_phase(
            app,
            UpdatePhase::ReadyToInstall,
            finish_message(&remote_version),
            Some(remote_version.clone()),
            100,
            true,
        );

        open_installer(&dest)?;
        info!(path = %dest.display(), "updater: opened installer");
        Ok(())
    }

    fn set_phase(
        &self,
        app: &AppHandle,
        phase: UpdatePhase,
        message: impl Into<String>,
        available_version: Option<String>,
        progress: u8,
        active: bool,
    ) {
        {
            let mut guard = self.state.lock();
            guard.phase = phase;
            guard.message = message.into();
            guard.available_version = available_version;
            guard.progress = progress;
            guard.active = active;
            guard.error = None;
        }
        emit(app, self.snapshot());
    }

    fn set_progress(&self, app: &AppHandle, progress: u8, message: impl Into<String>) {
        {
            let mut guard = self.state.lock();
            guard.progress = progress;
            guard.message = message.into();
            guard.active = true;
            guard.phase = UpdatePhase::Downloading;
        }
        emit(app, self.snapshot());
    }

    fn set_error(&self, app: &AppHandle, err: String) {
        {
            let mut guard = self.state.lock();
            guard.active = false;
            guard.phase = UpdatePhase::Error;
            guard.error = Some(err.clone());
            guard.message = "Couldn’t update automatically. Try again later or reinstall from MLT.".into();
            guard.progress = 0;
        }
        emit(app, self.snapshot());
        let _ = err;
    }

    fn clear_to_idle(&self, app: &AppHandle, message: Option<&str>) {
        {
            let mut guard = self.state.lock();
            guard.active = false;
            guard.phase = UpdatePhase::Idle;
            guard.message = message.unwrap_or("").into();
            guard.available_version = None;
            guard.progress = 0;
            guard.error = None;
        }
        emit(app, self.snapshot());
    }
}

#[derive(Debug, serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[allow(dead_code)]
    size: u64,
}

async fn fetch_latest_release() -> Result<GhRelease, String> {
    let client = http_client()?;
    let response = client
        .get(GITHUB_LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("Could not reach update server: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Update server returned HTTP {}",
            response.status()
        ));
    }

    response
        .json::<GhRelease>()
        .await
        .map_err(|e| format!("Could not read update info: {e}"))
}

async fn download_file(
    url: &str,
    dest: &Path,
    app: &AppHandle,
    svc: &UpdaterService,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("temp dir: {e}"))?;
    }

    let client = http_client()?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Download failed (HTTP {})", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if total > 0 {
        let pct = ((bytes.len() as u64).min(total) * 70 / total) as u8 + 20;
        svc.set_progress(app, pct.min(90), "Downloading update…");
    } else {
        svc.set_progress(app, 70, "Downloading update…");
    }

    std::fs::write(dest, &bytes).map_err(|e| format!("Could not save update: {e}"))?;
    Ok(())
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(format!(
            "MLT-Desktop-Connector/{} (+https://github.com/7Socialapps/mlt-desktop-connector)",
            CONNECTOR_VERSION
        ))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))
}

fn download_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("cache dir: {e}"))?;
    Ok(base.join("updates"))
}

fn open_installer(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Could not open installer: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(path)
            .spawn()
            .map_err(|e| format!("Could not open installer: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Could not open installer: {e}"))?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        Err("Updates are not supported on this platform".into())
    }
}

fn finish_message(version: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!(
            "Update {version} downloaded. Drag MLT Desktop Connector to Applications to finish, then reopen the app."
        )
    }
    #[cfg(target_os = "windows")]
    {
        format!(
            "Update {version} is installing. Follow the installer prompts, then reopen the app."
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        format!("Update {version} downloaded. Open the installer to finish.")
    }
}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn emit(app: &AppHandle, state: UpdateUiState) {
    let _ = app.emit("connector://update-changed", state);
}

pub fn normalize_version(raw: &str) -> String {
    raw.trim().trim_start_matches('v').trim().to_string()
}

/// Parse `major.minor.patch` (ignores pre-release suffix after `-`).
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let core = normalize_version(raw);
    let core = core.split('-').next().unwrap_or(&core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer_version(remote: &str, local: &str) -> bool {
    match (parse_semver(remote), parse_semver(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

pub fn select_platform_asset(assets: &[GhAsset]) -> Option<GhAsset> {
    select_platform_asset_for(assets, std::env::consts::OS, std::env::consts::ARCH)
}

fn select_platform_asset_for(assets: &[GhAsset], os: &str, arch: &str) -> Option<GhAsset> {
    let mut scored: Vec<(i32, &GhAsset)> = assets
        .iter()
        .filter_map(|asset| {
            let name = asset.name.to_ascii_lowercase();
            let score = match os {
                "macos" => score_macos_asset(&name, arch),
                "windows" => score_windows_asset(&name),
                "linux" => score_linux_asset(&name, arch),
                _ => -1,
            };
            if score >= 0 {
                Some((score, asset))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.first().map(|(_, a)| (*a).clone())
}

fn score_macos_asset(name: &str, arch: &str) -> i32 {
    if !name.ends_with(".dmg") {
        return -1;
    }
    let arch_match = match arch {
        "aarch64" => name.contains("aarch64") || name.contains("arm64"),
        "x86_64" => name.contains("x64") || name.contains("x86_64"),
        _ => false,
    };
    if arch_match {
        100
    } else {
        -1
    }
}

fn score_windows_asset(name: &str) -> i32 {
    if name.contains("setup.exe") || (name.ends_with(".exe") && name.contains("x64")) {
        return 100;
    }
    if name.ends_with(".msi") {
        return 80;
    }
    if name.ends_with(".exe") {
        return 50;
    }
    -1
}

fn score_linux_asset(name: &str, arch: &str) -> i32 {
    let arch_ok = match arch {
        "aarch64" => name.contains("aarch64") || name.contains("arm64"),
        "x86_64" => name.contains("x64") || name.contains("amd64") || name.contains("x86_64"),
        _ => false,
    };
    if !arch_ok {
        return -1;
    }
    if name.ends_with(".appimage") {
        100
    } else if name.ends_with(".deb") {
        80
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GhAsset {
        GhAsset {
            name: name.into(),
            browser_download_url: format!("https://example.com/{name}"),
            size: 1,
        }
    }

    #[test]
    fn semver_newer() {
        assert!(is_newer_version("1.0.6", "1.0.5"));
        assert!(is_newer_version("v1.1.0", "1.0.9"));
        assert!(!is_newer_version("1.0.5", "1.0.5"));
        assert!(!is_newer_version("1.0.4", "1.0.5"));
    }

    #[test]
    fn selects_mac_aarch64_dmg() {
        let assets = vec![
            asset("MLT.Desktop.Connector_1.0.6_x64.dmg"),
            asset("MLT.Desktop.Connector_1.0.6_aarch64.dmg"),
            asset("MLT Desktop Connector_1.0.6_x64-setup.exe"),
        ];
        let picked = select_platform_asset_for(&assets, "macos", "aarch64").unwrap();
        assert!(picked.name.contains("aarch64"));
    }

    #[test]
    fn selects_windows_setup_exe() {
        let assets = vec![
            asset("MLT.Desktop.Connector_1.0.6_x64.dmg"),
            asset("MLT Desktop Connector_1.0.6_x64-setup.exe"),
            asset("MLT Desktop Connector_1.0.6_x64_en-US.msi"),
        ];
        let picked = select_platform_asset_for(&assets, "windows", "x86_64").unwrap();
        assert!(picked.name.contains("setup.exe"));
    }

    #[test]
    fn normalize_strips_v() {
        assert_eq!(normalize_version("v1.0.6"), "1.0.6");
    }
}
