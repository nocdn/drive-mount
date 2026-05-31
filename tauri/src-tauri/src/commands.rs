use std::process::Command;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::credentials::{
    delete_seedbox_password, has_saved_seedbox_password, load_b2_credentials,
    load_seedbox_password, save_b2_credentials, save_seedbox_password,
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
            google_drive: request.google_drive.clone(),
            seedbox: request.seedbox.clone(),
            selected_provider: request.selected_provider,
            ..load_settings()
        };
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
            google_drive: google_drive.clone(),
            ..load_settings()
        };
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
            seedbox: seedbox.clone(),
            ..load_settings()
        };
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
pub fn clear_logs(state: State<'_, AppState>) -> Result<(), String> {
    state.logger.lock().map_err(|e| e.to_string())?.clear()
}

#[tauri::command]
pub async fn browse_folder(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_title("Choose mount folder")
            .pick_folder(move |path| {
                let _ = tx.send(path.map(|p| p.to_string()));
            });
        rx.await
            .map_err(|_| "Folder picker closed unexpectedly.".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
pub async fn restart_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let rclone = state.rclone.clone();

    run_blocking(move || {
        rclone.unmount_all(&app);

        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut cmd = Command::new(exe);
        cmd.arg("--show-settings");
        cmd.spawn().map_err(|e| e.to_string())?;
        app.exit(0);
        Ok(())
    })
    .await
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
        buckets: if has_b2 { settings.buckets } else { Vec::new() },
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
        window.on_window_event(move |event| match event {
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
        });
    }
}

pub fn emit_mount_state(app: &AppHandle, state: &AppState) {
    let mounted = state.rclone.is_mounted();
    let _ = app.emit("mount-state-changed", MountState { mounted });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CloudProvider, GDRIVE_REMOTE, SEEDBOX_REMOTE};

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
                remote_name: "custom-drive".to_string(),
                remote_path: " :/Team\\Docs ".to_string(),
                root_folder_id: " root ".to_string(),
                drive_letter: "Z".to_string(),
                ..GoogleDriveSettings::default()
            },
            seedbox: SeedboxSettings {
                remote_name: "custom-seedbox".to_string(),
                host: "https://seedbox.example.com///".to_string(),
                username: " user ".to_string(),
                port: 0,
                remote_path: " :/downloads\\movies ".to_string(),
                drive_letter: "X".to_string(),
                ..SeedboxSettings::default()
            },
            ..AppSettings::default()
        })
        .unwrap();

        let saved = load_settings();
        assert_eq!(saved.selected_provider, CloudProvider::Seedbox);
        assert_eq!(saved.google_drive.remote_name, GDRIVE_REMOTE);
        assert_eq!(saved.google_drive.remote_path, "Team/Docs");
        assert_eq!(saved.google_drive.root_folder_id, "root");
        assert_eq!(saved.google_drive.drive_letter, "G");
        assert_eq!(saved.seedbox.remote_name, SEEDBOX_REMOTE);
        assert_eq!(saved.seedbox.host, "seedbox.example.com");
        assert_eq!(saved.seedbox.username, "user");
        assert_eq!(saved.seedbox.port, 21);
        assert_eq!(saved.seedbox.remote_path, "downloads/movies");
        assert_eq!(saved.seedbox.drive_letter, "S");

        crate::test_support::clear_test_dirs();
    }
}
