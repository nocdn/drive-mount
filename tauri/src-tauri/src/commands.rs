use std::process::Command;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::credentials::{
    delete_seedbox_password, has_saved_seedbox_password, load_b2_credentials, load_seedbox_password,
    save_b2_credentials, save_seedbox_password,
};
use crate::logging::LogEmitter;
use crate::models::{
    AppSettings, B2Credentials, GoogleDriveSettings, LoadedSettings, MountRequest, MountState,
    SeedboxSettings,
};
use crate::paths::{log_dir, platform_name};
use crate::rclone::{
    has_complete_b2_config, has_complete_google_drive_config, has_complete_seedbox_config,
    is_fuse_installed, is_google_drive_configured, is_seedbox_configured, RcloneManager,
};
use crate::settings::{load_settings, save_settings};

pub struct AppState {
    pub rclone: Arc<RcloneManager>,
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
        is_google_drive_configured: is_google_drive_configured(),
        is_seedbox_configured: is_seedbox_configured(),
        has_saved_seedbox_password: has_saved_seedbox_password().unwrap_or(false),
    })
}

#[tauri::command]
pub fn save_settings_cmd(mut settings: AppSettings) -> Result<(), String> {
    settings.google_drive = settings.google_drive.normalized();
    settings.seedbox = settings.seedbox.normalized();
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
        google_drive: request.google_drive.clone(),
        seedbox: request.seedbox.clone(),
        selected_provider: request.selected_provider,
        ..load_settings()
    };
    save_settings(&settings)?;

    state.rclone.mount_all(&app, &request)
}

#[tauri::command]
pub fn is_google_drive_configured_cmd(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.rclone.is_google_drive_configured())
}

#[tauri::command]
pub fn configure_google_drive_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    google_drive: GoogleDriveSettings,
) -> Result<(), String> {
    let settings = AppSettings {
        google_drive: google_drive.clone(),
        ..load_settings()
    };
    save_settings(&settings)?;

    state.rclone.configure_google_drive(&app, &google_drive)
}

#[tauri::command]
pub fn disconnect_google_drive_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    google_drive: GoogleDriveSettings,
) -> Result<(), String> {
    state.rclone.disconnect_google_drive(&app, &google_drive)
}

#[tauri::command]
pub fn test_google_drive_connection_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    google_drive: GoogleDriveSettings,
) -> Result<(), String> {
    state.rclone.test_google_drive_connection(&app, &google_drive)
}

#[tauri::command]
pub fn is_seedbox_configured_cmd(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.rclone.is_seedbox_configured())
}

#[tauri::command]
pub fn test_seedbox_connection_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    seedbox: SeedboxSettings,
    password: String,
) -> Result<(), String> {
    let settings = AppSettings {
        seedbox: seedbox.clone(),
        ..load_settings()
    };
    save_settings(&settings)?;

    let resolved_password = if password.trim().is_empty() {
        load_seedbox_password()?.unwrap_or_default()
    } else {
        password.trim().to_string()
    };

    state
        .rclone
        .test_seedbox_connection(&app, &seedbox, &resolved_password)?;

    save_seedbox_password(&resolved_password)
}

#[tauri::command]
pub fn forget_seedbox_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    seedbox: SeedboxSettings,
) -> Result<(), String> {
    state.rclone.disconnect_seedbox(&app, &seedbox)?;
    delete_seedbox_password()
}

#[tauri::command]
pub fn unmount_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.rclone.unmount_all(&app);
    Ok(())
}

#[tauri::command]
pub fn is_mounted(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.rclone.is_mounted())
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
    state.rclone.unmount_all(&app);

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

    let has_b2 = has_complete_b2_config(&creds, &settings.buckets);
    let has_gdrive = has_complete_google_drive_config();
    let has_seedbox = has_complete_seedbox_config();

    if !has_b2 && !has_gdrive && !has_seedbox {
        return;
    }

    if let Ok(logger) = state.logger.lock() {
        logger.info("Attempting auto-mount from saved settings.");
    }

    let request = MountRequest {
        application_key_id: creds
            .as_ref()
            .map(|c| c.application_key_id.clone())
            .unwrap_or_default(),
        application_key: creds
            .as_ref()
            .map(|c| c.application_key.clone())
            .unwrap_or_default(),
        buckets: if has_b2 {
            settings.buckets
        } else {
            Vec::new()
        },
        google_drive: settings.google_drive,
        seedbox: settings.seedbox,
        seedbox_password: load_seedbox_password().ok().flatten().unwrap_or_default(),
        selected_provider: settings.selected_provider,
    };

    match state.rclone.mount_all(app, &request) {
        Ok(()) => {
            if let Ok(logger) = state.logger.lock() {
                logger.info("Auto-mount completed.");
            }
        }
        Err(err) => {
            if let Ok(logger) = state.logger.lock() {
                logger.error(format!("Auto-mount failed: {err}"));
            }
        }
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
    let mounted = state.rclone.is_mounted();
    let _ = app.emit("mount-state-changed", MountState { mounted });
}
