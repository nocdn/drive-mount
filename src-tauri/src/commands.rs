use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::credentials::{
    delete_seedbox_password, has_saved_seedbox_password, load_b2_credentials,
    load_seedbox_password, save_b2_credentials, save_seedbox_password,
};
use crate::logging::LogEmitter;
use crate::models::{
    AppSettings, B2Credentials, GoogleDriveSettings, LoadedCredentials, MountRequest,
    SeedboxSettings,
};
use crate::notifications::show_app_notification;
use crate::paths::{log_dir, platform_name};
use crate::rclone::{
    has_complete_b2_config, has_complete_google_drive_config, has_complete_seedbox_config,
    is_fuse_installed, is_google_drive_configured, is_seedbox_configured, RcloneManager,
};
use crate::settings::{load_settings, save_settings};

const AUTO_MOUNT_START_NOTIFICATION: &str = "Mounting saved drives on launch. Please wait.";
const AUTO_MOUNT_COMPLETE_NOTIFICATION: &str = "Auto-mount complete. Cloud Drive Mount is ready.";
const AUTO_MOUNT_FAILED_NOTIFICATION: &str = "Auto-mount failed. Open Settings for details.";

pub struct AppState {
    pub rclone: Arc<RcloneManager>,
    pub logger: Arc<Mutex<LogEmitter>>,
    pub auto_mount_attempted: Arc<Mutex<bool>>,
}

async fn run_blocking<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_platform() -> String {
    platform_name().to_string()
}

#[tauri::command]
pub fn load_settings_cmd(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let settings = load_settings();

    if let Ok(logger) = state.logger.lock() {
        logger.info("Settings loaded.");
    }

    Ok(settings)
}

#[tauri::command]
pub async fn load_credentials_cmd(state: State<'_, AppState>) -> Result<LoadedCredentials, String> {
    let logger = state.logger.clone();

    run_blocking(move || {
        let b2_credentials = load_b2_credentials()?;
        let has_saved_credentials = b2_credentials.is_some();
        let is_google_drive_configured = is_google_drive_configured();
        let is_seedbox_configured = is_seedbox_configured();
        let has_saved_seedbox_password = has_saved_seedbox_password().unwrap_or(false);

        if let Ok(logger) = logger.lock() {
            logger.info("Credential state loaded.");
        }

        Ok(LoadedCredentials {
            has_saved_credentials,
            b2_credentials,
            is_google_drive_configured,
            is_seedbox_configured,
            has_saved_seedbox_password,
        })
    })
    .await
}

#[tauri::command]
pub async fn attempt_auto_mount_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let state = AppState {
        rclone: state.rclone.clone(),
        logger: state.logger.clone(),
        auto_mount_attempted: state.auto_mount_attempted.clone(),
    };

    run_blocking(move || {
        attempt_auto_mount_once(&app, &state);
        Ok(())
    })
    .await
}
#[tauri::command]
pub fn save_settings_cmd(mut settings: AppSettings) -> Result<AppSettings, String> {
    settings = settings.normalized();
    save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn save_b2_credentials_cmd(credentials: B2Credentials) -> Result<(), String> {
    save_b2_credentials(&credentials)
}

#[tauri::command]
pub async fn mount_all(
    app: AppHandle,
    state: State<'_, AppState>,
    request: MountRequest,
) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || {
        if !request.application_key_id.trim().is_empty()
            || !request.application_key.trim().is_empty()
        {
            save_b2_credentials(&B2Credentials {
                application_key_id: request.application_key_id.clone(),
                application_key: request.application_key.clone(),
            })?;
        }

        let settings = AppSettings {
            buckets: request.buckets.clone(),
            google_drive: request.google_drive.normalized(),
            seedbox: request.seedbox.normalized(),
            selected_provider: request.selected_provider,
            ..load_settings()
        }
        .normalized();
        save_settings(&settings)?;

        rclone.mount_all(&app, &request)
    })
    .await
}

#[tauri::command]
pub fn is_google_drive_configured_cmd(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.rclone.is_google_drive_configured())
}

#[tauri::command]
pub async fn configure_google_drive_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    google_drive: GoogleDriveSettings,
) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || {
        let settings = AppSettings {
            google_drive: google_drive.normalized(),
            ..load_settings()
        }
        .normalized();
        save_settings(&settings)?;

        rclone.configure_google_drive(&app, &google_drive)
    })
    .await
}

#[tauri::command]
pub async fn disconnect_google_drive_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    google_drive: GoogleDriveSettings,
) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || rclone.disconnect_google_drive(&app, &google_drive)).await
}

#[tauri::command]
pub async fn test_google_drive_connection_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    google_drive: GoogleDriveSettings,
) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || rclone.test_google_drive_connection(&app, &google_drive)).await
}

#[tauri::command]
pub fn is_seedbox_configured_cmd(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.rclone.is_seedbox_configured())
}

#[tauri::command]
pub async fn test_seedbox_connection_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    seedbox: SeedboxSettings,
    password: String,
) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || {
        let settings = AppSettings {
            seedbox: seedbox.normalized(),
            ..load_settings()
        }
        .normalized();
        save_settings(&settings)?;

        let resolved_password = if password.trim().is_empty() {
            load_seedbox_password()?.unwrap_or_default()
        } else {
            password.trim().to_string()
        };

        rclone.test_seedbox_connection(&app, &seedbox, &resolved_password)?;

        save_seedbox_password(&resolved_password)
    })
    .await
}

#[tauri::command]
pub async fn forget_seedbox_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    seedbox: SeedboxSettings,
) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || {
        rclone.disconnect_seedbox(&app, &seedbox)?;
        delete_seedbox_password()
    })
    .await
}

#[tauri::command]
pub async fn unmount_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || {
        rclone.unmount_all(&app);
        Ok(())
    })
    .await
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
pub fn open_mount_target(app: AppHandle, target: String) -> Result<(), String> {
    if target.trim().is_empty() {
        return Err("Mount target is missing.".to_string());
    }
    app.opener()
        .open_path(target, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) -> Result<(), String> {
    state.logger.lock().map_err(|e| e.to_string())?.clear()
}

#[tauri::command]
pub async fn restart_mounts(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let rclone = state.rclone.clone();
    let logger = state.logger.clone();

    run_blocking(move || {
        if let Ok(logger) = logger.lock() {
            logger.info("Restarting mount background processes.");
        }

        rclone.restart_mount_cleanup(&app);

        if !is_fuse_installed() {
            if let Ok(logger) = logger.lock() {
                logger.info(
                    "FUSE provider not detected; mount restart completed without remounting.",
                );
            }
            return Ok(());
        }

        let Some(request) = saved_mount_request() else {
            if let Ok(logger) = logger.lock() {
                logger.info("No configured mounts to restart.");
            }
            return Ok(());
        };

        if let Ok(logger) = logger.lock() {
            logger.info("Remounting configured targets after cleanup.");
        }

        match rclone.mount_all(&app, &request) {
            Ok(()) => {
                if let Ok(logger) = logger.lock() {
                    logger.info("Mount background processes restarted.");
                }
                Ok(())
            }
            Err(err) => {
                if let Ok(logger) = logger.lock() {
                    logger.error(format!("Mount restart failed: {err}"));
                }
                Err(err)
            }
        }
    })
    .await
}

pub fn attempt_auto_mount(app: &AppHandle, state: &AppState) {
    if !is_fuse_installed() {
        if let Ok(logger) = state.logger.lock() {
            logger.info("FUSE provider not detected; skipping auto-mount.");
        }
        return;
    }

    let Some(request) = saved_mount_request() else {
        return;
    };

    show_app_notification(app, AUTO_MOUNT_START_NOTIFICATION);

    if let Ok(logger) = state.logger.lock() {
        logger.info("Attempting auto-mount from saved settings.");
    }

    match state.rclone.mount_all(app, &request) {
        Ok(()) => {
            if let Ok(logger) = state.logger.lock() {
                logger.info("Auto-mount completed.");
            }
            show_app_notification(app, AUTO_MOUNT_COMPLETE_NOTIFICATION);
        }
        Err(err) => {
            if let Ok(logger) = state.logger.lock() {
                logger.error(format!("Auto-mount failed: {err}"));
            }
            show_app_notification(app, AUTO_MOUNT_FAILED_NOTIFICATION);
        }
    }
}

pub fn attempt_auto_mount_once(app: &AppHandle, state: &AppState) {
    let Ok(mut attempted) = state.auto_mount_attempted.lock() else {
        return;
    };

    if *attempted {
        return;
    }

    *attempted = true;
    drop(attempted);

    attempt_auto_mount(app, state);
}

fn saved_mount_request() -> Option<MountRequest> {
    let settings = load_settings().normalized();
    let creds = load_b2_credentials().ok().flatten();
    let has_b2 = has_complete_b2_config(&creds, &settings.buckets);
    let has_gdrive = has_complete_google_drive_config();
    let has_seedbox = has_complete_seedbox_config();

    if !has_b2 && !has_gdrive && !has_seedbox {
        return None;
    }

    Some(MountRequest {
        application_key_id: creds
            .as_ref()
            .map(|c| c.application_key_id.clone())
            .unwrap_or_default(),
        application_key: creds
            .as_ref()
            .map(|c| c.application_key.clone())
            .unwrap_or_default(),
        buckets: if has_b2 { settings.buckets } else { Vec::new() },
        google_drive: settings.google_drive,
        seedbox: settings.seedbox,
        seedbox_password: load_seedbox_password().ok().flatten().unwrap_or_default(),
        selected_provider: settings.selected_provider,
    })
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
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window_clone.hide();
            }
            tauri::WindowEvent::Resized(_) if window_clone.is_minimized().unwrap_or(false) => {
                let _ = window_clone.hide();
                let _ = window_clone.unminimize();
            }
            _ => {}
        });
    }
}

pub fn emit_mount_state(app: &AppHandle, state: &AppState) {
    let _ = app.emit("mount-state-changed", state.rclone.mount_state());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CloudProvider;

    #[test]
    fn get_platform_delegates_to_platform_name() {
        assert_eq!(get_platform(), crate::paths::platform_name());
    }

    #[test]
    fn save_settings_cmd_normalizes_cloud_specific_settings_before_writing() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        save_settings_cmd(AppSettings {
            selected_provider: CloudProvider::Seedbox,
            google_drive: GoogleDriveSettings {
                remote_path: " :/Team\\Docs ".to_string(),
                root_folder_id: " root ".to_string(),
            },
            seedbox: SeedboxSettings {
                host: "https://seedbox.example.com///".to_string(),
                username: " user ".to_string(),
                port: 2121,
                remote_path: " :/downloads\\movies ".to_string(),
                ..SeedboxSettings::default()
            },
            ..AppSettings::default()
        })
        .unwrap();

        let saved = load_settings();
        assert_eq!(saved.selected_provider, CloudProvider::Seedbox);
        assert_eq!(saved.google_drive.remote_path, "Team/Docs");
        assert_eq!(saved.google_drive.root_folder_id, "root");
        assert_eq!(saved.seedbox.host, "seedbox.example.com");
        assert_eq!(saved.seedbox.username, "user");
        assert_eq!(saved.seedbox.port, 2121);
        assert_eq!(saved.seedbox.remote_path, "downloads/movies");

        crate::test_support::clear_test_dirs();
    }
}
