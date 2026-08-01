mod api;
mod browser;
mod config;
mod credentials;
mod device;
mod install_location;
mod launch_session;
mod lifecycle;
mod logging;
mod marketplace;
mod protocol;
mod runtime;
mod services;
mod startup;
mod state;
mod version;

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Listener, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder,
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
    ChromiumProvisionService, ChromiumProvisionState, ConnectionTestReport, DeepLinkCoordinator,
    DeepLinkUiState, HeartbeatService, PairingCoordinator, PairingUiState, PollingService,
    UpdateUiState, UpdaterService,
};
use services::reconnect::ReconnectService;
use startup::{mark_startup_begin, startup_log, DeferredStartup};
use browser::{
    BrowserActivePage, BrowserManagerSnapshot, BrowserRuntimeService, BrowserRuntimeSnapshot,
};
use runtime::FacebookRuntime;
use runtime::DiagnosticsSnapshot;

use launch_session::{LaunchSessionService, LaunchSessionStore};
use protocol::{
    enqueue_startup_deep_links, extract_deep_link_from_argv, listen_for_deep_links,
    register_deep_links_if_supported,
};
use state::{AppState, ConnectionState};

struct PendingDeepLinks(Mutex<Vec<String>>);

struct PendingDeferredStartup(Mutex<Option<DeferredStartup>>);

struct AppServices {
    shutdown: Arc<ShutdownCoordinator>,
    state: Arc<Mutex<AppState>>,
    api_client: Arc<ConnectorApiClient>,
    heartbeat: Arc<HeartbeatService>,
    pairing: Arc<PairingCoordinator>,
    polling: Arc<PollingService>,
    browser_runtime: Arc<BrowserRuntimeService>,
    browser_manager: Arc<browser::BrowserManager>,
    facebook_runtime: Arc<FacebookRuntime>,
    deep_link: Arc<DeepLinkCoordinator>,
    chromium_provision: Arc<ChromiumProvisionService>,
    updater: Arc<UpdaterService>,
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
    services.facebook_runtime.launch_browser()
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
    services.facebook_runtime.restart_browser()
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
async fn browser_open_facebook_login(
    app: tauri::AppHandle,
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    // Repair path: provision Chromium if missing, restart sticky-failed sidecar, then open FB.
    services
        .chromium_provision
        .ensure_ready(app.clone())
        .await
        .map_err(|msg| {
            if msg.contains("CHROMIUM") || msg.contains("Chromium") || msg.contains("browser") {
                msg
            } else {
                format!("Couldn’t prepare the Facebook browser. {msg}")
            }
        })?;

    let manager = services.browser_manager.clone();
    let recover = tauri::async_runtime::spawn_blocking(move || manager.recover_runtime());
    match recover.await {
        Ok(Ok(_)) => {}
        Ok(Err(err)) if err == "CHROMIUM_NOT_INSTALLED" => {
            // Rare race: detect flipped after ensure_ready — one more provision attempt.
            services.chromium_provision.ensure_ready(app.clone()).await?;
            services.browser_manager.recover_runtime()?;
        }
        Ok(Err(err)) => return Err(err),
        Err(err) => return Err(format!("Browser recovery failed: {err}")),
    }

    let opened = services.facebook_runtime.open_facebook_login()?;
    services.heartbeat.trigger_now();
    Ok(opened)
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
fn browser_open_vehicle_create(
    services: tauri::State<'_, AppServices>,
) -> Result<BrowserManagerSnapshot, String> {
    services
        .facebook_runtime
        .marketplace
        .open_vehicle_create_route()
        .map_err(|e| e.to_string())?;
    Ok(services.browser_manager.get_status())
}

#[tauri::command]
fn runtime_cancel_operation(services: tauri::State<'_, AppServices>) -> Result<(), String> {
    services.facebook_runtime.bus.request_cancel();
    Ok(())
}

#[tauri::command]
fn runtime_diagnostics_snapshot(
    services: tauri::State<'_, AppServices>,
) -> Result<DiagnosticsSnapshot, String> {
    Ok(services.facebook_runtime.diagnostics.snapshot())
}

#[tauri::command]
fn get_job_progress(services: tauri::State<'_, AppServices>) -> Option<crate::marketplace::jobs::JobProgressSnapshot> {
    services.polling.job_progress()
}

#[tauri::command]
fn runtime_status(services: tauri::State<'_, AppServices>) -> runtime::FacebookRuntimeStatus {
    services.facebook_runtime.aggregate_status()
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
            services.facebook_runtime.as_ref(),
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
fn get_deep_link_state(services: tauri::State<'_, AppServices>) -> DeepLinkUiState {
    services.deep_link.snapshot()
}

#[tauri::command]
fn get_chromium_provision_state(
    services: tauri::State<'_, AppServices>,
) -> ChromiumProvisionState {
    services.chromium_provision.snapshot()
}

#[tauri::command]
fn get_update_state(services: tauri::State<'_, AppServices>) -> UpdateUiState {
    services.updater.snapshot()
}

#[tauri::command]
fn get_runtime_location() -> install_location::RuntimeLocation {
    install_location::RuntimeLocation::detect()
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn check_for_updates(app: tauri::AppHandle, services: tauri::State<'_, AppServices>) {
    services.updater.request_check(app);
}

#[tauri::command]
fn reopen_update_installer(services: tauri::State<'_, AppServices>) -> Result<(), String> {
    services.updater.reopen_installer()
}

/// After drag-to-Applications: launch installed app and quit this (old) process.
#[tauri::command]
fn finish_update_install(
    app: tauri::AppHandle,
    services: tauri::State<'_, AppServices>,
) -> Result<(), String> {
    services.updater.finish_and_relaunch(&app)
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
    logging::install_panic_hook();

    let config = match AppConfig::from_env() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Configuration error: {err}");
            eprintln!(
                "This build is missing embedded staging configuration. Reinstall from a valid staging package."
            );
            std::process::exit(1);
        }
    };

    let api_client = Arc::new(ConnectorApiClient::new(config.clone()));

    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            focus_main_window(app);
            if let Some(url) = extract_deep_link_from_argv(&argv) {
                if let Some(services) = app.try_state::<AppServices>() {
                    services.deep_link.enqueue(url);
                    services.deep_link.drain_pending();
                } else if let Some(pending) = app.try_state::<PendingDeepLinks>() {
                    pending.0.lock().push(url);
                }
            }
        }))
        .setup(move |app| {
            mark_startup_begin();

            let _log_guard = logging::init_logging(app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            startup_log("logging ready");

            let _cred_store = credentials::init(app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            startup_log("credential store ready");

            if let Ok(resource_dir) = app.path().resource_dir() {
                browser::set_resource_root(resource_dir);
            }

            let launch_store = LaunchSessionStore::load(
                &app
                    .path()
                    .app_data_dir()
                    .map_err(|e| e.to_string())?
                    .join("launch-sessions")
                    .join("redeemed.json"),
            );
            let launch_sessions = Arc::new(LaunchSessionService::new(
                api_client.clone(),
                launch_store,
            ));

            let device_id = device::load_or_create_device_id(app.handle())?;
            let cred_status = credentials::bootstrap_from_disk();
            let paired = cred_status == CredentialStatus::Paired;
            let needs_reconnect = cred_status == CredentialStatus::NeedsReconnect;
            let initial_error = credentials::needs_reconnect_message();

            // Never boot into Starting — that paints infinite "Setting up…" if deferred
            // init is slow. Browser/chromium work runs in the background after shell ready.
            let initial_connection = if paired && !needs_reconnect {
                ConnectionState::Idle
            } else {
                ConnectionState::Offline
            };
            let state = Arc::new(Mutex::new(AppState {
                device_id,
                environment: config.environment.clone(),
                paired,
                needs_reconnect,
                connection_state: initial_connection,
                last_heartbeat_at: None,
                last_error: initial_error.clone(),
                current_job_id: None,
                deep_link_route: None,
                deep_link_message: None,
                launch_session_id: None,
                launch_status: None,
            }));

            mark_instance_ready(&state);
            startup_log("core state ready");

            let browser_runtime = browser::init(browser::is_browser_enabled());
            let browser_manager = browser::init_manager(browser_runtime.clone());
            let facebook_runtime = FacebookRuntime::new(browser_manager.clone());

            {
                let manager_for_delegates = browser_manager.clone();
                let rt_marketplace = facebook_runtime.clone();
                let rt_session = facebook_runtime.clone();
                browser_manager.set_runtime_delegates(
                    Arc::new(move |create_vehicle| {
                        if create_vehicle {
                            rt_marketplace
                                .marketplace
                                .open_create_listing()
                                .map_err(|e| e.to_string())?;
                        } else {
                            rt_marketplace
                                .marketplace
                                .open_marketplace()
                                .map_err(|e| e.to_string())?;
                        }
                        Ok(manager_for_delegates.get_status())
                    }),
                    Arc::new(move || {
                        rt_session
                            .session
                            .check_session()
                            .map_err(|e| e.to_string())?;
                        Ok(rt_session.bus.browser_manager().get_status())
                    }),
                );
            }

            let heartbeat = HeartbeatService::spawn(
                app.handle().clone(),
                state.clone(),
                api_client.clone(),
                browser_manager.clone(),
                facebook_runtime.clone(),
            );
            let polling = PollingService::spawn(
                app.handle().clone(),
                state.clone(),
                api_client.clone(),
                facebook_runtime.clone(),
            );
            let pairing = Arc::new(PairingCoordinator::new(
                app.handle().clone(),
                state.clone(),
                api_client.clone(),
                polling.clone(),
                heartbeat.clone(),
            ));

            let updater = UpdaterService::new();

            let chromium_provision =
                Arc::new(ChromiumProvisionService::new(browser_runtime.clone()));

            let deep_link = Arc::new(DeepLinkCoordinator::new(
                app.handle().clone(),
                state.clone(),
                pairing.clone(),
                facebook_runtime.clone(),
                launch_sessions,
                heartbeat.clone(),
                updater.clone(),
                chromium_provision.clone(),
            ));

            let browser_health =
                BrowserHealthService::spawn(browser_manager.clone(), facebook_runtime.clone());

            let shutdown = Arc::new(ShutdownCoordinator::new(
                heartbeat.clone(),
                polling.clone(),
                browser_health,
                browser_manager.clone(),
            ));
            let shutdown_for_tray = shutdown.clone();
            let deep_link_for_listeners = deep_link.clone();
            spawn_sleep_resume_monitor(app.handle().clone(), state.clone(), heartbeat.clone());

            // Register managed state before creating the webview so the first IPC
            // invoke from the frontend cannot race setup or block the main thread.
            app.manage(PendingDeepLinks(Mutex::new(Vec::new())));
            app.manage(AppServices {
                shutdown,
                state: state.clone(),
                api_client: api_client.clone(),
                heartbeat: heartbeat.clone(),
                pairing,
                polling: polling.clone(),
                browser_runtime,
                browser_manager: browser_manager.clone(),
                facebook_runtime,
                deep_link: deep_link.clone(),
                chromium_provision: chromium_provision.clone(),
                updater: updater.clone(),
            });
            app.manage(PendingDeferredStartup(Mutex::new(Some(DeferredStartup {
                app: app.handle().clone(),
                state: state.clone(),
                api_client: api_client.clone(),
                browser_manager,
                heartbeat,
                polling,
                deep_link,
                chromium_provision,
                paired,
                needs_reconnect,
            }))));

            build_tray(app.handle(), shutdown_for_tray, state.clone())?;
            build_main_window(app.handle())?;
            startup_log("main window shown");

            register_deep_links_if_supported(app.handle());
            enqueue_startup_deep_links(app.handle(), &deep_link_for_listeners);
            listen_for_deep_links(app.handle(), deep_link_for_listeners);

            updater.spawn_periodic(app.handle().clone());

            if let Some(pending) = app.try_state::<PendingDeepLinks>() {
                if let Some(services) = app.try_state::<AppServices>() {
                    for url in pending.0.lock().drain(..) {
                        services.deep_link.enqueue(url);
                    }
                }
            }

            info!(
                version = version::CONNECTOR_VERSION,
                environment = %config.environment,
                device_id = %device_id,
                paired,
                "MLT Desktop Connector shell ready"
            );

            // IMPORTANT: In Tauri 2, setup() already runs as part of Ready handling.
            // Waiting for RunEvent::Ready in .run() never fires after setup, so deferred
            // init + deep-link drain never ran — blank UI / Connect never redeemed.
            // Spawn after a short yield so the webview can paint first.
            if let Some(pending) = app.try_state::<PendingDeferredStartup>() {
                if let Some(deferred) = pending.0.lock().take() {
                    startup_log("setup complete — scheduling deferred init");
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        startup_log("deferred init starting (post-setup)");
                        deferred.run().await;
                    });
                } else {
                    startup_log("setup complete — deferred init already taken");
                }
            } else {
                startup_log("setup complete — no deferred init pending");
            }

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
            browser_open_vehicle_create,
            browser_detect_facebook_session,
            runtime_cancel_operation,
            runtime_diagnostics_snapshot,
            runtime_status,
            get_job_progress,
            browser_reset_profile,
            browser_profile_status,
            run_connection_tests_cmd,
            open_log_folder,
            reconnect_device,
            get_deep_link_state,
            get_chromium_provision_state,
            get_update_state,
            get_runtime_location,
            quit_app,
            check_for_updates,
            reopen_update_installer,
            finish_update_install
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                // Keep as a safety net if Ready is ever delivered after setup in some runtime.
                RunEvent::Ready => {
                    if let Some(pending) = app_handle.try_state::<PendingDeferredStartup>() {
                        if let Some(deferred) = pending.0.lock().take() {
                            startup_log("RunEvent::Ready — starting deferred init (fallback)");
                            deferred.spawn();
                        }
                    }
                }
                RunEvent::ExitRequested { api, .. } => {
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
                _ => {}
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
