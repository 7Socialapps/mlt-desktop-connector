use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use super::types::{SidecarDetectResponse, SidecarSimpleResponse};

fn browser_test_state_path() -> String {
    if let Ok(dir) = std::env::var("MLT_BROWSER_TEST_STATE_DIR") {
        return PathBuf::from(dir)
            .join("browser-test-state.json")
            .to_string_lossy()
            .into_owned();
    }
    // Dev fallback under sidecar folder; production BrowserManager uses app-data (2.2+).
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("..")
        .join("browser-sidecar")
        .join(".browser-test-state.json")
        .to_string_lossy()
        .into_owned()
}

pub fn resolve_sidecar_cli() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("MLT_BROWSER_SIDECAR_CLI") {
        let path = PathBuf::from(custom);
        if path.exists() {
            return Ok(path);
        }
        anyhow::bail!("MLT_BROWSER_SIDECAR_CLI points to missing file: {}", path.display());
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_path = manifest_dir
        .join("..")
        .join("browser-sidecar")
        .join("cli.mjs");
    if dev_path.exists() {
        return Ok(dev_path.canonicalize().unwrap_or(dev_path));
    }

    anyhow::bail!(
        "browser sidecar CLI not found (expected {}). Set MLT_BROWSER_SIDECAR_CLI for custom builds.",
        dev_path.display()
    );
}

fn resolve_node_binary() -> PathBuf {
    if let Ok(node) = std::env::var("MLT_NODE_BINARY") {
        return PathBuf::from(node);
    }
    PathBuf::from("node")
}

pub fn run_sidecar_command<T: serde::de::DeserializeOwned>(
    cli_path: &Path,
    command: &str,
) -> Result<T> {
    let node = resolve_node_binary();
    info!(
        command,
        node = %node.display(),
        sidecar = %cli_path.display(),
        "playwright sidecar command"
    );

    let output = Command::new(&node)
        .arg(cli_path)
        .arg(command)
        .env(
            "MLT_BROWSER_TEST_STATE_FILE",
            browser_test_state_path(),
        )
        .output()
        .with_context(|| format!("failed to spawn sidecar command `{command}`"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stderr.trim().is_empty() {
        debug!(stderr = %stderr.trim(), "playwright sidecar stderr");
    }

    if !output.status.success() {
        warn!(
            command,
            exit_code = ?output.status.code(),
            stderr = %stderr.trim(),
            "playwright sidecar command failed"
        );
        if let Ok(parsed) = serde_json::from_str::<T>(&stdout) {
            return Ok(parsed);
        }
        anyhow::bail!(
            "sidecar `{command}` failed (exit {:?}): {}",
            output.status.code(),
            if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            }
        );
    }

    let line = stdout.lines().last().unwrap_or("").trim();
    serde_json::from_str(line).with_context(|| {
        format!("failed to parse sidecar `{command}` JSON output: {line}")
    })
}

pub fn detect_runtime(cli_path: &Path) -> Result<SidecarDetectResponse> {
    run_sidecar_command(cli_path, "detect")
}

pub fn launch_test(cli_path: &Path) -> Result<SidecarSimpleResponse> {
    run_sidecar_command(cli_path, "launch-test")
}

pub fn close_test(cli_path: &Path) -> Result<SidecarSimpleResponse> {
    run_sidecar_command(cli_path, "close-test")
}
