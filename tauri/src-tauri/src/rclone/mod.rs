mod config;
mod platform;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::credentials::{delete_google_drive_config, save_google_drive_config};
#[cfg(not(test))]
use crate::credentials::{
    has_saved_google_drive_config, has_saved_seedbox_password, load_google_drive_config,
};
use crate::logging::{redact_sensitive_line, LogEmitter};
use crate::models::{
    B2Credentials, BucketMount, CloudProvider, GoogleDriveSettings, MountRequest, MountState,
    SeedboxSettings, GDRIVE_REMOTE, SEEDBOX_REMOTE,
};
use crate::paths::{
    rclone_cache_dir, rclone_config_path, GOOGLE_DRIVE_MOUNT_NAME, SEEDBOX_MOUNT_NAME,
};
use crate::settings::{ensure_app_data_dir, load_settings};

pub use platform::is_fuse_installed;

const B2_REMOTE: &str = "b2remote";
const MOUNT_STARTUP_TIMEOUT_SECS: u64 = 90;

struct RunningMount {
    child: Child,
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
        mounts.retain(|_, m| match m.child.try_wait() {
            Ok(Some(_)) => false,
            _ => true,
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

        Err(
            "rclone was not found. Bundle rclone with the app or install it with Homebrew."
                .to_string(),
        )
    }

    pub fn mount_all(&self, app: &AppHandle, request: &MountRequest) -> Result<(), String> {
        if !is_fuse_installed() {
            #[cfg(target_os = "macos")]
            return Err("macFUSE is not installed or has not been enabled.".to_string());
            #[cfg(windows)]
            return Err("WinFsp is not installed.".to_string());
            #[cfg(not(any(target_os = "macos", windows)))]
            return Err("FUSE provider is not available on this platform.".to_string());
        }

        let google_drive = request.google_drive.normalized();
        let seedbox = request.seedbox.normalized();
        let has_b2_buckets = has_actionable_b2_buckets(&request.buckets);
        ensure_google_drive_config()?;

        if request.selected_provider == CloudProvider::GoogleDrive
            && !has_b2_buckets
            && !is_google_drive_configured()
        {
            return Err(
                "Google Drive is not connected. Click Connect Google Drive first.".to_string(),
            );
        }

        if request.selected_provider == CloudProvider::Seedbox
            && !has_b2_buckets
            && !is_google_drive_configured()
            && !is_seedbox_configured()
        {
            let password = resolve_seedbox_password(&request.seedbox_password)?;
            if password.is_empty() {
                return Err(
                    "Seedbox is not configured. Enter your FTPS password and click Test Connection first.".to_string(),
                );
            }
        }

        if has_seedbox_settings(&seedbox) {
            if !is_valid_seedbox(&seedbox) {
                return Err("Enter Seedbox host, username, port, and mount target.".to_string());
            }
            let password = resolve_seedbox_password(&request.seedbox_password)?;
            self.configure_seedbox(app, &seedbox, &password)?;
        }

        let specs = build_all_mount_specs(request, &google_drive, &seedbox)?;
        self.unmount_all(app);

        ensure_app_data_dir()?;

        if specs
            .iter()
            .any(|spec| spec.remote_path.starts_with("b2remote:"))
        {
            let key_id = request.application_key_id.trim();
            let key = request.application_key.trim();
            if key_id.is_empty() || key.is_empty() {
                return Err(
                    "Enter your Backblaze B2 Application Key ID and Application Key.".to_string(),
                );
            }
            ensure_b2_config(&B2Credentials {
                application_key_id: key_id.to_string(),
                application_key: key.to_string(),
            })?;
            self.log_info(&format!(
                "B2 rclone config written to: {}",
                rclone_config_path().display()
            ));
        }

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

    pub fn is_google_drive_configured(&self) -> bool {
        is_google_drive_configured()
    }

    pub fn configure_google_drive(
        &self,
        app: &AppHandle,
        google_drive: &GoogleDriveSettings,
    ) -> Result<(), String> {
        let google_drive = google_drive.normalized();
        let rclone_path = self.resolve_rclone_path(app)?;
        ensure_app_data_dir()?;

        let config_path = rclone_config_path();
        config::remove_config_section(&config_path, GDRIVE_REMOTE)?;

        let mut args = vec![
            "config".to_string(),
            "create".to_string(),
            GDRIVE_REMOTE.to_string(),
            "drive".to_string(),
            "scope".to_string(),
            "drive".to_string(),
            "config_is_local".to_string(),
            "true".to_string(),
            "--no-output".to_string(),
        ];

        if !google_drive.root_folder_id.is_empty() {
            args.push("root_folder_id".to_string());
            args.push(google_drive.root_folder_id.clone());
        }

        args.push("--config".to_string());
        args.push(config_path.to_string_lossy().into_owned());

        self.log_info("Starting Google Drive authorization.");
        self.log_info(
            "A browser window should open. Sign in and allow access to complete the rclone setup.",
        );

        let ok = run_rclone_blocking(
            &rclone_path,
            &args,
            self.logger.clone(),
            "Google Drive authorization",
        )?;

        if !ok || !config::has_config_section(&config_path, GDRIVE_REMOTE) {
            let _ = config::remove_config_section(&config_path, GDRIVE_REMOTE);
            return Err("Google Drive authorization failed or was cancelled.".to_string());
        }

        if let Some(lines) = config::read_config_section_lines(&config_path, GDRIVE_REMOTE)? {
            save_google_drive_config(&lines)?;
        }

        self.log_info(&format!(
            "Google Drive is configured in {}.",
            config_path.display()
        ));
        let _ = app;
        Ok(())
    }

    pub fn disconnect_google_drive(
        &self,
        app: &AppHandle,
        google_drive: &GoogleDriveSettings,
    ) -> Result<(), String> {
        let google_drive = google_drive.normalized();
        let target = platform::google_drive_mount_target(&google_drive);
        if !target.is_empty() {
            self.unmount_one(app, &target);
        }

        config::remove_config_section(&rclone_config_path(), GDRIVE_REMOTE)?;
        delete_google_drive_config()?;
        self.log_info("Google Drive remote has been removed from the app rclone config.");
        Ok(())
    }

    pub fn test_google_drive_connection(
        &self,
        app: &AppHandle,
        google_drive: &GoogleDriveSettings,
    ) -> Result<(), String> {
        let google_drive = google_drive.normalized();
        ensure_google_drive_config()?;
        if !is_google_drive_configured() {
            return Err(
                "Google Drive is not connected. Click Connect Google Drive first.".to_string(),
            );
        }

        let rclone_path = self.resolve_rclone_path(app)?;
        let remote_path = build_google_drive_remote_path(&google_drive);
        self.log_info(&format!(
            "Testing Google Drive connection using {remote_path}."
        ));

        let args = vec![
            "lsd".to_string(),
            remote_path,
            "--config".to_string(),
            rclone_config_path().to_string_lossy().into_owned(),
        ];

        let ok = run_rclone_blocking(
            &rclone_path,
            &args,
            self.logger.clone(),
            "Google Drive connection test",
        )?;

        if ok {
            self.log_info("Google Drive connection test completed successfully.");
            Ok(())
        } else {
            Err("Google Drive connection test failed.".to_string())
        }
    }

    pub fn is_seedbox_configured(&self) -> bool {
        is_seedbox_configured()
    }

    pub fn configure_seedbox(
        &self,
        app: &AppHandle,
        seedbox: &SeedboxSettings,
        password: &str,
    ) -> Result<(), String> {
        let seedbox = seedbox.normalized();
        if !is_valid_seedbox(&seedbox) {
            return Err("Enter Seedbox host, username, port, and mount target.".to_string());
        }
        if password.is_empty() {
            if is_seedbox_configured() {
                self.log_info("Using existing Seedbox rclone config.");
                return Ok(());
            }
            return Err("Enter your FTPS password.".to_string());
        }

        let rclone_path = self.resolve_rclone_path(app)?;
        let obscured_password = obscure_password(&rclone_path, password)?;

        let lines = vec![
            "type = ftp".to_string(),
            format!("host = {}", seedbox.host),
            format!("user = {}", seedbox.username),
            format!("port = {}", seedbox.port),
            format!("pass = {obscured_password}"),
            "explicit_tls = true".to_string(),
            "tls = false".to_string(),
            format!(
                "no_check_certificate = {}",
                if seedbox.allow_unverified_certificate {
                    "true"
                } else {
                    "false"
                }
            ),
        ];

        config::upsert_config_section(&rclone_config_path(), SEEDBOX_REMOTE, &lines)?;
        self.log_info(&format!(
            "Seedbox FTPS rclone config written to: {}",
            rclone_config_path().display()
        ));
        Ok(())
    }

    pub fn disconnect_seedbox(
        &self,
        app: &AppHandle,
        seedbox: &SeedboxSettings,
    ) -> Result<(), String> {
        let seedbox = seedbox.normalized();
        let target = platform::seedbox_mount_target(&seedbox);
        if !target.is_empty() {
            self.unmount_one(app, &target);
        }

        config::remove_config_section(&rclone_config_path(), SEEDBOX_REMOTE)?;
        self.log_info("Seedbox remote has been removed from the app rclone config.");
        Ok(())
    }

    pub fn test_seedbox_connection(
        &self,
        app: &AppHandle,
        seedbox: &SeedboxSettings,
        password: &str,
    ) -> Result<(), String> {
        let seedbox = seedbox.normalized();
        self.configure_seedbox(app, &seedbox, password)?;

        let rclone_path = self.resolve_rclone_path(app)?;
        let remote_path = build_seedbox_remote_path(&seedbox);
        self.log_info(&format!(
            "Testing Seedbox FTPS connection using {remote_path}."
        ));

        let args = vec![
            "lsd".to_string(),
            remote_path,
            "--config".to_string(),
            rclone_config_path().to_string_lossy().into_owned(),
        ];

        let ok = run_rclone_blocking(
            &rclone_path,
            &args,
            self.logger.clone(),
            "Seedbox connection test",
        )?;

        if ok {
            self.log_info("Seedbox connection test completed successfully.");
            Ok(())
        } else {
            Err("Seedbox connection test failed.".to_string())
        }
    }

    pub fn unmount_all(&self, app: &AppHandle) {
        let mut targets: Vec<String> = self.mounts.lock().unwrap().keys().cloned().collect();
        targets.extend(configured_mount_targets());
        let mut seen = HashSet::new();
        targets.retain(|target| seen.insert(target.clone()));

        for target in &targets {
            self.unmount_one(app, target);
        }
        self.cleanup_stale_processes(app);
        for target in &targets {
            self.cleanup_mount_folder(target);
        }
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
                if !line.contains(&config_str)
                    && !line.contains(rclone_path.to_string_lossy().as_ref())
                {
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

    fn mount_remote(
        &self,
        app: &AppHandle,
        rclone_path: &Path,
        spec: &MountSpec,
    ) -> Result<(), String> {
        platform::prepare_mount_target(&spec.target)?;
        if platform::is_mount_ready(&spec.target) {
            platform::unmount_target(&spec.target);
            thread::sleep(Duration::from_millis(500));
        }
        if platform::is_mount_ready(&spec.target) {
            return Err(format!(
                "Mount target '{}' is already mounted.",
                spec.target
            ));
        }

        ensure_app_data_dir()?;
        let cache_dir = rclone_cache_dir();
        std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

        let args = build_mount_command_args(spec, &cache_dir);

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

        self.mounts
            .lock()
            .unwrap()
            .insert(target.clone(), RunningMount { child });

        let mounts = self.mounts.clone();
        let intentional = self.intentional_stops.clone();
        let logger = self.logger.clone();
        let app_handle = app.clone();
        let target_watch = target.clone();
        let label_watch = label.clone();
        thread::spawn(move || loop {
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
                        logger.error(format!(
                            "Mount process exited unexpectedly for {label_watch}"
                        ));
                    }
                    let _ = app_handle.emit("mount-state-changed", MountState { mounted: false });
                }
                break;
            }
        });

        Ok(())
    }

    fn unmount_one(&self, app: &AppHandle, target: &str) {
        self.intentional_stops
            .lock()
            .unwrap()
            .insert(target.to_string());
        self.log_info(&format!("Unmounting {target}"));

        if let Some(mut running) = self.mounts.lock().unwrap().remove(target) {
            platform::unmount_target(target);
            let _ = running.child.kill();
            let _ = running.child.wait();
        } else {
            platform::unmount_target(target);
        }

        platform::notify_mount_change(target, false);
        self.cleanup_mount_folder(target);
        let still_mounted = self.is_mounted();
        let _ = app.emit(
            "mount-state-changed",
            MountState {
                mounted: still_mounted,
            },
        );
    }

    fn cleanup_mount_folder(&self, target: &str) {
        match platform::cleanup_mount_target(target) {
            Ok(true) => self.log_info(&format!("Removed mount folder {target}")),
            Ok(false) => {}
            Err(err) => self.log_error(&format!("Could not remove mount folder {target}: {err}")),
        }
    }

    fn log_info(&self, message: &str) {
        if let Ok(logger) = self.logger.lock() {
            logger.info(message);
        }
    }

    fn log_error(&self, message: &str) {
        if let Ok(logger) = self.logger.lock() {
            logger.error(message);
        }
    }
}

#[derive(Clone, Debug)]
struct MountSpec {
    label: String,
    remote_path: String,
    target: String,
    volume_name: String,
    vfs_cache_mode: String,
    read_only: bool,
    extra_args: Vec<String>,
}

fn build_mount_command_args(spec: &MountSpec, cache_dir: &Path) -> Vec<String> {
    let mut args = vec![
        "mount".to_string(),
        spec.remote_path.clone(),
        spec.target.clone(),
        "--config".to_string(),
        rclone_config_path().to_string_lossy().into_owned(),
        "--cache-dir".to_string(),
        cache_dir.to_string_lossy().into_owned(),
        "--vfs-cache-mode".to_string(),
        spec.vfs_cache_mode.clone(),
    ];
    args.extend(platform::volume_name_args(&spec.volume_name));
    args.extend([
        "--links".to_string(),
        "--log-level".to_string(),
        "NOTICE".to_string(),
    ]);
    args.extend(platform::extra_mount_args(&spec.target));
    if spec.read_only {
        args.push("--read-only".to_string());
    }
    args.extend(spec.extra_args.clone());
    args
}

fn has_actionable_b2_buckets(buckets: &[BucketMount]) -> bool {
    buckets
        .iter()
        .any(|bucket| !bucket.bucket_name.trim().is_empty())
}

fn configured_mount_targets() -> Vec<String> {
    let settings = load_settings();
    let mut targets = Vec::new();

    for bucket in settings
        .buckets
        .iter()
        .filter(|bucket| !bucket.bucket_name.trim().is_empty())
    {
        if let Ok(target) = platform::normalize_mount_target(bucket) {
            targets.push(target);
        }
    }

    if is_google_drive_configured() {
        targets.push(platform::google_drive_mount_target(
            &settings.google_drive.normalized(),
        ));
    }

    if has_seedbox_settings(&settings.seedbox) {
        targets.push(platform::seedbox_mount_target(
            &settings.seedbox.normalized(),
        ));
    }

    targets.retain(|target| !target.trim().is_empty());
    targets
}

pub fn is_google_drive_configured() -> bool {
    #[cfg(not(test))]
    {
        has_saved_google_drive_config().unwrap_or(false)
    }

    #[cfg(test)]
    {
        config::has_config_section(&rclone_config_path(), GDRIVE_REMOTE)
    }
}

fn ensure_google_drive_config() -> Result<(), String> {
    #[cfg(not(test))]
    if let Some(lines) = load_google_drive_config()? {
        config::upsert_config_section(&rclone_config_path(), GDRIVE_REMOTE, &lines)?;
    }

    Ok(())
}

fn build_google_drive_remote_path(google_drive: &GoogleDriveSettings) -> String {
    if google_drive.remote_path.is_empty() {
        format!("{GDRIVE_REMOTE}:")
    } else {
        format!("{GDRIVE_REMOTE}:{}", google_drive.remote_path)
    }
}

fn build_google_drive_volume_name(_google_drive: &GoogleDriveSettings) -> String {
    GOOGLE_DRIVE_MOUNT_NAME.to_string()
}

fn build_google_drive_spec(google_drive: &GoogleDriveSettings) -> Result<MountSpec, String> {
    let target = platform::google_drive_mount_target(google_drive);
    if target.is_empty() {
        return Err("Google Drive mount path is missing.".to_string());
    }

    platform::validate_mount_target(&target)?;

    Ok(MountSpec {
        label: "Google Drive".to_string(),
        remote_path: build_google_drive_remote_path(google_drive),
        target,
        volume_name: build_google_drive_volume_name(google_drive),
        vfs_cache_mode: "full".to_string(),
        read_only: false,
        extra_args: Vec::new(),
    })
}

fn build_seedbox_remote_path(seedbox: &SeedboxSettings) -> String {
    if seedbox.remote_path.is_empty() {
        format!("{SEEDBOX_REMOTE}:")
    } else {
        format!("{SEEDBOX_REMOTE}:{}", seedbox.remote_path)
    }
}

fn build_seedbox_spec(seedbox: &SeedboxSettings) -> Result<MountSpec, String> {
    let target = platform::seedbox_mount_target(seedbox);
    if target.is_empty() {
        return Err("Seedbox mount path is missing.".to_string());
    }

    platform::validate_mount_target(&target)?;

    Ok(MountSpec {
        label: "Seedbox".to_string(),
        remote_path: build_seedbox_remote_path(seedbox),
        target,
        volume_name: SEEDBOX_MOUNT_NAME.to_string(),
        vfs_cache_mode: "full".to_string(),
        read_only: seedbox.read_only,
        extra_args: seedbox_large_file_mount_args(),
    })
}

#[cfg(target_os = "macos")]
fn seedbox_large_file_mount_args() -> Vec<String> {
    vec![
        "--buffer-size".to_string(),
        "32M".to_string(),
        "--vfs-read-ahead".to_string(),
        "128M".to_string(),
        "--vfs-read-chunk-size".to_string(),
        "64M".to_string(),
        "--vfs-read-chunk-size-limit".to_string(),
        "1G".to_string(),
        "--multi-thread-cutoff".to_string(),
        "256M".to_string(),
        "--multi-thread-streams".to_string(),
        "4".to_string(),
        "--vfs-fast-fingerprint".to_string(),
    ]
}

#[cfg(not(target_os = "macos"))]
fn seedbox_large_file_mount_args() -> Vec<String> {
    Vec::new()
}

fn has_seedbox_settings(seedbox: &SeedboxSettings) -> bool {
    !seedbox.host.trim().is_empty() || !seedbox.username.trim().is_empty()
}

fn is_valid_seedbox(seedbox: &SeedboxSettings) -> bool {
    let seedbox = seedbox.normalized();
    if seedbox.host.is_empty() || seedbox.username.is_empty() {
        return false;
    }
    if seedbox.port == 0 {
        return false;
    }
    let target = platform::seedbox_mount_target(&seedbox);
    !target.is_empty() && platform::validate_mount_target(&target).is_ok()
}

pub fn is_seedbox_configured() -> bool {
    #[cfg(not(test))]
    {
        has_saved_seedbox_password().unwrap_or(false)
    }

    #[cfg(test)]
    {
        config::has_config_section(&rclone_config_path(), SEEDBOX_REMOTE)
    }
}

fn resolve_seedbox_password(request_password: &str) -> Result<String, String> {
    let trimmed = request_password.trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    Ok(crate::credentials::load_seedbox_password()?.unwrap_or_default())
}

fn build_all_mount_specs(
    request: &MountRequest,
    google_drive: &GoogleDriveSettings,
    seedbox: &SeedboxSettings,
) -> Result<Vec<MountSpec>, String> {
    let mut specs = Vec::new();

    let buckets: Vec<BucketMount> = request
        .buckets
        .iter()
        .filter(|bucket| !bucket.bucket_name.trim().is_empty())
        .cloned()
        .collect();

    if !buckets.is_empty() {
        specs.extend(build_mount_specs(&buckets)?);
    }

    if is_google_drive_configured() {
        specs.push(build_google_drive_spec(google_drive)?);
    }

    if has_seedbox_settings(seedbox) {
        if !is_valid_seedbox(seedbox) {
            return Err("Enter Seedbox host, username, port, and mount target.".to_string());
        }
        if !is_seedbox_configured() {
            return Err(
                "Seedbox is not configured. Enter your FTPS password and click Test Connection first.".to_string(),
            );
        }
        specs.push(build_seedbox_spec(seedbox)?);
    }

    if specs.is_empty() {
        return Err(
            "Nothing to mount. Configure Backblaze B2, connect Google Drive, or configure Seedbox."
                .to_string(),
        );
    }

    Ok(specs)
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
            remote_path: format!("{B2_REMOTE}:{bucket_name}"),
            target,
            volume_name: bucket_name,
            vfs_cache_mode: "writes".to_string(),
            read_only: false,
            extra_args: Vec::new(),
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
        for candidate in [
            "/opt/homebrew/bin/rclone",
            "/usr/local/bin/rclone",
            "/usr/bin/rclone",
        ] {
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
    if creds
        .as_ref()
        .map(|c| c.application_key_id.trim().is_empty() || c.application_key.trim().is_empty())
        .unwrap_or(true)
    {
        return false;
    }
    buckets
        .iter()
        .any(|b| !b.bucket_name.trim().is_empty() && platform::normalize_mount_target(b).is_ok())
}

pub fn has_complete_google_drive_config() -> bool {
    is_google_drive_configured()
}

pub fn has_complete_seedbox_config() -> bool {
    let settings = crate::settings::load_settings();
    let seedbox = settings.seedbox.normalized();
    is_seedbox_configured() && is_valid_seedbox(&seedbox)
}

fn obscure_password(rclone_path: &Path, password: &str) -> Result<String, String> {
    use std::io::Write;

    let mut child = Command::new(rclone_path)
        .args(["obscure", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start rclone obscure: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(password.as_bytes())
            .map_err(|e| format!("Failed to write Seedbox password to rclone: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for rclone obscure: {e}"))?;

    if !output.status.success() {
        return Err("Could not prepare the Seedbox password for rclone.".to_string());
    }

    let obscured = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if obscured.is_empty() {
        return Err("Could not prepare the Seedbox password for rclone.".to_string());
    }

    Ok(obscured)
}

fn run_rclone_blocking(
    rclone_path: &Path,
    args: &[String],
    logger: Arc<Mutex<LogEmitter>>,
    label: &str,
) -> Result<bool, String> {
    let mut child = Command::new(rclone_path)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start rclone: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let logger_out = logger.clone();
    let label_out = label.to_string();
    thread::spawn(move || stream_output(stdout, stderr, logger_out, label_out));

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for rclone: {e}"))?;
    if !status.success() {
        if let Ok(logger) = logger.lock() {
            logger.error(format!("{label} failed."));
        }
    }
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;

    fn empty_request() -> MountRequest {
        MountRequest {
            application_key_id: String::new(),
            application_key: String::new(),
            buckets: Vec::new(),
            google_drive: GoogleDriveSettings::default(),
            seedbox: SeedboxSettings::default(),
            seedbox_password: String::new(),
            selected_provider: CloudProvider::BackblazeB2,
        }
    }

    #[cfg(any(target_os = "macos", windows))]
    fn mountable_bucket(name: &str, target: &str) -> BucketMount {
        #[cfg(target_os = "macos")]
        {
            BucketMount {
                bucket_name: name.to_string(),
                mount_path: target.to_string(),
                drive_letter: String::new(),
            }
        }
        #[cfg(windows)]
        {
            BucketMount {
                bucket_name: name.to_string(),
                mount_path: String::new(),
                drive_letter: target.to_string(),
            }
        }
    }

    #[cfg(windows)]
    fn expected_target(target: &str) -> String {
        format!("{}:", target.trim().trim_end_matches(':').to_uppercase())
    }

    #[cfg(any(target_os = "macos", windows))]
    fn valid_google_drive_settings(target: &str) -> GoogleDriveSettings {
        #[cfg(target_os = "macos")]
        {
            GoogleDriveSettings {
                mount_path: target.to_string(),
                ..GoogleDriveSettings::default()
            }
        }
        #[cfg(windows)]
        {
            let _ = target;
            GoogleDriveSettings::default()
        }
    }

    #[cfg(any(target_os = "macos", windows))]
    fn valid_seedbox_settings(target: &str) -> SeedboxSettings {
        #[cfg(target_os = "macos")]
        {
            SeedboxSettings {
                host: "seedbox.example.com".to_string(),
                username: "user".to_string(),
                mount_path: target.to_string(),
                ..SeedboxSettings::default()
            }
        }
        #[cfg(windows)]
        {
            let _ = target;
            SeedboxSettings {
                host: "seedbox.example.com".to_string(),
                username: "user".to_string(),
                ..SeedboxSettings::default()
            }
        }
    }

    #[test]
    fn b2_bucket_presence_ignores_blank_names() {
        assert!(!has_actionable_b2_buckets(&[]));
        assert!(!has_actionable_b2_buckets(&[BucketMount {
            bucket_name: "   ".to_string(),
            mount_path: "/tmp/blank".to_string(),
            drive_letter: "P".to_string(),
        }]));
        assert!(has_actionable_b2_buckets(&[BucketMount {
            bucket_name: "photos".to_string(),
            mount_path: String::new(),
            drive_letter: String::new(),
        }]));
    }

    #[test]
    fn google_drive_remote_path_and_volume_name_follow_remote_path() {
        let root = GoogleDriveSettings::default().normalized();
        assert_eq!(build_google_drive_remote_path(&root), "gdrive:");
        assert_eq!(build_google_drive_volume_name(&root), "google-drive");

        let nested = GoogleDriveSettings {
            remote_path: "Team/Docs".to_string(),
            ..GoogleDriveSettings::default()
        }
        .normalized();
        assert_eq!(build_google_drive_remote_path(&nested), "gdrive:Team/Docs");
        assert_eq!(build_google_drive_volume_name(&nested), "google-drive");
    }

    #[test]
    fn seedbox_remote_path_uses_remote_root_or_nested_path() {
        assert_eq!(
            build_seedbox_remote_path(&SeedboxSettings {
                remote_path: String::new(),
                ..SeedboxSettings::default()
            }),
            "seedbox:"
        );
        assert_eq!(
            build_seedbox_remote_path(&SeedboxSettings {
                remote_path: "downloads/movies".to_string(),
                ..SeedboxSettings::default()
            }),
            "seedbox:downloads/movies"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_mount_command_args_mount_as_hidden_folder_not_finder_volume() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();
        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        let spec = MountSpec {
            label: "B2 nocdn-main".to_string(),
            remote_path: "b2remote:nocdn-main".to_string(),
            target: crate::paths::default_bucket_mount_path("nocdn-main"),
            volume_name: "b2 nocdn-main".to_string(),
            vfs_cache_mode: "writes".to_string(),
            read_only: false,
            extra_args: Vec::new(),
        };

        let args = build_mount_command_args(&spec, &temp.path().join("cache"));

        assert!(!args.iter().any(|arg| arg == "--volname"));
        assert!(!args.iter().any(|arg| arg == "b2 nocdn-main"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--option" && pair[1] == "nobrowse"));

        crate::test_support::clear_test_dirs();
    }

    #[cfg(windows)]
    #[test]
    fn windows_mount_specs_and_args_use_named_drive_letters() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();
        let temp = tempfile::tempdir().unwrap();
        let app_data = temp.path().join("app");
        crate::test_support::set_test_dirs(&app_data, &temp.path().join("logs"));

        config::upsert_config_section(
            &rclone_config_path(),
            GDRIVE_REMOTE,
            &["type = drive".to_string()],
        )
        .unwrap();
        config::upsert_config_section(
            &rclone_config_path(),
            SEEDBOX_REMOTE,
            &["type = ftp".to_string()],
        )
        .unwrap();

        let google = GoogleDriveSettings::default().normalized();
        let seedbox = valid_seedbox_settings("").normalized();
        let request = MountRequest {
            buckets: vec![mountable_bucket("photos", "P")],
            google_drive: google.clone(),
            seedbox: seedbox.clone(),
            ..empty_request()
        };

        let specs = build_all_mount_specs(&request, &google, &seedbox).unwrap();
        assert_eq!(specs[0].target, "P:");
        assert_eq!(specs[0].volume_name, "photos");
        assert_eq!(specs[1].target, "G:");
        assert_eq!(specs[1].volume_name, "google-drive");
        assert_eq!(specs[2].target, "S:");
        assert_eq!(specs[2].volume_name, "seedbox");

        for spec in &specs {
            let args = build_mount_command_args(spec, &temp.path().join("cache"));
            assert!(args.iter().any(|arg| arg == &spec.target));
            assert!(args.windows(2).any(|pair| pair[0].as_str() == "--volname"
                && pair[1].as_str() == spec.volume_name.as_str()));
            assert!(!args.iter().any(|arg| arg == "nobrowse"));
        }

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn resolve_seedbox_password_prefers_trimmed_request_password() {
        assert_eq!(
            resolve_seedbox_password("  super-secret  ").unwrap(),
            "super-secret"
        );
    }

    #[test]
    fn complete_b2_config_requires_real_credentials_and_mount_target() {
        let bucket_with_mount = BucketMount {
            bucket_name: "photos".to_string(),
            mount_path: "/Volumes/photos".to_string(),
            drive_letter: String::new(),
        };
        let bucket_with_drive = BucketMount {
            bucket_name: "docs".to_string(),
            mount_path: String::new(),
            drive_letter: "P".to_string(),
        };
        let creds = Some(B2Credentials {
            application_key_id: "id".to_string(),
            application_key: "key".to_string(),
        });

        assert!(!has_complete_b2_config(&None, &[bucket_with_mount.clone()]));
        assert!(!has_complete_b2_config(
            &Some(B2Credentials {
                application_key_id: "   ".to_string(),
                application_key: "key".to_string(),
            }),
            &[bucket_with_mount.clone()]
        ));
        #[cfg(windows)]
        assert!(!has_complete_b2_config(
            &creds,
            &[BucketMount {
                bucket_name: "photos".to_string(),
                mount_path: "  ".to_string(),
                drive_letter: "  ".to_string(),
            }]
        ));
        #[cfg(target_os = "macos")]
        assert!(has_complete_b2_config(
            &creds,
            &[BucketMount {
                bucket_name: "photos".to_string(),
                mount_path: "  ".to_string(),
                drive_letter: "  ".to_string(),
            }]
        ));
        assert!(has_complete_b2_config(&creds, &[bucket_with_mount]));
        assert!(has_complete_b2_config(&creds, &[bucket_with_drive]));
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn build_mount_specs_builds_b2_specs_and_ignores_whitespace() {
        #[cfg(target_os = "macos")]
        let target = "/tmp/photos";
        #[cfg(windows)]
        let target = "P";
        let bucket = mountable_bucket(" photos ", target);

        let specs = build_mount_specs(&[bucket]).unwrap();

        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].label, "B2 photos");
        assert_eq!(specs[0].remote_path, "b2remote:photos");
        #[cfg(target_os = "macos")]
        assert!(specs[0].target.ends_with("/Drives/photos"));
        #[cfg(windows)]
        assert_eq!(specs[0].target, expected_target(target));
        assert_eq!(specs[0].volume_name, "photos");
        assert_eq!(specs[0].vfs_cache_mode, "writes");
        assert!(!specs[0].read_only);
        assert!(specs[0].extra_args.is_empty());
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn build_mount_specs_rejects_duplicate_buckets_case_insensitively() {
        #[cfg(target_os = "macos")]
        let targets = ("/tmp/photos-a", "/tmp/photos-b");
        #[cfg(windows)]
        let targets = ("P", "Q");

        let err = build_mount_specs(&[
            mountable_bucket("Photos", targets.0),
            mountable_bucket("photos", targets.1),
        ])
        .unwrap_err();

        assert_eq!(err, "Bucket 'photos' is listed more than once.");
    }

    #[cfg(windows)]
    #[test]
    fn build_mount_specs_rejects_duplicate_targets_case_insensitively() {
        let targets = ("P", "p");

        let err = build_mount_specs(&[
            mountable_bucket("photos", targets.0),
            mountable_bucket("docs", targets.1),
        ])
        .unwrap_err();

        assert!(err.contains("is used more than once"));
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn build_all_mount_specs_returns_error_when_nothing_is_configured() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();
        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        let err = build_all_mount_specs(
            &empty_request(),
            &GoogleDriveSettings::default(),
            &SeedboxSettings::default(),
        )
        .unwrap_err();

        assert_eq!(
            err,
            "Nothing to mount. Configure Backblaze B2, connect Google Drive, or configure Seedbox."
        );

        crate::test_support::clear_test_dirs();
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn build_all_mount_specs_combines_b2_google_drive_and_seedbox() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();
        let temp = tempfile::tempdir().unwrap();
        let app_data = temp.path().join("app");
        crate::test_support::set_test_dirs(&app_data, &temp.path().join("logs"));

        config::upsert_config_section(
            &rclone_config_path(),
            GDRIVE_REMOTE,
            &["type = drive".to_string()],
        )
        .unwrap();
        config::upsert_config_section(
            &rclone_config_path(),
            SEEDBOX_REMOTE,
            &["type = ftp".to_string()],
        )
        .unwrap();

        let google_target = temp.path().join("google").to_string_lossy().into_owned();
        let seedbox_target = temp.path().join("seedbox").to_string_lossy().into_owned();
        let google = GoogleDriveSettings {
            remote_path: "Team Docs".to_string(),
            ..valid_google_drive_settings(&google_target)
        }
        .normalized();
        let seedbox = SeedboxSettings {
            remote_path: "downloads/movies".to_string(),
            read_only: true,
            ..valid_seedbox_settings(&seedbox_target)
        }
        .normalized();
        #[cfg(target_os = "macos")]
        let b2_target = "/tmp/photos";
        #[cfg(windows)]
        let b2_target = "P";
        let request = MountRequest {
            buckets: vec![mountable_bucket("photos", b2_target)],
            google_drive: google.clone(),
            seedbox: seedbox.clone(),
            ..empty_request()
        };

        let specs = build_all_mount_specs(&request, &google, &seedbox).unwrap();

        assert_eq!(specs.len(), 3);
        assert_eq!(specs[0].label, "B2 photos");
        assert_eq!(specs[0].remote_path, "b2remote:photos");
        assert_eq!(specs[1].label, "Google Drive");
        assert_eq!(specs[1].remote_path, "gdrive:Team Docs");
        assert_eq!(specs[1].vfs_cache_mode, "full");
        assert!(!specs[1].read_only);
        assert_eq!(specs[2].label, "Seedbox");
        assert_eq!(specs[2].remote_path, "seedbox:downloads/movies");
        assert!(specs[2].read_only);

        crate::test_support::clear_test_dirs();
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn build_all_mount_specs_rejects_seedbox_settings_without_config() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();
        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        let seedbox_target = temp.path().join("seedbox").to_string_lossy().into_owned();
        let seedbox = valid_seedbox_settings(&seedbox_target).normalized();
        let request = MountRequest {
            seedbox: seedbox.clone(),
            selected_provider: CloudProvider::Seedbox,
            ..empty_request()
        };

        let err =
            build_all_mount_specs(&request, &GoogleDriveSettings::default(), &seedbox).unwrap_err();

        assert_eq!(
            err,
            "Seedbox is not configured. Enter your FTPS password and click Test Connection first."
        );

        crate::test_support::clear_test_dirs();
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn complete_seedbox_config_requires_saved_settings_and_rclone_section() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();
        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        assert!(!has_complete_seedbox_config());

        config::upsert_config_section(
            &rclone_config_path(),
            SEEDBOX_REMOTE,
            &["type = ftp".to_string()],
        )
        .unwrap();
        assert!(!has_complete_seedbox_config());

        let seedbox_target = temp.path().join("seedbox").to_string_lossy().into_owned();
        crate::settings::save_settings(&AppSettings {
            seedbox: valid_seedbox_settings(&seedbox_target),
            ..AppSettings::default()
        })
        .unwrap();

        assert!(has_complete_seedbox_config());

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn ensure_b2_config_upserts_expected_remote() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();
        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        ensure_b2_config(&B2Credentials {
            application_key_id: "account-id".to_string(),
            application_key: "application-key".to_string(),
        })
        .unwrap();

        let config = std::fs::read_to_string(rclone_config_path()).unwrap();
        assert!(config.contains("[b2remote]"));
        assert!(config.contains("type = b2"));
        assert!(config.contains("account = account-id"));
        assert!(config.contains("key = application-key"));

        crate::test_support::clear_test_dirs();
    }
}
