use std::path::PathBuf;

pub fn app_data_dir() -> PathBuf {
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

pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

pub fn rclone_config_path() -> PathBuf {
    app_data_dir().join("rclone.conf")
}

pub fn rclone_cache_dir() -> PathBuf {
    app_data_dir().join("cache")
}

pub fn default_bucket_mount_path(bucket_name: &str) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    home.join("Drives").join(bucket_name).to_string_lossy().into_owned()
}

pub fn default_google_drive_mount_path() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    home.join("Drives")
        .join("Google Drive")
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

pub fn default_seedbox_mount_path() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    home.join("Drives")
        .join("Seedbox")
        .to_string_lossy()
        .into_owned()
}

pub fn expand_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('~') {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        if trimmed == "~" {
            return home.to_string_lossy().into_owned();
        }
        if trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
            return home
                .join(trimmed.trim_start_matches('~').trim_start_matches(['/', '\\']))
                .to_string_lossy()
                .into_owned();
        }
    }
    trimmed.to_string()
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
