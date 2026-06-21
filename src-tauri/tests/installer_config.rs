use serde_json::Value;
use std::{fs, path::PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: &str) -> Value {
    let path = manifest_dir().join(path);
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn read_repo_file(path: &str) -> String {
    let path = manifest_dir().join("..").join(path);
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    contents.replace("\r\n", "\n")
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("expected an array")
        .iter()
        .map(|item| item.as_str().expect("expected a string"))
        .collect()
}

#[test]
fn base_bundle_config_enables_installers_and_rclone_sidecar() {
    let config = read_json("tauri.conf.json");
    let bundle = &config["bundle"];

    assert_eq!(bundle["active"], true);
    assert_eq!(
        string_array(&bundle["externalBin"]),
        vec!["binaries/rclone"]
    );

    let icons = string_array(&bundle["icon"]);
    assert!(icons.contains(&"icons/icon.icns"));
    assert!(icons.contains(&"icons/icon.ico"));
}

#[test]
fn platform_bundle_configs_select_dmg_and_msi_only() {
    let macos = read_json("tauri.macos.conf.json");
    assert_eq!(string_array(&macos["bundle"]["targets"]), vec!["dmg"]);
    assert_eq!(macos["bundle"]["macOS"]["signingIdentity"], "-");

    let windows = read_json("tauri.windows.conf.json");
    let windows_targets = string_array(&windows["bundle"]["targets"]);
    assert_eq!(windows_targets, vec!["msi"]);
    assert!(!windows_targets.contains(&"nsis"));
    assert_eq!(
        string_array(&windows["bundle"]["windows"]["wix"]["language"]),
        vec!["en-US"]
    );
}

#[test]
fn package_scripts_build_installers_through_tauri() {
    let package = read_json("../package.json");
    let scripts = &package["scripts"];

    assert_eq!(
        scripts["prepare:sidecars"],
        "bun scripts/download-rclone.mjs"
    );
    assert_eq!(
        scripts["build"],
        "bun run prepare:sidecars && bun run build:frontend"
    );
    assert_eq!(scripts["build:installer"], "tauri build");
    assert_eq!(scripts["build:installer:mac"], "tauri build --bundles dmg");
    assert_eq!(
        scripts["build:installer:windows"],
        "tauri build --bundles msi"
    );
    assert_eq!(
        scripts["build:installer:open"],
        "bun scripts/build-open-installer.mjs"
    );
}

#[test]
fn release_workflow_uses_tauri_installers_only() {
    let workflow = read_repo_file(".github/workflows/build-release.yml");

    assert!(workflow.contains("tauri-apps/tauri-action@v0.6.2"));
    assert!(workflow.contains("Prepare Tauri sidecars"));
    assert!(workflow.contains("TAURI_ENV_TARGET_TRIPLE: ${{ matrix.rust_target }}"));
    assert!(workflow.contains("--bundles dmg"));
    assert!(workflow.contains("--bundles msi"));
    assert!(workflow.contains("actions/upload-artifact@v7"));
    assert!(workflow.contains("actions/download-artifact@v8"));
    assert!(workflow.contains("release/*.dmg"));
    assert!(workflow.contains("release/*.msi"));
    assert!(workflow.contains("bundles_file=\"$(mktemp)\""));
    assert!(workflow.contains("bundle_count=\"$(wc -l"));
    assert!(workflow.contains("runner.os"));
    assert!(workflow.contains("cargo test --locked --no-run"));
    assert!(workflow.contains("cargo test --locked --test installer_config"));

    assert!(!workflow.contains("win/installer"));
    assert!(!workflow.contains("mac/CloudDriveMount"));
    assert!(!workflow.contains("release/*.zip"));
    assert!(!workflow.contains("release/*.exe"));
    assert!(!workflow.contains("mapfile"));
    assert!(!workflow.contains("actions/upload-artifact@v4"));
    assert!(!workflow.contains("actions/download-artifact@v5"));

    let sidecar_step = workflow.find("Prepare Tauri sidecars").unwrap();
    let test_step = workflow.find("Run Tauri tests").unwrap();
    assert!(
        sidecar_step < test_step,
        "sidecars must be prepared before cargo test because Tauri validates externalBin paths"
    );
}

#[test]
fn sidecar_downloader_supports_ci_targets_without_powershell_args() {
    let script = read_repo_file("scripts/download-rclone.mjs");

    assert!(script.contains("rclone-aarch64-apple-darwin"));
    assert!(script.contains("rclone-x86_64-apple-darwin"));
    assert!(script.contains("rclone-x86_64-pc-windows-msvc.exe"));
    assert!(script.contains("powerShellSingleQuoted"));
    assert!(script.contains("Expand-Archive"));
    assert!(!script.contains("$args[0]"));
    assert!(!script.contains("$args[1]"));
}

#[test]
fn installer_open_helper_builds_and_opens_latest_platform_installer() {
    let script = read_repo_file("scripts/build-open-installer.mjs");

    assert!(script
        .contains(r#"`build:installer:${process.platform === "darwin" ? "mac" : "windows"}`"#));
    assert!(script.contains(r#"? "mac" : "windows""#));
    assert!(script.contains("src-tauri\", \"target\", \"release\", \"bundle"));
    assert!(script.contains("Start-Process -FilePath ${powerShellSingleQuoted(installerPath)}"));
    assert!(script.contains("function powerShellSingleQuoted"));
    assert!(script.contains("return run(\"open\", [installerPath], { cwd: repoRoot });"));
    assert!(script.contains("Unsupported platform"));
    assert!(script.contains("installedMacosVersion"));
    assert!(script.contains("CFBundleShortVersionString"));
    assert!(script.contains("installedWindowsVersion"));
    assert!(script.contains("DisplayVersion"));
    assert!(script.contains("newestVersionFromLines"));
    assert!(script.contains("bumpVersionPastInstalled"));
    assert!(script.contains("syncProjectVersion"));
    assert!(script.contains("packageJsonPath"));
    assert!(script.contains("cargoTomlPath"));
    assert!(script.contains("findLatestInstaller"));
    assert!(script.contains("removeExistingInstallers(current.extension)"));
}

#[test]
fn windows_platform_uses_current_windows_crate_api() {
    let source = read_repo_file("src-tauri/src/rclone/platform/windows.rs");

    assert!(source.contains("SHCNE_DRIVEREMOVED"));
    assert!(!source.contains("SHCNE_DRIVEREMOVE,"));
    assert!(source.contains("ch.is_ascii_uppercase()"));
    assert!(!source.contains("('A'..='Z').contains(&ch)"));
    assert!(
        source.contains("RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey, Some(0), KEY_READ, &mut key)")
    );
    assert!(!source.contains("RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey, 0, KEY_READ, &mut key)"));
    assert!(source.contains(
        "GetVolumeInformationW(\n            windows::core::PCWSTR(wide.as_ptr()),\n            None,\n            None,\n            None,\n            None,\n            None,\n        )"
    ));
    assert!(!source.contains(
        "use windows::core::w;\n    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;"
    ));
}

#[test]
fn windows_mount_processes_are_hidden_from_user() {
    let process = read_repo_file("src-tauri/src/rclone/process.rs");
    let rclone = read_repo_file("src-tauri/src/rclone/mod.rs");
    let windows = read_repo_file("src-tauri/src/rclone/platform/windows.rs");

    assert!(process.contains("pub const CREATE_NO_WINDOW: u32 = 0x08000000;"));
    assert!(process.contains("command.creation_flags(CREATE_NO_WINDOW);"));
    assert!(rclone.contains("pub(crate) mod process;"));
    assert!(rclone.contains("use process::hidden_command;"));
    assert!(
        rclone.contains("let mut child = hidden_command(rclone_path)\n            .args(&args)")
    );
    assert!(rclone.contains("let _ = hidden_command(\"powershell.exe\")"));
    assert!(rclone.contains(
        "let mut child = hidden_command(rclone_path)\n        .args([\"obscure\", \"-\"])"
    ));
    assert!(rclone.contains("let mut child = hidden_command(rclone_path)\n        .args(args)"));
    assert!(windows.contains("use crate::rclone::process::hidden_command;"));
    assert_eq!(windows.matches("hidden_command(&rclone)").count(), 2);

    assert!(!rclone.contains("Command::new(rclone_path)"));
    assert!(!rclone.contains("Command::new(\"powershell.exe\")"));
    assert!(!windows.contains("Command::new(&rclone)"));
}

#[test]
fn quit_cleanup_does_not_repeat_windows_drive_unmount_sweep() {
    let rclone = read_repo_file("src-tauri/src/rclone/mod.rs");

    assert!(rclone.contains("self.cleanup_stale_mount_processes(app);"));
    assert!(rclone.contains("fn cleanup_stale_mount_processes(&self, app: &AppHandle)"));
    assert!(rclone.contains("cleanup_windows_rclone_processes(rclone_path.as_deref());"));
    assert!(!rclone.contains(
        "self.unmount_all(app);\n        self.cleanup_stale_processes(app);\n        self.refresh_configured_mount_targets();"
    ));
}

#[test]
fn launch_auto_mount_uses_start_and_finish_notifications() {
    let lib = read_repo_file("src-tauri/src/lib.rs");
    let commands = read_repo_file("src-tauri/src/commands.rs");
    let notifications = read_repo_file("src-tauri/src/notifications.rs");

    assert!(lib.contains("mod notifications;"));
    assert!(lib.contains("use notifications::show_app_notification;"));
    assert!(lib.contains("show_app_notification("));
    assert!(lib.contains("\"Unmounting active drives before quitting. Please wait.\""));
    assert!(lib.contains("let quit_item_for_menu = quit_item.clone();"));
    assert!(lib.contains("let _ = quit_item_for_menu.set_text(\"Quitting...\");"));
    assert!(lib.contains("let _ = quit_item_for_menu.set_enabled(false);"));
    assert!(
        lib.find("let _ = quit_item_for_menu.set_text(\"Quitting...\");")
            .unwrap()
            < lib.find("app.exit(0);").unwrap()
    );
    assert!(notifications.contains("pub fn show_app_notification(app: &AppHandle, body: &str)"));
    assert!(notifications.contains(".title(\"Cloud Drive Mount\")"));

    assert!(commands.contains("use crate::notifications::show_app_notification;"));
    assert!(commands.contains(
        "const AUTO_MOUNT_START_NOTIFICATION: &str = \"Mounting saved drives on launch. Please wait.\";"
    ));
    assert!(commands.contains(
        "const AUTO_MOUNT_COMPLETE_NOTIFICATION: &str = \"Auto-mount complete. Cloud Drive Mount is ready.\";"
    ));
    assert!(commands.contains(
        "const AUTO_MOUNT_FAILED_NOTIFICATION: &str = \"Auto-mount failed. Open Settings for details.\";"
    ));
    assert!(commands.contains("const AUTO_MOUNT_NETWORK_FAILED_NOTIFICATION: &str ="));
    assert!(commands.contains("Auto-mount failed because of network connectivity."));
    assert!(
        commands
            .find("show_app_notification(app, AUTO_MOUNT_START_NOTIFICATION);")
            .unwrap()
            < commands
                .find("match state.rclone.mount_all(app, &request)")
                .unwrap()
    );
    assert!(commands.contains("show_app_notification(app, AUTO_MOUNT_COMPLETE_NOTIFICATION);"));
    assert!(commands.contains("show_app_notification(app, auto_mount_failed_notification(&err));"));
    assert!(commands.contains("fn auto_mount_failed_notification(error: &str) -> &'static str"));
    assert!(commands.contains("is_network_connectivity_error(error)"));
    assert!(commands.contains("AUTO_MOUNT_FAILED_NOTIFICATION"));
}

#[test]
fn restart_mounts_keeps_app_running_and_refreshes_platform_mount_state() {
    let lib = read_repo_file("src-tauri/src/lib.rs");
    let commands = read_repo_file("src-tauri/src/commands.rs");
    let rclone = read_repo_file("src-tauri/src/rclone/mod.rs");
    let frontend = read_repo_file("src/main.ts");
    let index = read_repo_file("index.html");

    assert!(lib.contains("const ARG_AUTOSTART: &str = \"--autostart\";"));
    assert!(lib.contains("const ARG_CLEAN_RESTART: &str = \"--clean-restart\";"));
    assert!(lib.contains("const ARG_SHOW_SETTINGS: &str = \"--show-settings\";"));
    assert!(lib.contains("Some(vec![ARG_AUTOSTART])"));
    assert!(lib.contains("let clean_restart = launch_args.iter().any(|a| a == ARG_CLEAN_RESTART);"));
    assert!(lib.contains("restart_mounts,"));
    assert!(commands.contains("pub async fn restart_mounts("));
    assert!(commands.contains("rclone.restart_mount_cleanup(&app);"));
    assert!(commands.contains("let Some(request) = saved_mount_request() else"));
    assert!(commands.contains("rclone.mount_all(&app, &request)"));
    assert!(commands.contains("Mount background processes restarted."));
    assert!(frontend.contains("await invoke(\"restart_mounts\");"));
    assert!(frontend.contains("Restarting Mounts..."));
    assert!(index.contains("Restart Mounts"));
    assert!(lib.contains("if clean_restart {\n                    if let Err(err) = log.clear()"));
    assert!(lib.contains("state.rclone.refresh_configured_mount_targets();"));
    assert!(rclone.contains("pub fn refresh_configured_mount_targets(&self)"));
    assert!(rclone.contains("pub fn restart_mount_cleanup(&self, app: &AppHandle)"));
    assert!(rclone.contains("cleanup_windows_rclone_processes(rclone_path.as_deref());"));
    assert!(rclone.contains("platform::notify_mount_change(&target, false);"));
    assert!(!lib.contains("RESTART_PARENT_PID"));
    assert!(!commands.contains("restart_app"));
    assert!(!commands.contains("tauri::process::current_binary"));
    assert!(!commands.contains("detach_restart_command"));
    assert!(!commands.contains("Spawned clean restart replacement process"));
}

#[test]
fn refresh_button_flushes_active_mount_directory_caches() {
    let lib = read_repo_file("src-tauri/src/lib.rs");
    let commands = read_repo_file("src-tauri/src/commands.rs");
    let rclone = read_repo_file("src-tauri/src/rclone/mod.rs");
    let macos = read_repo_file("src-tauri/src/rclone/platform/macos.rs");
    let windows = read_repo_file("src-tauri/src/rclone/platform/windows.rs");
    let frontend = read_repo_file("src/main.ts");
    let index = read_repo_file("index.html");

    assert!(lib.contains("refresh_mount_caches,"));
    assert!(commands.contains("pub async fn refresh_mount_caches("));
    assert!(commands.contains("rclone.refresh_mount_caches(&app)"));
    assert!(rclone.contains("pub fn refresh_mount_caches(&self, app: &AppHandle)"));
    assert!(rclone.contains("platform::refresh_vfs_cache("));
    assert!(rclone.contains("&mount.target"));
    assert!(rclone.contains("mount.pid"));
    assert!(rclone.contains("\"No mounted drives to refresh.\""));
    assert!(macos.contains("\"-HUP\""));
    assert!(windows.contains("\"vfs/forget\""));
    assert!(windows.contains("\"--rc-addr\""));
    assert!(windows.contains("\"--rc-no-auth\""));
    assert!(frontend.contains("btnRefreshCaches"));
    assert!(frontend.contains("setMountOperation(\"refreshing\")"));
    assert!(frontend.contains("await invoke(\"refresh_mount_caches\");"));
    assert!(index.contains("id=\"btn-refresh-caches\""));
    assert!(index.contains(">Refresh</button>"));
    assert!(index
        .contains("changes made in OneDrive on the web or another device can take 1 to 5 minutes"));
    assert!(!index.contains("Open Log Folder"));
}
