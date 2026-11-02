use std::path::PathBuf;

const APP_DATA_DIR_ENV: &str = "CLOUD_DRIVE_MOUNT_APP_DATA_DIR";
const LOG_DIR_ENV: &str = "CLOUD_DRIVE_MOUNT_LOG_DIR";
pub const GOOGLE_DRIVE_MOUNT_NAME: &str = "google-drive";
pub const ONEDRIVE_MOUNT_NAME: &str = "onedrive";
pub const SEEDBOX_MOUNT_NAME: &str = "seedbox";

pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = path_from_env(APP_DATA_DIR_ENV) {
        return dir;
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join("Library")
            .join("Application Support")
            .join("CloudDriveMount")
    }
    #[cfg(windows)]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("CloudDriveMount")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("CloudDriveMount")
    }
}

pub fn log_dir() -> PathBuf {
    if let Some(dir) = path_from_env(LOG_DIR_ENV) {
        return dir;
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join("Library")
            .join("Logs")
            .join("CloudDriveMount")
    }
    #[cfg(not(target_os = "macos"))]
    {
        app_data_dir().join("logs")
    }
}

fn path_from_env(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    if value.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

pub fn rclone_config_path() -> PathBuf {
    app_data_dir().join("rclone.conf")
}

pub fn rclone_cache_dir() -> PathBuf {
    app_data_dir().join("cache")
}

#[cfg(target_os = "macos")]
pub fn drives_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Drives")
}

#[cfg(target_os = "macos")]
pub fn default_bucket_mount_path(bucket_name: &str) -> String {
    drives_dir()
        .join(bucket_name)
        .to_string_lossy()
        .into_owned()
}

#[cfg(target_os = "macos")]
pub fn default_google_drive_mount_path() -> String {
    drives_dir()
        .join(GOOGLE_DRIVE_MOUNT_NAME)
        .to_string_lossy()
        .into_owned()
}

#[cfg(target_os = "macos")]
pub fn default_one_drive_mount_path() -> String {
    drives_dir()
        .join(ONEDRIVE_MOUNT_NAME)
        .to_string_lossy()
        .into_owned()
}

pub fn normalize_remote_path(path: &str) -> String {
    let mut normalized = path.trim().replace('\\', "/");
    while normalized.starts_with('/') || normalized.starts_with(':') {
        normalized = normalized[1..].to_string();
    }
    normalized
}

pub fn normalize_google_drive_path(path: &str) -> String {
    normalize_remote_path(path)
}

pub fn normalize_seedbox_host(host: &str) -> String {
    let mut normalized = host.trim().to_string();
    for scheme in ["https://", "http://", "ftps://", "ftp://"] {
        if normalized.to_lowercase().starts_with(scheme) {
            normalized = normalized[scheme.len()..].trim().to_string();
            break;
        }
    }
    while normalized.ends_with('/') {
        normalized.pop();
    }
    normalized.trim().to_string()
}

#[cfg(target_os = "macos")]
pub fn default_seedbox_mount_path() -> String {
    drives_dir()
        .join(SEEDBOX_MOUNT_NAME)
        .to_string_lossy()
        .into_owned()
}

pub fn platform_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(windows)]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        "linux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_overrides_control_all_stateful_app_paths() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        let app_data = temp.path().join("app-data");
        let logs = temp.path().join("logs");
        crate::test_support::set_test_dirs(&app_data, &logs);

        assert_eq!(app_data_dir(), app_data);
        assert_eq!(log_dir(), logs);
        assert_eq!(settings_path(), app_data.join("settings.json"));
        assert_eq!(rclone_config_path(), app_data.join("rclone.conf"));
        assert_eq!(rclone_cache_dir(), app_data.join("cache"));

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn empty_env_overrides_are_ignored() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        std::env::set_var(APP_DATA_DIR_ENV, "   ");
        std::env::set_var(LOG_DIR_ENV, "");

        assert!(app_data_dir().ends_with("CloudDriveMount"));
        assert!(log_dir().ends_with("CloudDriveMount") || log_dir().ends_with("logs"));

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn normalize_remote_path_strips_prefixes_and_converts_separators() {
        assert_eq!(normalize_remote_path("  /Movies\\2026  "), "Movies/2026");
        assert_eq!(normalize_remote_path("::/nested/path"), "nested/path");
        assert_eq!(
            normalize_remote_path("folder/subfolder"),
            "folder/subfolder"
        );
        assert_eq!(normalize_remote_path("   "), "");
    }

    #[test]
    fn normalize_google_drive_path_uses_remote_path_rules() {
        assert_eq!(normalize_google_drive_path(":\\Team Drive"), "Team Drive");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_service_mount_paths_use_stable_lowercase_names() {
        assert!(default_google_drive_mount_path().ends_with("/Drives/google-drive"));
        assert!(default_one_drive_mount_path().ends_with("/Drives/onedrive"));
        assert!(default_seedbox_mount_path().ends_with("/Drives/seedbox"));
    }

    #[test]
    fn normalize_seedbox_host_removes_schemes_and_trailing_slashes() {
        assert_eq!(
            normalize_seedbox_host("  FTPS://seedbox.example.com///  "),
            "seedbox.example.com"
        );
        assert_eq!(
            normalize_seedbox_host("https://host.example.com/path/"),
            "host.example.com/path"
        );
        assert_eq!(
            normalize_seedbox_host("plain.example.com"),
            "plain.example.com"
        );
    }

    #[test]
    fn platform_name_matches_current_target() {
        #[cfg(target_os = "macos")]
        assert_eq!(platform_name(), "macos");
        #[cfg(windows)]
        assert_eq!(platform_name(), "windows");
        #[cfg(not(any(target_os = "macos", windows)))]
        assert_eq!(platform_name(), "linux");
    }
}
