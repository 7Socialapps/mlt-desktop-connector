use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::state::AppState;

pub fn mark_instance_ready(_state: &Arc<Mutex<AppState>>) {
    info!("single-instance guard active");
}

pub fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
