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
fn windows_platform_uses_current_windows_crate_api() {
    let source = read_repo_file("src-tauri/src/rclone/platform/windows.rs");

    assert!(source.contains("SHCNE_DRIVEREMOVED"));
    assert!(!source.contains("SHCNE_DRIVEREMOVE,"));
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
