#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(any(target_os = "macos", windows)))]
pub fn is_fuse_installed() -> bool {
    false
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn is_mount_ready(_target: &str) -> bool {
    false
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn wait_for_mount_ready(_target: &str, _timeout_secs: u64) -> bool {
    false
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn unmount_target(_target: &str) -> bool {
    false
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn unmount_target_with_rclone(_target: &str, _rclone_path: Option<&std::path::Path>) -> bool {
    false
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn cleanup_mount_target(_target: &str) -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn validate_mount_target(_target: &str) -> Result<(), String> {
    Err("Unsupported platform".to_string())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn normalize_mount_target(bucket: &crate::models::BucketMount) -> Result<String, String> {
    let _ = bucket;
    Err("Unsupported platform".to_string())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn extra_mount_args(_target: &str) -> Vec<String> {
    vec![]
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn volume_name_args(_volume_name: &str) -> Vec<String> {
    vec![]
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn prepare_mount_target(_target: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn notify_mount_change(_target: &str, _added: bool) {}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn google_drive_mount_target(_settings: &crate::models::GoogleDriveSettings) -> String {
    String::new()
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn seedbox_mount_target(_settings: &crate::models::SeedboxSettings) -> String {
    String::new()
}

#[cfg(not(windows))]
pub fn used_windows_drive_letters() -> Vec<String> {
    Vec::new()
}
