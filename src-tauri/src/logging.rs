use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static PANIC_LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub struct LogGuard {
    _worker_guard: WorkerGuard,
}

/// Records panics to stderr and the connector log file (when initialized).
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info.location().map(|loc| format!("{}:{}", loc.file(), loc.line()));
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".into()
        };

        let line = format!(
            "PANIC{}: {}",
            location
                .as_ref()
                .map(|l| format!(" at {l}"))
                .unwrap_or_default(),
            message
        );
        eprintln!("{line}");

        if let Some(path) = PANIC_LOG_PATH.get() {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{line}");
            }
        }
    }));
}

pub fn init_logging(app: &AppHandle) -> Result<LogGuard> {
    let log_dir = log_directory(app)?;
    let _ = PANIC_LOG_PATH.set(log_dir.join("connector.log"));
    std::fs::create_dir_all(&log_dir).context("failed to create log directory")?;

    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "connector.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,mlt_desktop_connector_lib=debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(std::io::stdout).with_target(false))
        .with(
            fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_target(true),
        )
        .init();

    info!(log_dir = %log_dir.display(), "structured logging initialized");
    Ok(LogGuard {
        _worker_guard: guard,
    })
}

pub fn log_directory(app: &AppHandle) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Ok(PathBuf::from(home).join("Library/Logs/MLT Desktop Connector"));
        }
    }

    app.path()
        .app_log_dir()
        .context("failed to resolve app log directory")
}
