use std::process::Command;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::credentials::{load_b2_credentials, save_b2_credentials};
use crate::logging::LogEmitter;
use crate::models::{
    AppSettings, B2Credentials, LoadedSettings, MountRequest, MountState,
};
use crate::paths::{log_dir, platform_name};
use crate::rclone::{has_complete_b2_config, is_fuse_installed, RcloneManager};
use crate::settings::{load_settings, save_settings};

pub struct AppState {
    pub rclone: Mutex<RcloneManager>,
    pub logger: Arc<Mutex<LogEmitter>>,
}

#[tauri::command]
pub fn get_platform() -> String {
    platform_name().to_string()
}

#[tauri::command]
pub fn load_settings_cmd(state: State<'_, AppState>) -> Result<LoadedSettings, String> {
    let settings = load_settings();
    let saved = load_b2_credentials()?;
    let has_saved_credentials = saved.is_some();

    let (application_key_id, application_key) = saved
        .map(|c| (c.application_key_id, c.application_key))
        .unwrap_or_default();

    if let Ok(logger) = state.logger.lock() {
        logger.info("Settings loaded.");
    }

    Ok(LoadedSettings {
        settings,
        has_saved_credentials,
        application_key_id,
        application_key,
    })
}

#[tauri::command]
pub fn save_settings_cmd(settings: AppSettings) -> Result<(), String> {
    save_settings(&settings)
}

#[tauri::command]
pub fn save_b2_credentials_cmd(credentials: B2Credentials) -> Result<(), String> {
    save_b2_credentials(&credentials)
}

#[tauri::command]
pub fn mount_all(
    app: AppHandle,
    state: State<'_, AppState>,
    request: MountRequest,
) -> Result<(), String> {
    if !request.application_key_id.trim().is_empty() || !request.application_key.trim().is_empty() {
        save_b2_credentials(&B2Credentials {
            application_key_id: request.application_key_id.clone(),
            application_key: request.application_key.clone(),
        })?;
    }

    let settings = AppSettings {
        buckets: request.buckets.clone(),
        ..load_settings()
    };
    save_settings(&settings)?;

    state
        .rclone
        .lock()
        .map_err(|e| e.to_string())?
        .mount_b2(&app, &request)
}

#[tauri::command]
pub fn unmount_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state
        .rclone
        .lock()
        .map_err(|e| e.to_string())?
        .unmount_all(&app);
    Ok(())
}

#[tauri::command]
pub fn is_mounted(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state
        .rclone
        .lock()
        .map_err(|e| e.to_string())?
        .is_mounted())
}

#[tauri::command]
pub fn is_fuse_installed_cmd() -> bool {
    is_fuse_installed()
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> Result<(), String> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) -> Result<(), String> {
    state
        .logger
        .lock()
        .map_err(|e| e.to_string())?
        .clear()
}

#[tauri::command]
pub async fn browse_folder(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let path = app
            .dialog()
            .file()
            .set_title("Choose mount folder")
            .blocking_pick_folder();
        Ok(path.map(|p| p.to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
pub fn restart_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state
        .rclone
        .lock()
        .map_err(|e| e.to_string())?
        .unmount_all(&app);

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("--show-settings");
    cmd.spawn().map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

pub fn attempt_auto_mount(app: &AppHandle, state: &AppState) {
    let settings = load_settings();
    let creds = load_b2_credentials().ok().flatten();
    if !is_fuse_installed() {
        if let Ok(logger) = state.logger.lock() {
            logger.info("FUSE provider not detected; skipping auto-mount.");
        }
        return;
    }
    if !has_complete_b2_config(&creds, &settings.buckets) {
        return;
    }

    let Some(creds) = creds else {
        return;
    };

    if let Ok(logger) = state.logger.lock() {
        logger.info("Attempting auto-mount from saved settings.");
    }

    let request = MountRequest {
        application_key_id: creds.application_key_id,
        application_key: creds.application_key,
        buckets: settings.buckets,
    };

    if let Ok(manager) = state.rclone.lock() {
        let _ = manager.mount_b2(app, &request);
    }
}

pub fn show_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn setup_window_events(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let window_clone = window.clone();
        window.on_window_event(move |event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window_clone.hide();
                }
                tauri::WindowEvent::Resized(_) => {
                    if window_clone.is_minimized().unwrap_or(false) {
                        let _ = window_clone.hide();
                        let _ = window_clone.unminimize();
                    }
                }
                _ => {}
            }
        });
    }
}

pub fn emit_mount_state(app: &AppHandle, state: &AppState) {
    let mounted = state
        .rclone
        .lock()
        .map(|m| m.is_mounted())
        .unwrap_or(false);
    let _ = app.emit("mount-state-changed", MountState { mounted });
}
