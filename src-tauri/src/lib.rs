mod api;
mod browser;
mod config;
mod credentials;
mod device;
mod lifecycle;
mod logging;
mod marketplace;
mod services;
mod state;
mod version;

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder,
};
use tracing::info;

use api::ConnectorApiClient;
use config::AppConfig;
use crate::credentials::CredentialStatus;
use lifecycle::{
    focus_main_window, is_shutting_down, mark_instance_ready, spawn_sleep_resume_monitor,
    ShutdownCoordinator,
};
use services::{
    enable_polling_if_authenticated, run_connection_tests, BrowserHealthService,
    ConnectionTestReport, HeartbeatService, PairingCoordinator, PairingUiState, PollingService,
};
use services::reconnect::ReconnectService;
use browser::{
    BrowserActivePage, BrowserManagerSnapshot, BrowserRuntimeService, BrowserRuntimeSnapshot,
};

use state::{AppState, ConnectionState};

struct AppServices {
    shutdown: Arc<ShutdownCoordinator>,
    state: Arc<Mutex<AppState>>,
    api_client: Arc<ConnectorApiClient>,
    heartbeat: Arc<HeartbeatService>,
    pairing: Arc<PairingCoordinator>,
    polling: Arc<PollingService>,
    browser_runtime: Arc<BrowserRuntimeService>,
    browser_manager: Arc<browser::BrowserManager>,
}

#[tauri::command]
fn get_status(services: tauri::State<'_, AppServices>) -> state::StatusSnapshot {
    services.state.lock().status_snapshot()
}

#[tauri::command]
fn get_connector_version() -> &'static str {
    version::CONNECTOR_VERSION
}

#[tauri::command]
fn get_pairing_state(services: tauri::State<'_, AppServices>) -> PairingUiState {
    services.pairing.snapshot()
}

#[tauri::command]
async fn start_pairing_session(
    services: tauri::State<'_, AppServices>,
    device_name: Option<String>,
) -> Result<PairingUiState, String> {
    let device_id = services.state.lock().device_id.to_string();
    tracing::info!(device_id = %device_id, device_name = ?device_name, "start_pairing_session command invoked");
    services.pairing.start(device_id, device_name).await
}

#[tauri::command]
fn get_browser_runtime_status(services: tauri::State<'_, AppServices>) -> BrowserRuntimeSnapshot {
    services.browser_runtime.snapshot()
}

#[tauri::command]
fn detect_browser_runtime(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserRuntimeSnapshot, String> {
    services.browser_runtime.detect()
}

#[tauri::command]
fn browser_test_launch(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserRuntimeSnapshot, String> {
    #[cfg(not(debug_assertions))]
    {
        return Err("Browser test commands are not available in production builds".into());
    }
    services.browser_runtime.test_launch()
}

#[tauri::command]
fn browser_test_close(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserRuntimeSnapshot, String> {
    #[cfg(not(debug_assertions))]
    {
        return Err("Browser test commands are not available in production builds".into());
    }
    services.browser_runtime.test_close()
}

#[tauri::command]
fn get_browser_status(services: tauri::State<'_, AppServices>) -> BrowserManagerSnapshot {
    services.browser_manager.get_status()
}

#[tauri::command]
fn browser_launch(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.launch()
}

#[tauri::command]
fn browser_stop(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.stop()
}

#[tauri::command]
fn browser_restart(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.restart()
}

#[tauri::command]
fn browser_health_check(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.health_check()
}

#[tauri::command]
fn browser_get_active_page(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserActivePage, String> {
    services.browser_manager.get_active_page()
}

#[tauri::command]
fn browser_open_marketplace(
    services: tauri::State<'_, AppServices>,
    create_vehicle: Option<bool>,
) -> Result<BrowserManagerSnapshot, String> {
    services
        .browser_manager
        .open_marketplace(create_vehicle.unwrap_or(false))
}

#[tauri::command]
fn browser_open_facebook_login(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.open_facebook_login()
}

#[tauri::command]
fn browser_detect_facebook_session(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.detect_facebook_session()
}

#[tauri::command]
fn browser_reset_profile(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.reset_profile()
}

#[tauri::command]
fn browser_profile_status(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services.browser_manager.profile_status()
}

#[tauri::command]
async fn run_connection_tests_cmd(
    services: tauri::State<'_, AppServices>,
) -> Result<ConnectionTestReport, String> {
    Ok(
        run_connection_tests(
            services.api_client.as_ref(),
            &services.state,
            services.browser_manager.as_ref(),
            services.browser_runtime.as_ref(),
        )
        .await,
    )
}

#[tauri::command]
fn open_log_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = logging::log_directory(&app).map_err(|e| e.to_string())?;
    open_path_in_file_manager(&dir)
}

#[tauri::command]
async fn reconnect_device(services: tauri::State<'_, AppServices>) -> Result<state::StatusSnapshot, String> {
    let refreshed = ReconnectService::try_refresh_tokens(services.api_client.as_ref()).await;
    services.heartbeat.trigger_now();
    if refreshed {
        let mut guard = services.state.lock();
        guard.needs_reconnect = false;
        guard.last_error = None;
        if guard.paired {
            guard.connection_state = ConnectionState::Idle;
        }
    } else {
        let mut guard = services.state.lock();
        guard.needs_reconnect = true;
        guard.connection_state = ConnectionState::Offline;
        if guard.last_error.is_none() {
            guard.last_error = Some(
                "Reconnect failed — start pairing again from the dashboard.".into(),
            );
        }
    }
    Ok(services.state.lock().status_snapshot())
}

fn open_path_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to open log folder: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to open log folder: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to open log folder: {e}"))?;
    }
    Ok(())
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
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

            let _cred_store = credentials::init(app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

            let device_id = device::load_or_create_device_id(app.handle())?;
            let cred_status = credentials::bootstrap_from_disk();
            let paired = cred_status == CredentialStatus::Paired;
            let needs_reconnect = cred_status == CredentialStatus::NeedsReconnect;
            let initial_error = credentials::needs_reconnect_message();

            let state = Arc::new(Mutex::new(AppState {
                device_id,
                environment: config.environment.clone(),
                paired,
                needs_reconnect,
                connection_state: ConnectionState::Starting,
                last_heartbeat_at: None,
                last_error: initial_error.clone(),
                current_job_id: None,
            }));

            mark_instance_ready(&state);

            let browser_runtime = browser::init(browser::is_browser_enabled());
            let browser_manager = browser::init_manager(browser_runtime.clone());

            let heartbeat = HeartbeatService::spawn(
                app.handle().clone(),
                state.clone(),
                api_client.clone(),
                browser_manager.clone(),
            );
            let polling = PollingService::spawn(
                app.handle().clone(),
                state.clone(),
                api_client.clone(),
            );
            let pairing = Arc::new(PairingCoordinator::new(
                app.handle().clone(),
                state.clone(),
                api_client.clone(),
                polling.clone(),
            ));

            {
                let mut guard = state.lock();
                if paired {
                    enable_polling_if_authenticated(polling.as_ref(), &mut guard);
                    guard.connection_state = ConnectionState::Idle;
                } else if needs_reconnect {
                    guard.connection_state = ConnectionState::Offline;
                } else {
                    guard.connection_state = ConnectionState::Offline;
                }
            }

            let restore_client = api_client.clone();
            let restore_state = state.clone();
            let restore_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if !credentials::is_paired() {
                    return;
                }
                if credentials::has_access_token() {
                    return;
                }
                match credentials::ensure_access_token(&restore_client).await {
                    Ok(true) => {
                        let mut guard = restore_state.lock();
                        guard.paired = true;
                        guard.needs_reconnect = false;
                        guard.last_error = None;
                        guard.connection_state = ConnectionState::Idle;
                        drop(guard);
                        let _ = restore_app.emit(
                            "connector://status-changed",
                            restore_state.lock().status_snapshot(),
                        );
                    }
                    Ok(false) => {
                        let mut guard = restore_state.lock();
                        guard.paired = false;
                        guard.needs_reconnect = true;
                        if guard.last_error.is_none() {
                            guard.last_error = credentials::needs_reconnect_message();
                        }
                        guard.connection_state = ConnectionState::Offline;
                        drop(guard);
                        let _ = restore_app.emit(
                            "connector://status-changed",
                            restore_state.lock().status_snapshot(),
                        );
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "credential bootstrap failed");
                        credentials::mark_needs_reconnect(
                            "Reconnect device — stored credentials are unavailable. Start pairing again.",
                        );
                        let mut guard = restore_state.lock();
                        guard.paired = false;
                        guard.needs_reconnect = true;
                        guard.last_error = credentials::needs_reconnect_message();
                        guard.connection_state = ConnectionState::Offline;
                    }
                }
            });

            let browser_init = browser_manager.clone();
            let browser_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = browser_init.initialize(browser_app.clone()) {
                    tracing::warn!(error = %err, "browser manager initialization failed");
                }
                let _ = browser_app.emit(
                    "connector://browser-changed",
                    browser_init.get_status(),
                );
            });

            let browser_health =
                BrowserHealthService::spawn(browser_manager.clone());

            let shutdown = Arc::new(ShutdownCoordinator::new(
                heartbeat.clone(),
                polling.clone(),
                browser_health,
                browser_manager.clone(),
            ));
            spawn_sleep_resume_monitor(app.handle().clone(), state.clone(), heartbeat.clone());

            build_tray(app.handle(), shutdown.clone(), state.clone())?;
            build_main_window(app.handle())?;

            app.manage(AppServices {
                shutdown,
                state: state.clone(),
                api_client: api_client.clone(),
                heartbeat,
                pairing,
                polling,
                browser_runtime,
                browser_manager,
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
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_connector_version,
            get_pairing_state,
            start_pairing_session,
            get_browser_runtime_status,
            detect_browser_runtime,
            browser_test_launch,
            browser_test_close,
            get_browser_status,
            browser_launch,
            browser_stop,
            browser_restart,
            browser_health_check,
            browser_get_active_page,
            browser_open_facebook_login,
            browser_open_marketplace,
            browser_detect_facebook_session,
            browser_reset_profile,
            browser_profile_status,
            run_connection_tests_cmd,
            open_log_folder,
            reconnect_device
        ])
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
        .inner_size(440.0, 720.0)
        .resizable(true)
        .build()?;

    Ok(())
}
