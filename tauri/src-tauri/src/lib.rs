mod commands;
mod credentials;
mod logging;
mod models;
mod paths;
mod rclone;
mod settings;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test environment lock poisoned")
    }

    pub(crate) fn set_test_dirs(app_data_dir: &std::path::Path, log_dir: &std::path::Path) {
        std::env::set_var("CLOUD_DRIVE_MOUNT_APP_DATA_DIR", app_data_dir);
        std::env::set_var("CLOUD_DRIVE_MOUNT_LOG_DIR", log_dir);
    }

    pub(crate) fn clear_test_dirs() {
        std::env::remove_var("CLOUD_DRIVE_MOUNT_APP_DATA_DIR");
        std::env::remove_var("CLOUD_DRIVE_MOUNT_LOG_DIR");
    }
}

use std::sync::{Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent,
};
use tauri_plugin_autostart::MacosLauncher;

use commands::{
    attempt_auto_mount, browse_folder, clear_logs, configure_google_drive_cmd,
    disconnect_google_drive_cmd, emit_mount_state, forget_seedbox_cmd, get_platform,
    is_fuse_installed_cmd, is_google_drive_configured_cmd, is_mounted, is_seedbox_configured_cmd,
    load_settings_cmd, mount_all, open_log_folder, restart_app, save_b2_credentials_cmd,
    save_settings_cmd, setup_window_events, show_settings_window, test_google_drive_connection_cmd,
    test_seedbox_connection_cmd, unmount_all, AppState,
};
use logging::LogEmitter;
use rclone::RcloneManager;
use settings::load_settings;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let logger = Arc::new(Mutex::new(LogEmitter::new(None)));
    let rclone = Arc::new(RcloneManager::new(logger.clone()));
    let state = AppState {
        rclone: rclone.clone(),
        logger: logger.clone(),
    };

    let show_on_launch =
        std::env::args().any(|a| a == "--show-settings") || !load_settings().start_minimized;

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

            let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Cloud Drive Mount")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings_window(app),
                    "quit" => app.exit(0),
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
                if !show_on_launch {
                    let _ = window.hide();
                }
            }

            if let Some(state) = app.try_state::<AppState>() {
                emit_mount_state(app.handle(), state.inner());

                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        state.rclone.cleanup_stale_processes(&app_handle);

                        if is_fuse_installed_cmd() {
                            attempt_auto_mount(&app_handle, state.inner());
                        } else if let Ok(log) = state.logger.lock() {
                            log.info("FUSE provider not detected on launch.");
                        }

                        emit_mount_state(&app_handle, state.inner());
                    }
                });
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
            is_google_drive_configured_cmd,
            configure_google_drive_cmd,
            disconnect_google_drive_cmd,
            test_google_drive_connection_cmd,
            is_seedbox_configured_cmd,
            test_seedbox_connection_cmd,
            forget_seedbox_cmd,
            open_log_folder,
            clear_logs,
            browse_folder,
            restart_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(move |app, event| match event {
            RunEvent::Ready if show_on_launch => {
                show_settings_window(app);
            }
            RunEvent::ExitRequested { .. } => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.rclone.unmount_all(app);
                }
            }
            _ => {}
        });
}
