use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::models::BucketMount;
use crate::models::GoogleDriveSettings;
use crate::models::SeedboxSettings;
use crate::paths::{default_bucket_mount_path, default_google_drive_mount_path, default_seedbox_mount_path, expand_path};

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

    let mount_path = if bucket.mount_path.trim().is_empty() {
        default_bucket_mount_path(bucket_name)
    } else {
        expand_path(&bucket.mount_path)
    };

    if !mount_path.starts_with('/') {
        return Err(format!("Mount folder '{mount_path}' is invalid."));
    }

    Ok(mount_path)
}

pub fn validate_mount_target(target: &str) -> Result<(), String> {
    if !target.starts_with('/') {
        return Err(format!("Mount folder '{target}' is invalid."));
    }
    Ok(())
}

pub fn prepare_mount_target(target: &str) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| e.to_string())
}

pub fn extra_mount_args(_target: &str) -> Vec<String> {
    vec![]
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

pub fn google_drive_mount_target(settings: &GoogleDriveSettings) -> String {
    if settings.mount_path.trim().is_empty() {
        default_google_drive_mount_path()
    } else {
        expand_path(&settings.mount_path)
    }
}

pub fn seedbox_mount_target(settings: &SeedboxSettings) -> String {
    if settings.mount_path.trim().is_empty() {
        default_seedbox_mount_path()
    } else {
        expand_path(&settings.mount_path)
    }
}

fn is_mount_point(path: &str) -> bool {
    use std::os::unix::fs::MetadataExt;

    let path_meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let parent = Path::new(path).parent().map(|p| p.to_string_lossy().into_owned());
    let Some(parent) = parent else {
        return true;
    };

    let parent_meta = match std::fs::metadata(&parent) {
        Ok(m) => m,
        Err(_) => return false,
    };

    path_meta.dev() != parent_meta.dev()
}
