mod api;
mod config;
mod credentials;
mod device;
mod lifecycle;
mod logging;
mod services;
mod state;
mod version;

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent, WebviewUrl, WebviewWindowBuilder,
};
use tracing::info;

use api::ConnectorApiClient;
use config::AppConfig;
use credentials::has_access_token;
use lifecycle::{
    focus_main_window, is_shutting_down, mark_instance_ready, spawn_sleep_resume_monitor,
    ShutdownCoordinator,
};
use services::{
    enable_polling_if_authenticated, HeartbeatService, PollingService,
};
use state::{AppState, ConnectionState};

struct AppServices {
    shutdown: Arc<ShutdownCoordinator>,
    state: Arc<Mutex<AppState>>,
}

#[tauri::command]
fn get_status(services: tauri::State<'_, AppServices>) -> state::StatusSnapshot {
    services.state.lock().status_snapshot()
}

#[tauri::command]
fn get_connector_version() -> &'static str {
    version::CONNECTOR_VERSION
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = match AppConfig::from_env() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Configuration error: {err}");
            eprintln!("Set MLT_ENV=staging and staging Supabase URL/anon key before launching.");
            std::process::exit(1);
        }
    };

    let api_client = Arc::new(ConnectorApiClient::new(config.clone()));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app);
        }))
        .setup(move |app| {
            let _log_guard = logging::init_logging(app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

            let device_id = device::load_or_create_device_id(app.handle())?;
            let paired = has_access_token();

            let state = Arc::new(Mutex::new(AppState {
                device_id,
                environment: config.environment.clone(),
                paired,
                connection_state: ConnectionState::Starting,
                last_heartbeat_at: None,
                last_error: None,
            }));

            mark_instance_ready(&state);

            let heartbeat =
                HeartbeatService::spawn(app.handle().clone(), state.clone(), api_client.clone());
            let polling = PollingService::spawn(app.handle().clone(), state.clone());

            {
                let mut guard = state.lock();
                enable_polling_if_authenticated(polling.as_ref(), &mut guard);
                guard.connection_state = if paired {
                    ConnectionState::Idle
                } else {
                    ConnectionState::Offline
                };
            }

            let shutdown = Arc::new(ShutdownCoordinator::new(heartbeat.clone(), polling.clone()));
            spawn_sleep_resume_monitor(app.handle().clone(), state.clone(), heartbeat);

            build_tray(app.handle(), shutdown.clone(), state.clone())?;
            build_main_window(app.handle())?;

            app.manage(AppServices {
                shutdown,
                state: state.clone(),
            });

            info!(
                version = version::CONNECTOR_VERSION,
                environment = %config.environment,
                device_id = %device_id,
                paired,
                "MLT Desktop Connector started"
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_status, get_connector_version])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                if is_shutting_down() {
                    return;
                }
                api.prevent_exit();
                if let Some(services) = app_handle.try_state::<AppServices>() {
                    services.shutdown.graceful_shutdown(&services.state);
                }
                info!("exit requested — services drained");
                app_handle.exit(0);
            }
        });
}

fn build_tray(
    app: &tauri::AppHandle,
    shutdown: Arc<ShutdownCoordinator>,
    state: Arc<Mutex<AppState>>,
) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show status", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let shutdown_for_menu = shutdown.clone();
    let state_for_menu = state.clone();

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("MLT Desktop Connector")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                focus_main_window(app);
            }
            "quit" => {
                shutdown_for_menu.graceful_shutdown(&state_for_menu);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn build_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("main").is_some() {
        return Ok(());
    }

    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("MLT Desktop Connector")
        .inner_size(440.0, 360.0)
        .resizable(true)
        .build()?;

    Ok(())
}
