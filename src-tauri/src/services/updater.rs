//! Best-effort auto-update via GitHub Releases (unsigned builds).
//!
//! Official `tauri-plugin-updater` requires signed updater artifacts + pubkey.
//! Until Apple/Windows code signing and Tauri updater signing are configured,
//! we poll the public Releases API, download the matching installer, and open
//! it for the user (macOS DMG → drag to Applications; Windows setup.exe/msi).
//!
//! After the installer opens, UI must stay finishable: clear “Installer open”
//! copy, an “I’ve finished installing” relaunch, and a 2-minute stall timeout
//! with Retry / Open installer again — never a forever-disabled Updating button.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::install_location::{is_running_from_applications, is_running_from_dmg_volume};
use crate::version::CONNECTOR_VERSION;

const GITHUB_LATEST_API: &str =
    "https://api.github.com/repos/7Socialapps/mlt-desktop-connector/releases/latest";
#[cfg(not(debug_assertions))]
const CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);
#[cfg(not(debug_assertions))]
const INITIAL_DELAY: Duration = Duration::from_secs(8);
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);
/// If the user is still on this old binary after the installer opened, unstick UI.
const INSTALL_STALL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Idle,
    Checking,
    Downloading,
    ReadyToInstall,
    /// Installer was opened but this process is still the old binary after the stall timeout.
    InstallStalled,
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
    /// Local path to the downloaded installer (DMG / EXE) for “Open installer again”.
    pub installer_path: Option<String>,
    pub timed_out: bool,
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
            installer_path: None,
            timed_out: false,
        }
    }
}

pub struct UpdaterService {
    state: Arc<Mutex<UpdateUiState>>,
    /// Prevent overlapping update runs.
    in_flight: Arc<Mutex<bool>>,
    /// Generation token so a later update run cancels a prior stall timer.
    stall_generation: Arc<Mutex<u64>>,
}

impl UpdaterService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(Mutex::new(UpdateUiState::default())),
            in_flight: Arc::new(Mutex::new(false)),
            stall_generation: Arc::new(Mutex::new(0)),
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

    /// Re-open the already-downloaded installer (DMG / setup).
    pub fn reopen_installer(&self) -> Result<(), String> {
        let path = {
            let guard = self.state.lock();
            guard
                .installer_path
                .clone()
                .ok_or_else(|| "No installer file on this computer yet. Click Retry.".to_string())?
        };
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err("Installer file is missing. Click Retry to download again.".into());
        }
        open_installer(&path)?;
        info!(path = %path.display(), "updater: reopened installer");
        Ok(())
    }

    /// After the user dragged to Applications: launch the installed app and exit this process.
    pub fn finish_and_relaunch(&self, app: &AppHandle) -> Result<(), String> {
        schedule_relaunch_installed_app()?;
        info!("updater: scheduled relaunch from Applications — exiting old process");
        app.exit(0);
        Ok(())
    }

    async fn check_and_apply(self: &Arc<Self>, app: &AppHandle, force_ui: bool) {
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

    async fn run_check(self: &Arc<Self>, app: &AppHandle, force_ui: bool) -> Result<(), String> {
        self.bump_stall_generation();

        // Kill the DMG loop: never download/update while running from a mounted volume.
        if is_running_from_dmg_volume() {
            info!(
                local = CONNECTOR_VERSION,
                "updater: skipping — running from DMG volume (not Applications)"
            );
            self.clear_to_idle(app, None);
            return Ok(());
        }

        // Auto-update only from a real Applications install (never Downloads/tmp copies).
        if !is_running_from_applications() {
            info!(
                local = CONNECTOR_VERSION,
                "updater: skipping — not running from Applications"
            );
            self.clear_to_idle(app, None);
            return Ok(());
        }

        // Silent check first — do NOT paint “Updating…” until we know remote > local.
        let release = fetch_latest_release().await?;
        let remote_version = normalize_version(&release.tag_name);
        if remote_version.is_empty() {
            return Err("Latest release has no version tag".into());
        }

        if !is_newer_version(&remote_version, CONNECTOR_VERSION) {
            info!(
                local = CONNECTOR_VERSION,
                remote = %remote_version,
                "updater: already up to date — never show Updating"
            );
            self.clear_to_idle(app, force_ui.then_some("You’re up to date."));
            return Ok(());
        }

        let asset = select_platform_asset(&release.assets).ok_or_else(|| {
            format!("Update {remote_version} is available, but no installer for this computer was found.")
        })?;

        // Guard: asset filename must advertise a version > local (avoids same-version re-download).
        if let Some(asset_ver) = version_from_asset_name(&asset.name) {
            if !is_newer_version(&asset_ver, CONNECTOR_VERSION) {
                info!(
                    local = CONNECTOR_VERSION,
                    asset = %asset.name,
                    asset_ver = %asset_ver,
                    "updater: asset is not newer — skipping download"
                );
                self.clear_to_idle(app, force_ui.then_some("You’re up to date."));
                return Ok(());
            }
        }

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
            None,
            false,
        );
        focus_main_window(app);

        let dest = download_dir(app)?.join(&asset.name);
        download_file(&asset.browser_download_url, &dest, app, self).await?;

        open_installer(&dest)?;
        info!(path = %dest.display(), "updater: opened installer");

        self.set_phase(
            app,
            UpdatePhase::ReadyToInstall,
            finish_message(&remote_version),
            Some(remote_version.clone()),
            100,
            true,
            Some(dest.display().to_string()),
            false,
        );
        focus_main_window(app);
        self.spawn_install_stall_timer(app.clone());
        Ok(())
    }

    fn spawn_install_stall_timer(self: &Arc<Self>, app: AppHandle) {
        let gen = *self.stall_generation.lock();
        let svc = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(INSTALL_STALL_TIMEOUT).await;
            if *svc.stall_generation.lock() != gen {
                return;
            }
            let still_waiting = {
                let guard = svc.state.lock();
                matches!(
                    guard.phase,
                    UpdatePhase::ReadyToInstall | UpdatePhase::InstallStalled
                )
            };
            if !still_waiting {
                return;
            }
            let (version, installer_path) = {
                let guard = svc.state.lock();
                (
                    guard
                        .available_version
                        .clone()
                        .unwrap_or_else(|| "?".into()),
                    guard.installer_path.clone(),
                )
            };
            warn!(
                local = CONNECTOR_VERSION,
                remote = %version,
                "updater: still on old binary after install stall timeout"
            );
            svc.set_phase(
                &app,
                UpdatePhase::InstallStalled,
                stall_message(&version),
                Some(version),
                100,
                true,
                installer_path,
                true,
            );
            focus_main_window(&app);
        });
    }

    fn bump_stall_generation(&self) {
        *self.stall_generation.lock() += 1;
    }

    #[allow(clippy::too_many_arguments)]
    fn set_phase(
        &self,
        app: &AppHandle,
        phase: UpdatePhase,
        message: impl Into<String>,
        available_version: Option<String>,
        progress: u8,
        active: bool,
        installer_path: Option<String>,
        timed_out: bool,
    ) {
        {
            let mut guard = self.state.lock();
            guard.phase = phase;
            guard.message = message.into();
            guard.available_version = available_version;
            guard.progress = progress;
            guard.active = active;
            guard.error = None;
            if installer_path.is_some() {
                guard.installer_path = installer_path;
            }
            guard.timed_out = timed_out;
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
            guard.timed_out = false;
        }
        emit(app, self.snapshot());
    }

    fn set_error(&self, app: &AppHandle, err: String) {
        self.bump_stall_generation();
        {
            let mut guard = self.state.lock();
            guard.active = false;
            guard.phase = UpdatePhase::Error;
            guard.error = Some(err.clone());
            guard.message =
                "Couldn’t update automatically. Try again or download from MLT.".into();
            guard.progress = 0;
            guard.timed_out = false;
        }
        emit(app, self.snapshot());
        let _ = err;
    }

    fn clear_to_idle(&self, app: &AppHandle, message: Option<&str>) {
        self.bump_stall_generation();
        {
            let mut guard = self.state.lock();
            guard.active = false;
            guard.phase = UpdatePhase::Idle;
            guard.message = message.unwrap_or("").into();
            guard.available_version = None;
            guard.progress = 0;
            guard.error = None;
            guard.installer_path = None;
            guard.timed_out = false;
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

fn installed_app_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Applications/MLT Desktop Connector.app")
    }
    #[cfg(target_os = "windows")]
    {
        // Best-effort: relaunch via Start Menu shortcut / default install path is handled by open.
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("MLT Desktop Connector.exe"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("mlt-desktop-connector"))
    }
}

/// Quit this (old) process, then open the copy in Applications / installed location.
fn schedule_relaunch_installed_app() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let app_path = installed_app_path();
        if !app_path.exists() {
            return Err(
                "Couldn’t find MLT Desktop Connector in Applications. Drag it from the installer window first."
                    .into(),
            );
        }
        // Delay so this process can exit before the new instance starts.
        let script = format!(
            "sleep 1; open '{}'",
            app_path.to_string_lossy().replace('\'', "'\\''")
        );
        std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .spawn()
            .map_err(|e| format!("Could not schedule relaunch: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        // Restart via the same EXE path after a short delay (installer usually replaces files).
        let exe = std::env::current_exe().map_err(|e| format!("exe path: {e}"))?;
        let script = format!(
            "Start-Sleep -Seconds 2; Start-Process -FilePath '{}'",
            exe.to_string_lossy().replace('\'', "''")
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .spawn()
            .map_err(|e| format!("Could not schedule relaunch: {e}"))?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Relaunch is not supported on this platform".into())
    }
}

fn finish_message(_version: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        "Installer open — drag to Applications, then reopen from Applications.".into()
    }
    #[cfg(target_os = "windows")]
    {
        "Installer open — finish the setup prompts, then click I’ve finished installing.".into()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Installer open — finish installing, then reopen the app.".into()
    }
}

fn stall_message(version: &str) -> String {
    format!(
        "Still on the old version. Finish installing {version}, or open the installer again."
    )
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

/// Parse `major.minor.patch` (ignores pre-release / build metadata after `-` or `+`).
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let core = normalize_version(raw);
    let core = core.split(|c| c == '-' || c == '+').next().unwrap_or(&core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// True only when remote is a strict semver greater than local.
/// Equal versions, parse failures, and prereleases that don't parse → false (never update).
pub fn is_newer_version(remote: &str, local: &str) -> bool {
    match (parse_semver(remote), parse_semver(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

/// Pull `1.2.3` from names like `MLT.Desktop.Connector_1.1.0_aarch64.dmg`.
pub fn version_from_asset_name(name: &str) -> Option<String> {
    let re_parts: Vec<&str> = name.split(['_', '-', ' ']).collect();
    for part in re_parts {
        let candidate = normalize_version(part);
        if parse_semver(&candidate).is_some() && candidate.contains('.') {
            return Some(candidate);
        }
    }
    None
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
        assert!(!is_newer_version("1.1.0", "1.1.0"));
        assert!(!is_newer_version("v1.1.0", "1.1.0"));
        assert!(!is_newer_version("not-a-version", "1.0.0"));
    }

    #[test]
    fn asset_name_version() {
        assert_eq!(
            version_from_asset_name("MLT.Desktop.Connector_1.1.0_aarch64.dmg").as_deref(),
            Some("1.1.0")
        );
        assert_eq!(
            version_from_asset_name("MLT.Desktop.Connector_1.0.9_x64.dmg").as_deref(),
            Some("1.0.9")
        );
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

    #[test]
    fn finish_message_is_one_line() {
        let msg = finish_message("1.0.9");
        assert!(!msg.is_empty());
        assert!(!msg.to_ascii_lowercase().contains("updating"));
    }
}
