mod commands;
mod credentials;
mod logging;
mod models;
mod paths;
mod rclone;
mod settings;

use std::sync::{Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent,
};
use tauri_plugin_autostart::MacosLauncher;

use commands::{
    attempt_auto_mount, browse_folder, clear_logs, emit_mount_state, get_platform,
    is_fuse_installed_cmd, is_mounted, load_settings_cmd, mount_all, open_log_folder,
    restart_app, save_b2_credentials_cmd, save_settings_cmd, setup_window_events,
    show_settings_window, unmount_all, AppState,
};
use logging::LogEmitter;
use rclone::RcloneManager;
use settings::load_settings;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let logger = Arc::new(Mutex::new(LogEmitter::new(None)));
    let rclone = RcloneManager::new(logger.clone());
    let state = AppState {
        rclone: Mutex::new(rclone),
        logger: logger.clone(),
    };

    let show_on_launch = std::env::args().any(|a| a == "--show-settings")
        || !load_settings().start_minimized;

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--show-settings"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            if let Ok(mut log) = logger.lock() {
                log.set_app(app.handle().clone());
                log.info("Cloud Drive Mount starting.");
            }

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings_item =
                MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Cloud Drive Mount")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings_window(app),
                    "quit" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            if let Ok(manager) = state.rclone.lock() {
                                manager.unmount_all(app);
                            }
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } = event
                    {
                        show_settings_window(tray.app_handle());
                    }
                })
                .build(app)?;

            setup_window_events(app.handle());

            if let Some(window) = app.get_webview_window("main") {
                if show_on_launch {
                    let _ = window.show();
                    let _ = window.set_focus();
                } else {
                    let _ = window.hide();
                }
            }

            if let Some(state) = app.try_state::<AppState>() {
                state
                    .rclone
                    .lock()
                    .map_err(|e| e.to_string())?
                    .cleanup_stale_processes(app.handle());

                if is_fuse_installed_cmd() {
                    attempt_auto_mount(app.handle(), state.inner());
                } else if let Ok(log) = logger.lock() {
                    log.info("FUSE provider not detected on launch.");
                }

                emit_mount_state(app.handle(), state.inner());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_platform,
            load_settings_cmd,
            save_settings_cmd,
            save_b2_credentials_cmd,
            mount_all,
            unmount_all,
            is_mounted,
            is_fuse_installed_cmd,
            open_log_folder,
            clear_logs,
            browse_folder,
            restart_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(manager) = state.rclone.lock() {
                        manager.unmount_all(app);
                    }
                }
            }
        });
}
