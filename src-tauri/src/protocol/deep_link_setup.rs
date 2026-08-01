use std::sync::Arc;

use tauri::{AppHandle, Listener};
use tracing::warn;

use super::extract_deep_link_from_argv;
use crate::services::DeepLinkCoordinator;

/// Windows/Linux support runtime scheme registration; macOS registers schemes in Info.plist at build time.
pub fn should_register_deep_links_at_runtime() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux"))
}

pub fn register_deep_links_if_supported(app: &AppHandle) {
    if !should_register_deep_links_at_runtime() {
        return;
    }

    use tauri_plugin_deep_link::DeepLinkExt;
    if let Err(err) = app.deep_link().register_all() {
        warn!(
            error = %err,
            "deep link runtime registration failed (installer may still register schemes)"
        );
    }
}

pub fn enqueue_startup_deep_links(app: &AppHandle, deep_link: &Arc<DeepLinkCoordinator>) {
    if let Some(url) = extract_deep_link_from_argv(&std::env::args().collect::<Vec<_>>()) {
        deep_link.enqueue(url);
    }

    use tauri_plugin_deep_link::DeepLinkExt;
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            deep_link.enqueue(url.to_string());
        }
    }
}

pub fn listen_for_deep_links(app: &AppHandle, deep_link: Arc<DeepLinkCoordinator>) {
    let deep_link_listener = deep_link;
    let app_handle = app.clone();
    app.listen("deep-link://new-url", move |event| {
        if let Ok(urls) = serde_json::from_str::<Vec<String>>(event.payload()) {
            for url in urls {
                deep_link_listener.enqueue(url);
            }
            deep_link_listener.drain_pending();
        } else if let Some(url) = event
            .payload()
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
        {
            deep_link_listener.enqueue(url.to_string());
            deep_link_listener.drain_pending();
        }
        let _ = app_handle;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_does_not_register_deep_links_at_runtime() {
        if cfg!(target_os = "macos") {
            assert!(!should_register_deep_links_at_runtime());
        }
    }

    #[test]
    fn windows_or_linux_may_register_at_runtime() {
        if cfg!(any(target_os = "windows", target_os = "linux")) {
            assert!(should_register_deep_links_at_runtime());
        }
    }
}
