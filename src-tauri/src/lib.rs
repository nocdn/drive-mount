mod commands;
mod credentials;
mod logging;
mod models;
mod notifications;
mod paths;
mod rclone;
mod settings;

#[cfg(target_os = "macos")]
mod macos_menu_bar_icon;

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
    attempt_auto_mount_cmd, clear_logs, configure_google_drive_cmd, disconnect_google_drive_cmd,
    emit_mount_state, forget_seedbox_cmd, get_platform, is_fuse_installed_cmd,
    is_google_drive_configured_cmd, is_mounted, is_seedbox_configured_cmd, load_credentials_cmd,
    load_settings_cmd, mount_all, open_log_folder, open_mount_target, restart_mounts,
    save_b2_credentials_cmd, save_settings_cmd, setup_window_events, show_settings_window,
    test_google_drive_connection_cmd, test_seedbox_connection_cmd, unmount_all,
    used_windows_drive_letters_cmd, AppState,
};
use logging::LogEmitter;
use notifications::show_app_notification;
use rclone::RcloneManager;
use settings::load_settings;

const ARG_AUTOSTART: &str = "--autostart";
const ARG_CLEAN_RESTART: &str = "--clean-restart";
const ARG_SHOW_SETTINGS: &str = "--show-settings";

fn should_show_on_launch(launch_args: &[String], start_minimized: bool) -> bool {
    launch_args.iter().any(|arg| arg == ARG_SHOW_SETTINGS) || !start_minimized
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let logger = Arc::new(Mutex::new(LogEmitter::new(None)));
    let rclone = Arc::new(RcloneManager::new(logger.clone()));
    let state = AppState {
        rclone: rclone.clone(),
        logger: logger.clone(),
        auto_mount_attempted: Arc::new(Mutex::new(false)),
    };

    let launch_args: Vec<String> = std::env::args().skip(1).collect();
    let clean_restart = launch_args.iter().any(|a| a == ARG_CLEAN_RESTART);
    let show_on_launch = should_show_on_launch(&launch_args, load_settings().start_minimized);

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![ARG_AUTOSTART]),
        ))
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            if let Ok(mut log) = logger.lock() {
                log.set_app(app.handle().clone());
                if clean_restart {
                    if let Err(err) = log.clear() {
                        eprintln!("Could not clear logs during clean restart: {err}");
                    }
                }
                log.info("Cloud Drive Mount starting.");
            }

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::new()
                .tooltip("Cloud Drive Mount")
                .menu(&menu)
                .show_menu_on_left_click(true);

            #[cfg(target_os = "macos")]
            {
                tray_builder = tray_builder
                    .icon(macos_menu_bar_icon::load())
                    .icon_as_template(true);
            }

            #[cfg(not(target_os = "macos"))]
            {
                tray_builder = tray_builder.icon(app.default_window_icon().unwrap().clone());
            }

            let quit_item_for_menu = quit_item.clone();
            let _tray = tray_builder
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "settings" => show_settings_window(app),
                    "quit" => {
                        let _ = quit_item_for_menu.set_text("Quitting...");
                        let _ = quit_item_for_menu.set_enabled(false);
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
                        state.rclone.refresh_configured_mount_targets();

                        if !is_fuse_installed_cmd() {
                            if let Ok(log) = state.logger.lock() {
                                log.info("FUSE provider not detected on launch.");
                            }
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
            load_credentials_cmd,
            attempt_auto_mount_cmd,
            save_settings_cmd,
            save_b2_credentials_cmd,
            mount_all,
            unmount_all,
            is_mounted,
            is_fuse_installed_cmd,
            used_windows_drive_letters_cmd,
            is_google_drive_configured_cmd,
            configure_google_drive_cmd,
            disconnect_google_drive_cmd,
            test_google_drive_connection_cmd,
            is_seedbox_configured_cmd,
            test_seedbox_connection_cmd,
            forget_seedbox_cmd,
            open_log_folder,
            open_mount_target,
            clear_logs,
            restart_mounts,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(move |app, event| match event {
            RunEvent::Ready if show_on_launch => {
                show_settings_window(app);
            }
            RunEvent::ExitRequested { .. } => {
                if let Some(state) = app.try_state::<AppState>() {
                    show_app_notification(
                        app,
                        "Unmounting active drives before quitting. Please wait.",
                    );
                    state.rclone.unmount_all(app);
                    show_app_notification(
                        app,
                        "Unmount complete. Cloud Drive Mount has finished shutting down.",
                    );
                }
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn launch_visibility_respects_explicit_show_and_start_minimized() {
        assert!(!should_show_on_launch(&args(&[]), true));
        assert!(should_show_on_launch(&args(&[]), false));
        assert!(should_show_on_launch(&args(&["--show-settings"]), true));
        assert!(!should_show_on_launch(&args(&["--autostart"]), true));
        assert!(should_show_on_launch(&args(&["--autostart"]), false));
        assert!(should_show_on_launch(
            &args(&["--show-settings", "--clean-restart"]),
            true,
        ));
    }

    #[test]
    fn macos_menu_bar_icon_uses_retina_template_asset() {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(macos_menu_bar_icon::MENU_BAR_ICON_PIXEL_SIZE, 36);
            assert_eq!(
                macos_menu_bar_icon::MENU_BAR_ICON_ASSET,
                "src-tauri/icons/menu-bar-template.png"
            );
        }
    }
}
