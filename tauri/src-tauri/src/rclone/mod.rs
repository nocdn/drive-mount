mod config;
mod platform;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::logging::{redact_sensitive_line, LogEmitter};
use crate::models::{B2Credentials, BucketMount, MountRequest, MountState};
use crate::paths::{rclone_cache_dir, rclone_config_path};
use crate::settings::ensure_app_data_dir;

pub use platform::is_fuse_installed;

const B2_REMOTE: &str = "b2remote";
const MOUNT_STARTUP_TIMEOUT_SECS: u64 = 90;

struct RunningMount {
    child: Child,
    label: String,
}

pub struct RcloneManager {
    mounts: Arc<Mutex<HashMap<String, RunningMount>>>,
    intentional_stops: Arc<Mutex<HashSet<String>>>,
    logger: Arc<Mutex<LogEmitter>>,
    rclone_path: Mutex<Option<PathBuf>>,
}

impl RcloneManager {
    pub fn new(logger: Arc<Mutex<LogEmitter>>) -> Self {
        Self {
            mounts: Arc::new(Mutex::new(HashMap::new())),
            intentional_stops: Arc::new(Mutex::new(HashSet::new())),
            logger,
            rclone_path: Mutex::new(None),
        }
    }

    pub fn is_mounted(&self) -> bool {
        let mut mounts = self.mounts.lock().unwrap();
        mounts.retain(|_, m| {
            match m.child.try_wait() {
                Ok(Some(_)) => false,
                _ => true,
            }
        });
        !mounts.is_empty()
    }

    pub fn resolve_rclone_path(&self, app: &AppHandle) -> Result<PathBuf, String> {
        if let Some(path) = self.rclone_path.lock().unwrap().clone() {
            if path.exists() {
                return Ok(path);
            }
        }

        if let Some(path) = find_bundled_rclone(app) {
            *self.rclone_path.lock().unwrap() = Some(path.clone());
            return Ok(path);
        }

        if let Some(path) = find_rclone_in_path() {
            *self.rclone_path.lock().unwrap() = Some(path.clone());
            return Ok(path);
        }

        Err("rclone was not found. Bundle rclone with the app or install it with Homebrew.".to_string())
    }

    pub fn mount_b2(&self, app: &AppHandle, request: &MountRequest) -> Result<(), String> {
        if !is_fuse_installed() {
            #[cfg(target_os = "macos")]
            return Err("macFUSE is not installed or has not been enabled.".to_string());
            #[cfg(windows)]
            return Err("WinFsp is not installed.".to_string());
            #[cfg(not(any(target_os = "macos", windows)))]
            return Err("FUSE provider is not available on this platform.".to_string());
        }

        let key_id = request.application_key_id.trim();
        let key = request.application_key.trim();
        if key_id.is_empty() || key.is_empty() {
            return Err("Enter your Backblaze B2 Application Key ID and Application Key.".to_string());
        }

        let buckets: Vec<BucketMount> = request
            .buckets
            .iter()
            .filter(|b| !b.bucket_name.trim().is_empty() || !b.mount_path.trim().is_empty() || !b.drive_letter.trim().is_empty())
            .cloned()
            .collect();

        if buckets.is_empty() {
            return Err("Add at least one bucket.".to_string());
        }

        let specs = build_mount_specs(&buckets)?;
        self.unmount_all(app);

        ensure_app_data_dir()?;
        ensure_b2_config(&B2Credentials {
            application_key_id: key_id.to_string(),
            application_key: key.to_string(),
        })?;

        self.log_info(&format!("B2 rclone config written to: {}", rclone_config_path().display()));

        let rclone_path = self.resolve_rclone_path(app)?;
        let mut started = Vec::new();

        for spec in &specs {
            match self.mount_remote(app, &rclone_path, spec) {
                Ok(()) => started.push(spec.clone()),
                Err(err) => {
                    for started_spec in started.iter().rev() {
                        self.unmount_one(app, &started_spec.target);
                    }
                    return Err(err);
                }
            }
        }

        let _ = app.emit("mount-state-changed", MountState { mounted: true });
        Ok(())
    }

    pub fn unmount_all(&self, app: &AppHandle) {
        let targets: Vec<String> = self.mounts.lock().unwrap().keys().cloned().collect();
        for target in targets {
            self.unmount_one(app, &target);
        }
        self.cleanup_stale_processes(app);
        let _ = app.emit("mount-state-changed", MountState { mounted: false });
    }

    pub fn cleanup_stale_processes(&self, app: &AppHandle) {
        let Ok(rclone_path) = self.resolve_rclone_path(app) else {
            return;
        };

        let config = rclone_config_path();
        let config_str = config.to_string_lossy().to_string();

        if let Ok(output) = Command::new("ps").args(["-axo", "pid=,command="]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if !line.contains("rclone") || !line.contains("mount") {
                    continue;
                }
                if !line.contains(&config_str) && !line.contains(rclone_path.to_string_lossy().as_ref()) {
                    continue;
                }
                if let Some(pid_str) = line.split_whitespace().next() {
                    if let Ok(pid) = pid_str.parse::<i32>() {
                        let _ = Command::new("kill").arg(pid.to_string()).status();
                    }
                }
            }
        }
    }

    fn mount_remote(&self, app: &AppHandle, rclone_path: &Path, spec: &MountSpec) -> Result<(), String> {
        platform::prepare_mount_target(&spec.target)?;
        if platform::is_mount_ready(&spec.target) {
            platform::unmount_target(&spec.target);
            thread::sleep(Duration::from_millis(500));
        }
        if platform::is_mount_ready(&spec.target) {
            return Err(format!("Mount target '{}' is already mounted.", spec.target));
        }

        ensure_app_data_dir()?;
        let cache_dir = rclone_cache_dir();
        std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

        let mut args = vec![
            "mount".to_string(),
            format!("{B2_REMOTE}:{}", spec.bucket_name),
            spec.target.clone(),
            "--config".to_string(),
            rclone_config_path().to_string_lossy().into_owned(),
            "--cache-dir".to_string(),
            cache_dir.to_string_lossy().into_owned(),
            "--vfs-cache-mode".to_string(),
            "writes".to_string(),
            "--volname".to_string(),
            spec.bucket_name.clone(),
            "--links".to_string(),
            "--log-level".to_string(),
            "NOTICE".to_string(),
        ];
        args.extend(platform::extra_mount_args(&spec.target));

        self.log_info(&format!("Mounting {} at {}", spec.label, spec.target));

        let mut child = Command::new(rclone_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start rclone: {e}"))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let logger = self.logger.clone();
        let label = spec.label.clone();
        thread::spawn(move || stream_output(stdout, stderr, logger, label));

        let target = spec.target.clone();
        let label = spec.label.clone();

        if !platform::wait_for_mount_ready(&target, MOUNT_STARTUP_TIMEOUT_SECS) {
            let _ = child.kill();
            let _ = child.wait();
            platform::unmount_target(&target);
            return Err(format!(
                "{label} did not finish mounting in time. The partial mount was cleaned up."
            ));
        }

        platform::notify_mount_change(&target, true);
        self.log_info(&format!("Mount startup completed for {label}"));

        self.mounts.lock().unwrap().insert(
            target.clone(),
            RunningMount {
                child,
                label: label.clone(),
            },
        );

        let mounts = self.mounts.clone();
        let intentional = self.intentional_stops.clone();
        let logger = self.logger.clone();
        let app_handle = app.clone();
        let target_watch = target.clone();
        let label_watch = label.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(500));
                let exited = {
                    let mut map = mounts.lock().unwrap();
                    if let Some(running) = map.get_mut(&target_watch) {
                        matches!(running.child.try_wait(), Ok(Some(_)))
                    } else {
                        return;
                    }
                };
                if exited {
                    mounts.lock().unwrap().remove(&target_watch);
                    if !intentional.lock().unwrap().remove(&target_watch) {
                        if let Ok(logger) = logger.lock() {
                            logger.error(format!("Mount process exited unexpectedly for {label_watch}"));
                        }
                        let _ = app_handle.emit("mount-state-changed", MountState { mounted: false });
                    }
                    break;
                }
            }
        });

        Ok(())
    }

    fn unmount_one(&self, app: &AppHandle, target: &str) {
        self.intentional_stops.lock().unwrap().insert(target.to_string());
        self.log_info(&format!("Unmounting {target}"));

        if let Some(mut running) = self.mounts.lock().unwrap().remove(target) {
            platform::unmount_target(target);
            let _ = running.child.kill();
            let _ = running.child.wait();
        } else {
            platform::unmount_target(target);
        }

        platform::notify_mount_change(target, false);
        let still_mounted = self.is_mounted();
        let _ = app.emit(
            "mount-state-changed",
            MountState {
                mounted: still_mounted,
            },
        );
    }

    fn log_info(&self, message: &str) {
        if let Ok(logger) = self.logger.lock() {
            logger.info(message);
        }
    }
}

#[derive(Clone)]
struct MountSpec {
    label: String,
    bucket_name: String,
    target: String,
}

fn build_mount_specs(buckets: &[BucketMount]) -> Result<Vec<MountSpec>, String> {
    let mut specs = Vec::new();
    let mut seen_targets = HashSet::new();
    let mut seen_buckets = HashSet::new();

    for bucket in buckets {
        let bucket_name = bucket.bucket_name.trim().to_string();
        if bucket_name.is_empty() {
            return Err("Each bucket row needs a bucket name.".to_string());
        }

        let bucket_key = bucket_name.to_lowercase();
        if !seen_buckets.insert(bucket_key) {
            return Err(format!("Bucket '{bucket_name}' is listed more than once."));
        }

        let target = platform::normalize_mount_target(bucket)?;
        platform::validate_mount_target(&target)?;

        let target_key = target.to_lowercase();
        if !seen_targets.insert(target_key) {
            return Err(format!("Mount target '{target}' is used more than once."));
        }

        specs.push(MountSpec {
            label: format!("B2 {bucket_name}"),
            bucket_name,
            target,
        });
    }

    Ok(specs)
}

fn ensure_b2_config(credentials: &B2Credentials) -> Result<(), String> {
    let lines = vec![
        "type = b2".to_string(),
        format!("account = {}", credentials.application_key_id),
        format!("key = {}", credentials.application_key),
    ];
    config::upsert_config_section(&rclone_config_path(), B2_REMOTE, &lines)
}

fn find_bundled_rclone(app: &AppHandle) -> Option<PathBuf> {
    for name in sidecar_filenames() {
        if let Ok(resource_dir) = app.path().resource_dir() {
            let candidate = resource_dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn sidecar_filenames() -> &'static [&'static str] {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        &["rclone-aarch64-apple-darwin"]
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        &["rclone-x86_64-apple-darwin"]
    }
    #[cfg(windows)]
    {
        &["rclone-x86_64-pc-windows-msvc.exe"]
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        windows
    )))]
    {
        &[] as &[&str]
    }
}

fn find_rclone_in_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let name = "rclone.exe";
    #[cfg(not(windows))]
    let name = "rclone";

    if let Ok(path_var) = std::env::var("PATH") {
        #[cfg(windows)]
        let sep = ';';
        #[cfg(not(windows))]
        let sep = ':';

        for dir in path_var.split(sep) {
            let candidate = PathBuf::from(dir.trim()).join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for candidate in ["/opt/homebrew/bin/rclone", "/usr/local/bin/rclone", "/usr/bin/rclone"] {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

fn stream_output(
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    logger: Arc<Mutex<LogEmitter>>,
    label: String,
) {
    if let Some(out) = stdout {
        let logger = logger.clone();
        let label = label.clone();
        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(out);
            for line in reader.lines().map_while(Result::ok) {
                let line = redact_sensitive_line(&line);
                if let Ok(logger) = logger.lock() {
                    logger.info(format!("[{label}] {line}"));
                }
            }
        });
    }
    if let Some(err) = stderr {
        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(err);
            for line in reader.lines().map_while(Result::ok) {
                let line = redact_sensitive_line(&line);
                if let Ok(logger) = logger.lock() {
                    logger.info(format!("[{label}] {line}"));
                }
            }
        });
    }
}

pub fn has_complete_b2_config(creds: &Option<B2Credentials>, buckets: &[BucketMount]) -> bool {
    if creds.as_ref().map(|c| c.application_key_id.is_empty() || c.application_key.is_empty()).unwrap_or(true) {
        return false;
    }
    buckets.iter().any(|b| {
        !b.bucket_name.trim().is_empty()
            && (!b.mount_path.trim().is_empty() || !b.drive_letter.trim().is_empty())
    })
}
