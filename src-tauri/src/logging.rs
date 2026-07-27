use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct LogGuard {
    _worker_guard: WorkerGuard,
}

pub fn init_logging(app: &AppHandle) -> Result<LogGuard> {
    let log_dir = log_directory(app)?;
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
    let dir = app
        .path()
        .app_log_dir()
        .context("failed to resolve app log directory")?;
    Ok(dir)
}
