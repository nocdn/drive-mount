use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::models::BucketMount;
use crate::models::GoogleDriveSettings;
use crate::models::SeedboxSettings;
use crate::paths::{
    default_bucket_mount_path, default_google_drive_mount_path, default_seedbox_mount_path,
    drives_dir,
};

pub fn is_fuse_installed() -> bool {
    let paths = [
        "/Library/Filesystems/macfuse.fs",
        "/usr/local/lib/libfuse.2.dylib",
        "/opt/homebrew/lib/libfuse.2.dylib",
        "/Library/Filesystems/fuse-t.fs",
        "/usr/local/bin/fuse-t",
        "/opt/homebrew/bin/fuse-t",
    ];
    paths.iter().any(|p| Path::new(p).exists())
}

pub fn normalize_mount_target(bucket: &BucketMount) -> Result<String, String> {
    let bucket_name = bucket.bucket_name.trim();
    if bucket_name.is_empty() {
        return Err("Bucket name is required.".to_string());
    }

    validate_mount_folder_name(bucket_name, "Bucket")?;
    let target = default_bucket_mount_path(bucket_name);
    validate_mount_target(&target)?;
    Ok(target)
}

pub fn validate_mount_target(target: &str) -> Result<(), String> {
    let path = Path::new(target);
    if !path.is_absolute() {
        return Err(format!("Mount folder '{target}' is invalid."));
    }
    let drives_dir = drives_dir();
    if path == drives_dir.as_path() || path.parent() != Some(drives_dir.as_path()) {
        return Err("Mount folder must be directly inside ~/Drives.".to_string());
    }
    Ok(())
}

pub fn prepare_mount_target(target: &str) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| e.to_string())
}

pub fn extra_mount_args(_target: &str) -> Vec<String> {
    vec!["--option".to_string(), "nobrowse".to_string()]
}

pub fn volume_name_args(volume_name: &str) -> Vec<String> {
    vec!["--volname".to_string(), volume_name.to_string()]
}

pub fn is_mount_ready(target: &str) -> bool {
    is_mount_point(target)
}

pub fn wait_for_mount_ready(target: &str, timeout_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if is_mount_point(target) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

pub fn unmount_target(target: &str) -> bool {
    if !is_mount_point(target) {
        return true;
    }

    const UNMOUNT_TIMEOUT: Duration = Duration::from_secs(5);

    let attempts: [(&str, &[&str]); 4] = [
        ("/usr/sbin/diskutil", &["unmount", target]),
        ("/sbin/umount", &[target]),
        ("/usr/sbin/diskutil", &["unmount", "force", target]),
        ("/sbin/umount", &["-f", target]),
    ];

    for (executable, args) in attempts {
        if Path::new(executable).exists() {
            if run_command_with_timeout(executable, args, UNMOUNT_TIMEOUT)
                && !is_mount_point(target)
            {
                return true;
            }
        }
    }

    wait_for_mount_release(target, UNMOUNT_TIMEOUT);
    !is_mount_point(target)
}

pub fn cleanup_mount_target(target: &str) -> Result<bool, String> {
    let path = Path::new(target);
    if !path.exists() || is_mount_point(target) {
        return Ok(false);
    }

    let drives_dir = drives_dir();
    if path.parent() != Some(drives_dir.as_path()) || path == drives_dir.as_path() {
        return Ok(false);
    }

    if !path.is_dir() {
        return Ok(false);
    }

    if std::fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Ok(false);
    }

    std::fs::remove_dir(path).map_err(|e| e.to_string())?;
    Ok(true)
}

fn run_command_with_timeout(executable: &str, args: &[&str], timeout: Duration) -> bool {
    let mut child = match Command::new(executable)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn wait_for_mount_release(target: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_mount_point(target) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn notify_mount_change(_target: &str, _added: bool) {}

pub fn google_drive_mount_target(_settings: &GoogleDriveSettings) -> String {
    default_google_drive_mount_path()
}

pub fn seedbox_mount_target(_settings: &SeedboxSettings) -> String {
    default_seedbox_mount_path()
}

fn validate_mount_folder_name(name: &str, label: &str) -> Result<(), String> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(format!("{label} name must be a single folder name.")),
    }
}

fn is_mount_point(path: &str) -> bool {
    use std::os::unix::fs::MetadataExt;

    let path_meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let parent = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned());
    let Some(parent) = parent else {
        return true;
    };

    let parent_meta = match std::fs::metadata(&parent) {
        Ok(m) => m,
        Err(_) => return false,
    };

    path_meta.dev() != parent_meta.dev()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_mount_target_uses_default_bucket_path_when_blank() {
        let bucket = BucketMount {
            bucket_name: " photos ".to_string(),
            mount_path: "   ".to_string(),
            drive_letter: String::new(),
        };

        let target = normalize_mount_target(&bucket).unwrap();

        assert!(target.ends_with("/Drives/photos"));
        assert!(target.starts_with('/'));
    }

    #[test]
    fn normalize_mount_target_ignores_custom_path() {
        let bucket = BucketMount {
            bucket_name: "docs".to_string(),
            mount_path: "~/Mounts/docs".to_string(),
            drive_letter: String::new(),
        };

        assert!(normalize_mount_target(&bucket)
            .unwrap()
            .ends_with("/Drives/docs"));
    }

    #[test]
    fn normalize_mount_target_rejects_path_components() {
        for bucket_name in ["../outside", "nested/bucket", "."] {
            let bucket = BucketMount {
                bucket_name: bucket_name.to_string(),
                mount_path: String::new(),
                drive_letter: String::new(),
            };

            assert_eq!(
                normalize_mount_target(&bucket).unwrap_err(),
                "Bucket name must be a single folder name."
            );
        }
    }

    #[test]
    fn normalize_mount_target_rejects_blank_bucket() {
        assert_eq!(
            normalize_mount_target(&BucketMount::default()).unwrap_err(),
            "Bucket name is required."
        );
    }

    #[test]
    fn validate_mount_target_requires_direct_child_of_drives() {
        let drives = drives_dir();
        assert!(validate_mount_target(&drives.join("docs").to_string_lossy()).is_ok());
        assert_eq!(
            validate_mount_target("Volumes/docs").unwrap_err(),
            "Mount folder 'Volumes/docs' is invalid."
        );
        assert_eq!(
            validate_mount_target("/Volumes/docs").unwrap_err(),
            "Mount folder must be directly inside ~/Drives."
        );
        assert_eq!(
            validate_mount_target(&drives.to_string_lossy()).unwrap_err(),
            "Mount folder must be directly inside ~/Drives."
        );
        assert_eq!(
            validate_mount_target(&drives.join("nested/docs").to_string_lossy()).unwrap_err(),
            "Mount folder must be directly inside ~/Drives."
        );
    }

    #[test]
    fn extra_mount_args_marks_macos_mounts_non_browsable() {
        assert_eq!(
            extra_mount_args("/tmp/mount"),
            vec!["--option".to_string(), "nobrowse".to_string()]
        );
        assert_eq!(
            volume_name_args("google-drive"),
            vec!["--volname".to_string(), "google-drive".to_string()]
        );
    }

    #[test]
    fn google_drive_and_seedbox_targets_ignore_custom_mount_paths() {
        assert!(google_drive_mount_target(&GoogleDriveSettings::default())
            .ends_with("/Drives/google-drive"));
        assert!(seedbox_mount_target(&SeedboxSettings::default()).ends_with("/Drives/seedbox"));

        let google = GoogleDriveSettings {
            mount_path: "~/Google".to_string(),
            ..GoogleDriveSettings::default()
        };
        let seedbox = SeedboxSettings {
            mount_path: "/Volumes/Seedbox".to_string(),
            ..SeedboxSettings::default()
        };

        assert!(google_drive_mount_target(&google).ends_with("/Drives/google-drive"));
        assert!(seedbox_mount_target(&seedbox).ends_with("/Drives/seedbox"));
        assert!(validate_mount_target(&google_drive_mount_target(&google)).is_ok());
        assert!(validate_mount_target(&seedbox_mount_target(&seedbox)).is_ok());
    }

    #[test]
    fn mount_readiness_false_for_missing_target() {
        let missing = tempfile::tempdir().unwrap().path().join("missing");
        let missing = missing.to_string_lossy().to_string();

        assert!(!is_mount_ready(&missing));
        assert!(unmount_target(&missing));
    }
}
