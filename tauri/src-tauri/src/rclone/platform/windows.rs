use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::models::BucketMount;
use crate::models::GoogleDriveSettings;
use crate::models::SeedboxSettings;

const RC_BASE_PORT: u16 = 5572;

pub fn is_fuse_installed() -> bool {
    let paths = [
        r"C:\Program Files (x86)\WinFsp\bin\winfsp-x64.dll",
        r"C:\Program Files\WinFsp\bin\winfsp-x64.dll",
    ];
    if paths.iter().any(|p| Path::new(p).exists()) {
        return true;
    }

    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY_LOCAL_MACHINE, KEY_READ,
    };

    unsafe {
        for subkey in [w!(r"Software\WinFsp"), w!(r"Software\WOW6432Node\WinFsp")] {
            let mut key = Default::default();
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey, Some(0), KEY_READ, &mut key).is_ok() {
                let _ = RegCloseKey(key);
                return true;
            }
        }
    }

    false
}

pub fn normalize_mount_target(bucket: &BucketMount) -> Result<String, String> {
    let bucket_name = bucket.bucket_name.trim();
    if bucket_name.is_empty() {
        return Err("Bucket name is required.".to_string());
    }

    let letter = normalize_drive_letter(&bucket.drive_letter)?;
    Ok(format!("{letter}:"))
}

pub fn validate_mount_target(target: &str) -> Result<(), String> {
    let letter = target.trim_end_matches(':').trim();
    if letter.len() != 1 {
        return Err(format!("Drive letter '{target}' is invalid."));
    }
    let ch = letter.chars().next().unwrap();
    if !ch.is_ascii_alphabetic() {
        return Err(format!("Drive letter '{target}' is invalid."));
    }
    Ok(())
}

pub fn prepare_mount_target(_target: &str) -> Result<(), String> {
    Ok(())
}

pub fn rc_port_for_drive(target: &str) -> u16 {
    let letter = target.chars().next().unwrap_or('A').to_ascii_uppercase();
    RC_BASE_PORT + (letter as u16 - 'A' as u16)
}

pub fn extra_mount_args(target: &str) -> Vec<String> {
    let port = rc_port_for_drive(target);
    vec![
        "--rc".to_string(),
        "--rc-addr".to_string(),
        format!("127.0.0.1:{port}"),
        "--rc-no-auth".to_string(),
    ]
}

pub fn volume_name_args(volume_name: &str) -> Vec<String> {
    vec!["--volname".to_string(), volume_name.to_string()]
}

pub fn is_mount_ready(target: &str) -> bool {
    drive_exists(target)
}

pub fn wait_for_mount_ready(target: &str, timeout_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if drive_exists(target) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

pub fn unmount_target(target: &str) -> bool {
    if !drive_exists(target) {
        return true;
    }

    let letter = target.trim_end_matches(':');
    let port = rc_port_for_drive(target);
    let rc_url = format!("http://127.0.0.1:{port}");

    let _ = Command::new("rclone")
        .args([
            "rc",
            "mount/unmount",
            &format!("mountPoint={letter}:"),
            "--rc-addr",
            &format!("127.0.0.1:{port}"),
            "--rc-no-auth",
        ])
        .output();

    let _ = Command::new("rclone")
        .args([
            "rc",
            "core/quit",
            "--rc-addr",
            &format!("127.0.0.1:{port}"),
            "--rc-no-auth",
        ])
        .output();

    let _ = rc_url;
    wait_for_drive_release(target, 8)
}

pub fn cleanup_mount_target(_target: &str) -> Result<bool, String> {
    Ok(false)
}

pub fn notify_mount_change(target: &str, added: bool) {
    use windows::Win32::UI::Shell::{
        SHChangeNotify, SHCNE_DRIVEADD, SHCNE_DRIVEREMOVED, SHCNF_FLUSH, SHCNF_PATHW,
    };

    let letter = target.chars().next().unwrap_or('Z').to_ascii_uppercase();
    let drive = format!("{}:\\", letter);
    let wide: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
    let event = if added {
        SHCNE_DRIVEADD
    } else {
        SHCNE_DRIVEREMOVED
    };
    unsafe {
        SHChangeNotify(
            event,
            SHCNF_PATHW | SHCNF_FLUSH,
            Some(wide.as_ptr() as *const _),
            None,
        );
    }
}

pub fn google_drive_mount_target(_settings: &GoogleDriveSettings) -> String {
    "G:".to_string()
}

pub fn seedbox_mount_target(_settings: &SeedboxSettings) -> String {
    "S:".to_string()
}

fn normalize_drive_letter(input: &str) -> Result<String, String> {
    let trimmed = input.trim().trim_end_matches(':');
    if trimmed.len() != 1 {
        return Err("Drive letter must be a single letter A-Z.".to_string());
    }
    let ch = trimmed.chars().next().unwrap().to_ascii_uppercase();
    if !('A'..='Z').contains(&ch) {
        return Err("Drive letter must be A-Z.".to_string());
    }
    if ch == 'G' {
        return Err("Drive letter G: is reserved for Google Drive.".to_string());
    }
    if ch == 'S' {
        return Err("Drive letter S: is reserved for Seedbox.".to_string());
    }
    Ok(ch.to_string())
}

fn drive_exists(target: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;

    let letter = target.chars().next().unwrap_or('A').to_ascii_uppercase();
    let root = format!("{letter}:\\");
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        GetVolumeInformationW(
            windows::core::PCWSTR(wide.as_ptr()),
            None,
            None,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

fn wait_for_drive_release(target: &str, timeout_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if !drive_exists(target) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    !drive_exists(target)
}
