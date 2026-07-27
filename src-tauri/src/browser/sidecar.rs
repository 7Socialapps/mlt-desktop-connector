use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::Serialize;
use tracing::{debug, info, warn};

use super::types::{SidecarDaemonLine, SidecarDetectResponse, SidecarSimpleResponse};

fn browser_test_state_path() -> String {
    if let Ok(dir) = std::env::var("MLT_BROWSER_TEST_STATE_DIR") {
        return PathBuf::from(dir)
            .join("browser-test-state.json")
            .to_string_lossy()
            .into_owned();
    }
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
        anyhow::bail!(
            "MLT_BROWSER_SIDECAR_CLI points to missing file: {}",
            path.display()
        );
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

pub fn resolve_sidecar_server() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("MLT_BROWSER_SIDECAR_SERVER") {
        let path = PathBuf::from(custom);
        if path.exists() {
            return Ok(path);
        }
        anyhow::bail!(
            "MLT_BROWSER_SIDECAR_SERVER points to missing file: {}",
            path.display()
        );
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_path = manifest_dir
        .join("..")
        .join("browser-sidecar")
        .join("server.mjs");
    if dev_path.exists() {
        return Ok(dev_path.canonicalize().unwrap_or(dev_path));
    }

    anyhow::bail!(
        "browser sidecar server not found (expected {}). Set MLT_BROWSER_SIDECAR_SERVER for custom builds.",
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
        .env("MLT_BROWSER_TEST_STATE_FILE", browser_test_state_path())
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
    serde_json::from_str(line)
        .with_context(|| format!("failed to parse sidecar `{command}` JSON output: {line}"))
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

#[derive(Debug, Clone)]
pub enum SidecarEvent {
    Ready,
    BrowserStarting,
    BrowserReady { pid: Option<u32> },
    BrowserStopped { pid: Option<u32> },
    BrowserDisconnected { pid: Option<u32> },
    FacebookSessionChanged,
    MarketplaceStatusChanged,
    DaemonShutdown,
}

struct SidecarProcess {
    child: Child,
    stdin: ChildStdin,
    reader: JoinHandle<()>,
}

pub struct SidecarDaemon {
    server_path: PathBuf,
    profile_dir: Mutex<Option<PathBuf>>,
    diagnostics_dir: Mutex<Option<PathBuf>>,
    process: Mutex<Option<SidecarProcess>>,
    request_counter: AtomicU64,
    pending: Arc<Mutex<HashMap<String, Sender<Result<SidecarDaemonLine>>>>>,
    event_tx: Sender<SidecarEvent>,
    event_rx: Mutex<Option<Receiver<SidecarEvent>>>,
}

impl SidecarDaemon {
    pub fn new(server_path: PathBuf) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            server_path,
            profile_dir: Mutex::new(None),
            diagnostics_dir: Mutex::new(None),
            process: Mutex::new(None),
            request_counter: AtomicU64::new(0),
            pending: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            event_rx: Mutex::new(Some(event_rx)),
        }
    }

    pub fn take_event_receiver(&self) -> Option<Receiver<SidecarEvent>> {
        self.event_rx.lock().take()
    }

    pub fn server_path(&self) -> &PathBuf {
        &self.server_path
    }

    pub fn is_running(&self) -> bool {
        self.process.lock().is_some()
    }

    pub fn set_profile_dir(&self, path: PathBuf) {
        *self.profile_dir.lock() = Some(path);
    }

    pub fn set_diagnostics_dir(&self, path: PathBuf) {
        *self.diagnostics_dir.lock() = Some(path);
    }

    pub fn start(&self) -> Result<()> {
        if self.is_running() {
            return Ok(());
        }

        let node = resolve_node_binary();
        info!(
            node = %node.display(),
            server = %self.server_path.display(),
            "starting browser sidecar daemon"
        );

        let mut cmd = Command::new(&node);
        cmd.arg(&self.server_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(profile) = self.profile_dir.lock().as_ref() {
            cmd.env(
                "MLT_BROWSER_PROFILE_DIR",
                profile.to_string_lossy().into_owned(),
            );
        }
        if let Some(diag) = self.diagnostics_dir.lock().as_ref() {
            cmd.env(
                "MLT_BROWSER_DIAGNOSTICS_DIR",
                diag.to_string_lossy().into_owned(),
            );
        }

        let mut child = cmd
            .spawn()
            .context("failed to spawn browser sidecar daemon")?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("sidecar daemon missing stdout"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("sidecar daemon missing stdin"))?;

        let pending = self.pending.clone();
        let event_tx = self.event_tx.clone();
        let reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(raw) => {
                        let trimmed = raw.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Ok(parsed) = serde_json::from_str::<SidecarDaemonLine>(trimmed) {
                            if let Some(event) = parsed.event.as_deref() {
                                dispatch_sidecar_event(event, parsed.data.as_ref(), &event_tx);
                                continue;
                            }
                            if let Some(id) = parsed.id.clone() {
                                let mut guard = pending.lock();
                                if let Some(tx) = guard.remove(&id) {
                                    let _ = tx.send(Ok(parsed));
                                }
                            }
                        } else {
                            warn!("failed to parse sidecar daemon line");
                        }
                    }
                    Err(err) => {
                        debug!(error = %err, "sidecar daemon stdout closed");
                        break;
                    }
                }
            }
        });

        *self.process.lock() = Some(SidecarProcess {
            child,
            stdin,
            reader,
        });

        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }

        let _ = self.request("shutdown", serde_json::json!({}), Duration::from_secs(5));

        if let Some(mut proc) = self.process.lock().take() {
            let _ = proc.child.kill();
            let _ = proc.child.wait();
            let _ = proc.reader.join();
        }

        self.pending.lock().clear();
        Ok(())
    }

    pub fn request(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<SidecarDaemonLine> {
        if !self.is_running() {
            anyhow::bail!("sidecar daemon is not running");
        }

        let id = format!(
            "req-{}",
            self.request_counter.fetch_add(1, Ordering::Relaxed)
        );
        let (tx, rx) = mpsc::channel();
        self.pending.lock().insert(id.clone(), tx);

        let payload = SidecarRequest {
            id: id.clone(),
            method: method.to_string(),
            params: if params.is_null() {
                None
            } else {
                Some(params)
            },
        };

        {
            let mut guard = self.process.lock();
            let proc = guard
                .as_mut()
                .ok_or_else(|| anyhow!("sidecar daemon process missing"))?;
            let line = serde_json::to_string(&payload)?;
            proc.stdin
                .write_all(line.as_bytes())
                .context("failed to write to sidecar daemon stdin")?;
            proc.stdin
                .write_all(b"\n")
                .context("failed to write newline to sidecar daemon stdin")?;
            proc.stdin.flush().context("failed to flush sidecar stdin")?;
        }

        match rx.recv_timeout(timeout) {
            Ok(Ok(line)) => {
                if line.ok == Some(false) {
                    anyhow::bail!(
                        line.error
                            .unwrap_or_else(|| "sidecar request failed".into())
                    );
                }
                Ok(line)
            }
            Ok(Err(err)) => Err(err),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.pending.lock().remove(&id);
                anyhow::bail!("sidecar request `{method}` timed out")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("sidecar daemon response channel closed")
            }
        }
    }
}

#[derive(Serialize)]
struct SidecarRequest {
    id: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

fn dispatch_sidecar_event(
    event: &str,
    data: Option<&serde_json::Value>,
    event_tx: &Sender<SidecarEvent>,
) {
    let pid = data
        .and_then(|d| d.get("pid"))
        .and_then(|p| p.as_u64())
        .map(|p| p as u32);

    let mapped = match event {
        "ready" => Some(SidecarEvent::Ready),
        "browser_starting" => Some(SidecarEvent::BrowserStarting),
        "browser_ready" => Some(SidecarEvent::BrowserReady { pid }),
        "browser_stopped" => Some(SidecarEvent::BrowserStopped { pid }),
        "browser_disconnected" => Some(SidecarEvent::BrowserDisconnected { pid }),
        "facebook_session_changed" => Some(SidecarEvent::FacebookSessionChanged),
        "marketplace_status_changed" => Some(SidecarEvent::MarketplaceStatusChanged),
        "daemon_shutdown" => Some(SidecarEvent::DaemonShutdown),
        _ => None,
    };

    if let Some(evt) = mapped {
        let _ = event_tx.send(evt);
    }
}
