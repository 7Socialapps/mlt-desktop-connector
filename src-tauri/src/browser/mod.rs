mod manager;
mod runtime;
mod sidecar;
mod types;

pub use manager::BrowserManager;
pub use runtime::BrowserRuntimeService;
pub use types::{
    BrowserActivePage, BrowserManagerSnapshot, BrowserRuntimeSnapshot,
};

use std::sync::{Arc, OnceLock};

static RUNTIME: OnceLock<Arc<BrowserRuntimeService>> = OnceLock::new();
static MANAGER: OnceLock<Arc<BrowserManager>> = OnceLock::new();

pub fn init(enabled: bool) -> Arc<BrowserRuntimeService> {
    RUNTIME
        .get_or_init(|| Arc::new(BrowserRuntimeService::new(enabled)))
        .clone()
}

pub fn init_manager(runtime: Arc<BrowserRuntimeService>) -> Arc<BrowserManager> {
    MANAGER
        .get_or_init(|| {
            let server_path = sidecar::resolve_sidecar_server().unwrap_or_else(|err| {
                tracing::warn!(error = %err, "browser sidecar server unavailable at startup");
                std::path::PathBuf::new()
            });
            let daemon = Arc::new(SidecarDaemon::new(server_path));
            Arc::new(BrowserManager::new(runtime.clone(), daemon))
        })
        .clone()
}

pub fn manager() -> Option<Arc<BrowserManager>> {
    MANAGER.get().cloned()
}

pub fn service() -> Option<Arc<BrowserRuntimeService>> {
    RUNTIME.get().cloned()
}

pub fn is_browser_enabled() -> bool {
    std::env::var("MLT_BROWSER_ENABLED")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}

use sidecar::SidecarDaemon;
