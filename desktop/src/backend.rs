//! Everything the window asks of the backend, as plain blocking calls. The
//! window runs each on a worker thread (see `ui::tasks`) and gets the result
//! back on its own thread.

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::credentials::{
    delete_seedbox_password, has_saved_seedbox_password, load_b2_credentials,
    load_seedbox_password, save_b2_credentials, save_seedbox_password,
};
use crate::host::Host;
use crate::logging::LogEmitter;
use crate::models::{
    AppSettings, B2Credentials, GoogleDriveSettings, LoadedCredentials, MountRequest,
    OneDriveSettings, SeedboxSettings,
};
use crate::notifications::show_app_notification;
use crate::rclone::{
    has_complete_b2_config, has_complete_google_drive_config, has_complete_one_drive_config,
    has_complete_seedbox_config, is_fuse_installed, is_google_drive_configured,
    is_network_connectivity_error, is_one_drive_configured, is_seedbox_configured, RcloneManager,
};
use crate::settings::{load_settings, save_settings};

const AUTO_MOUNT_START_NOTIFICATION: &str = "Mounting saved drives on launch. Please wait.";
const AUTO_MOUNT_COMPLETE_NOTIFICATION: &str = "Auto-mount complete. Cloud Drive Mount is ready.";
const AUTO_MOUNT_FAILED_NOTIFICATION: &str = "Auto-mount failed. Open Settings for details.";
const AUTO_MOUNT_NETWORK_FAILED_NOTIFICATION: &str =
    "Auto-mount failed because of network connectivity. Check your internet connection and try again.";
pub const QUIT_START_NOTIFICATION: &str = "Unmounting active drives before quitting. Please wait.";
pub const QUIT_COMPLETE_NOTIFICATION: &str =
    "Unmount complete. Cloud Drive Mount has finished shutting down.";

/// The app's long-lived services, shared by the window, the tray and the
/// launch sequence. Cloning shares them.
#[derive(Clone)]
pub struct Backend {
    pub rclone: Arc<RcloneManager>,
    pub logger: Arc<Mutex<LogEmitter>>,
    pub host: Host,
    auto_mount_attempted: Arc<Mutex<bool>>,
}

impl Backend {
    pub fn new(host: Host) -> Self {
        let logger = Arc::new(Mutex::new(LogEmitter::new(Some(host.clone()))));
        Self {
            rclone: Arc::new(RcloneManager::new(logger.clone())),
            logger,
            host,
            auto_mount_attempted: Arc::new(Mutex::new(false)),
        }
    }

    pub fn log_info(&self, message: impl AsRef<str>) {
        if let Ok(logger) = self.logger.lock() {
            logger.info(message);
        }
    }

    pub fn log_error(&self, message: impl AsRef<str>) {
        if let Ok(logger) = self.logger.lock() {
            logger.error(message);
        }
    }

    pub fn load_settings(&self) -> AppSettings {
        let settings = load_settings();
        self.log_info("Settings loaded.");
        settings
    }

    pub fn load_credentials(&self) -> Result<LoadedCredentials, String> {
        let b2_credentials = load_b2_credentials()?;
        let loaded = LoadedCredentials {
            has_saved_credentials: b2_credentials.is_some(),
            b2_credentials,
            is_google_drive_configured: is_google_drive_configured(),
            is_one_drive_configured: is_one_drive_configured(),
            is_seedbox_configured: is_seedbox_configured(),
            has_saved_seedbox_password: has_saved_seedbox_password().unwrap_or(false),
        };
        self.log_info("Credential state loaded.");
        Ok(loaded)
    }

    /// Cleans up mounts a previous run left behind, then reports the state.
    pub fn startup_cleanup(&self) {
        self.rclone.cleanup_stale_processes(&self.host);
        self.rclone.refresh_configured_mount_targets();
        if !is_fuse_installed() {
            self.log_info("FUSE provider not detected on launch.");
        }
        self.host.emit_mount_state(self.rclone.mount_state());
    }

    pub fn mount_all(&self, request: MountRequest) -> Result<(), String> {
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
            one_drive: request.one_drive.normalized(),
            seedbox: request.seedbox.normalized(),
            selected_provider: request.selected_provider,
            ..load_settings()
        }
        .normalized();
        save_settings(&settings)?;

        self.rclone.mount_all(&self.host, &request)
    }

    pub fn configure_google_drive(
        &self,
        google_drive: GoogleDriveSettings,
        client_secret: &str,
    ) -> Result<(), String> {
        save_section(|settings| settings.google_drive = google_drive.normalized())?;
        self.rclone
            .configure_google_drive(&self.host, &google_drive, client_secret)
    }

    pub fn disconnect_google_drive(&self, google_drive: GoogleDriveSettings) -> Result<(), String> {
        self.rclone
            .disconnect_google_drive(&self.host, &google_drive)
    }

    pub fn test_google_drive(&self, google_drive: GoogleDriveSettings) -> Result<(), String> {
        save_section(|settings| settings.google_drive = google_drive.normalized())?;
        self.rclone
            .test_google_drive_connection(&self.host, &google_drive)
    }

    pub fn configure_one_drive(&self, one_drive: OneDriveSettings) -> Result<(), String> {
        save_section(|settings| settings.one_drive = one_drive.normalized())?;
        self.rclone.configure_one_drive(&self.host, &one_drive)
    }

    pub fn disconnect_one_drive(&self, one_drive: OneDriveSettings) -> Result<(), String> {
        self.rclone.disconnect_one_drive(&self.host, &one_drive)
    }

    pub fn test_one_drive(&self, one_drive: OneDriveSettings) -> Result<(), String> {
        save_section(|settings| settings.one_drive = one_drive.normalized())?;
        self.rclone
            .test_one_drive_connection(&self.host, &one_drive)
    }

    pub fn test_seedbox(&self, seedbox: SeedboxSettings, password: String) -> Result<(), String> {
        save_section(|settings| settings.seedbox = seedbox.normalized())?;
        let resolved_password = if password.trim().is_empty() {
            load_seedbox_password()?.unwrap_or_default()
        } else {
            password.trim().to_string()
        };
        self.rclone
            .test_seedbox_connection(&self.host, &seedbox, &resolved_password)?;
        save_seedbox_password(&resolved_password)
    }

    pub fn forget_seedbox(&self, seedbox: SeedboxSettings) -> Result<(), String> {
        self.rclone.disconnect_seedbox(&self.host, &seedbox)?;
        delete_seedbox_password()
    }

    pub fn unmount_all(&self) {
        self.rclone.unmount_all(&self.host);
    }

    pub fn refresh_mount_caches(&self) -> Result<(), String> {
        self.rclone.refresh_mount_caches(&self.host)
    }

    pub fn clear_logs(&self) -> Result<(), String> {
        self.logger.lock().map_err(|e| e.to_string())?.clear()
    }

    pub fn restart_mounts(&self) -> Result<(), String> {
        self.log_info("Restarting mount background processes.");
        self.rclone.restart_mount_cleanup(&self.host);

        if !is_fuse_installed() {
            self.log_info(
                "FUSE provider not detected; mount restart completed without remounting.",
            );
            return Ok(());
        }

        let Some(request) = saved_mount_request() else {
            self.log_info("No configured mounts to restart.");
            return Ok(());
        };

        self.log_info("Remounting configured targets after cleanup.");
        match self.rclone.mount_all(&self.host, &request) {
            Ok(()) => {
                self.log_info("Mount background processes restarted.");
                Ok(())
            }
            Err(err) => {
                self.log_error(format!("Mount restart failed: {err}"));
                Err(err)
            }
        }
    }

    pub fn attempt_auto_mount(&self) {
        if !is_fuse_installed() {
            self.log_info("FUSE provider not detected; skipping auto-mount.");
            return;
        }

        let Some(request) = saved_mount_request() else {
            return;
        };

        show_app_notification(AUTO_MOUNT_START_NOTIFICATION);
        self.log_info("Attempting auto-mount from saved settings.");

        match self.rclone.mount_all(&self.host, &request) {
            Ok(()) => {
                self.log_info("Auto-mount completed.");
                show_app_notification(AUTO_MOUNT_COMPLETE_NOTIFICATION);
            }
            Err(err) => {
                self.log_error(format!("Auto-mount failed: {err}"));
                show_app_notification(auto_mount_failed_notification(&err));
            }
        }
    }

    pub fn attempt_auto_mount_once(&self) {
        let Ok(mut attempted) = self.auto_mount_attempted.lock() else {
            return;
        };
        if *attempted {
            return;
        }
        *attempted = true;
        drop(attempted);
        self.attempt_auto_mount();
    }

    /// Unmounts everything before the app exits, telling the user while it
    /// takes time.
    pub fn shut_down(&self) {
        let busy = self.rclone.is_mounted();
        if busy {
            show_app_notification(QUIT_START_NOTIFICATION);
        }
        self.unmount_all();
        if busy {
            show_app_notification(QUIT_COMPLETE_NOTIFICATION);
        }
    }
}

/// Saves the settings file with one section replaced.
fn save_section(change: impl FnOnce(&mut AppSettings)) -> Result<(), String> {
    let mut settings = load_settings();
    change(&mut settings);
    save_settings(&settings.normalized())
}

/// Normalizes and saves `settings`, returning what was written.
pub fn save_settings_normalized(settings: AppSettings) -> Result<AppSettings, String> {
    let settings = settings.normalized();
    save_settings(&settings)?;
    Ok(settings)
}

pub fn save_b2_credentials_now(credentials: &B2Credentials) -> Result<(), String> {
    save_b2_credentials(credentials)
}

/// Opens a folder or drive in Finder or File Explorer.
pub fn open_path(target: &Path) -> Result<(), String> {
    if target.as_os_str().is_empty() {
        return Err("Mount target is missing.".to_string());
    }
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(windows)]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn auto_mount_failed_notification(error: &str) -> &'static str {
    if is_network_connectivity_error(error) {
        AUTO_MOUNT_NETWORK_FAILED_NOTIFICATION
    } else {
        AUTO_MOUNT_FAILED_NOTIFICATION
    }
}

fn saved_mount_request() -> Option<MountRequest> {
    let settings = load_settings().normalized();
    let creds = load_b2_credentials().ok().flatten();
    let has_b2 = has_complete_b2_config(&creds, &settings.buckets);
    let has_gdrive = has_complete_google_drive_config();
    let has_onedrive = has_complete_one_drive_config();
    let has_seedbox = has_complete_seedbox_config();

    if !has_b2 && !has_gdrive && !has_onedrive && !has_seedbox {
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
        one_drive: settings.one_drive,
        seedbox: settings.seedbox,
        seedbox_password: load_seedbox_password().ok().flatten().unwrap_or_default(),
        selected_provider: settings.selected_provider,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CloudProvider;

    #[test]
    fn auto_mount_failed_notification_mentions_network_connectivity_for_network_errors() {
        assert_eq!(
            auto_mount_failed_notification(
                "Google Drive could not connect because of network connectivity."
            ),
            AUTO_MOUNT_NETWORK_FAILED_NOTIFICATION
        );
        assert_eq!(
            auto_mount_failed_notification("dial tcp: lookup graph.microsoft.com: no such host"),
            AUTO_MOUNT_NETWORK_FAILED_NOTIFICATION
        );
    }

    #[test]
    fn auto_mount_failed_notification_stays_generic_for_non_network_errors() {
        assert_eq!(
            auto_mount_failed_notification("FUSE-T is not installed."),
            AUTO_MOUNT_FAILED_NOTIFICATION
        );
    }

    #[test]
    fn save_settings_normalizes_cloud_specific_settings_before_writing() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        save_settings_normalized(AppSettings {
            selected_provider: CloudProvider::Seedbox,
            google_drive: GoogleDriveSettings {
                remote_path: " :/Team\\Docs ".to_string(),
                root_folder_id: " root ".to_string(),
                ..GoogleDriveSettings::default()
            },
            one_drive: OneDriveSettings {
                remote_path: " :/Personal\\Docs ".to_string(),
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
        assert_eq!(saved.one_drive.remote_path, "Personal/Docs");
        assert_eq!(saved.seedbox.host, "seedbox.example.com");
        assert_eq!(saved.seedbox.username, "user");
        assert_eq!(saved.seedbox.port, 2121);
        assert_eq!(saved.seedbox.remote_path, "downloads/movies");

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn saved_mount_request_includes_one_drive_when_configured() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));
        std::fs::create_dir_all(crate::paths::app_data_dir()).unwrap();
        std::fs::write(
            crate::paths::rclone_config_path(),
            "[onedrive]\ntype = onedrive\ndrive_id = personal-id\ndrive_type = personal\n",
        )
        .unwrap();

        crate::settings::save_settings(&AppSettings {
            selected_provider: CloudProvider::OneDrive,
            one_drive: OneDriveSettings {
                remote_path: " Personal Docs ".to_string(),
            },
            ..AppSettings::default()
        })
        .unwrap();

        let request = saved_mount_request().unwrap();

        assert_eq!(request.selected_provider, CloudProvider::OneDrive);
        assert_eq!(request.buckets, Vec::new());
        assert_eq!(request.one_drive.remote_path, "Personal Docs");

        crate::test_support::clear_test_dirs();
    }
}
