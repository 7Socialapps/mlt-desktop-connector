mod runtime;
mod sidecar;
mod types;

pub use runtime::BrowserRuntimeService;
pub use types::{BrowserRuntimeSnapshot, BrowserRuntimeStatus};

use std::sync::{Arc, OnceLock};

static RUNTIME: OnceLock<Arc<BrowserRuntimeService>> = OnceLock::new();

pub fn init(enabled: bool) -> Arc<BrowserRuntimeService> {
    RUNTIME
        .get_or_init(|| Arc::new(BrowserRuntimeService::new(enabled)))
        .clone()
}

pub fn service() -> Option<Arc<BrowserRuntimeService>> {
    RUNTIME.get().cloned()
}

pub fn is_browser_enabled() -> bool {
    std::env::var("MLT_BROWSER_ENABLED")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}
