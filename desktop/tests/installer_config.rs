//! Checks on the packaging setup and on source-level behaviour that only one
//! platform builds (so the other platform's CI still guards it).

use serde_json::Value;
use std::{fs, path::PathBuf};

/// The identity installers and dev builds are signed with.
const DEVELOPER_ID: &str = "Developer ID Application: Bartosz Bak (7CAU3XFRLQ)";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_file(path: &str) -> String {
    let path = manifest_dir().join(path);
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    contents.replace("\r\n", "\n")
}

fn read_repo_file(path: &str) -> String {
    read_file(&format!("../{path}"))
}

fn read_json(path: &str) -> Value {
    serde_json::from_str(&read_repo_file(path))
        .unwrap_or_else(|error| panic!("failed to parse {path}: {error}"))
}

fn cargo_version(cargo_toml: &str) -> &str {
    cargo_toml
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("Cargo.toml has a package version")
}

#[test]
fn package_and_cargo_versions_match() {
    let package = read_json("package.json");
    let cargo = read_file("Cargo.toml");
    assert_eq!(package["version"].as_str(), Some(cargo_version(&cargo)));
}

#[test]
fn packager_builds_installers_with_the_rclone_sidecar() {
    let cargo = read_file("Cargo.toml");

    assert!(cargo.contains("[package.metadata.packager]"));
    assert!(cargo.contains("product-name = \"Cloud Drive Mount\""));
    assert!(cargo.contains("identifier = \"com.bartek.clouddrivemount\""));
    assert!(cargo.contains("before-packaging-command = \"cargo build --release\""));
    assert!(cargo.contains("external-binaries = [\"binaries/rclone\"]"));
    assert!(cargo.contains("\"icons/icon.icns\""));
    assert!(cargo.contains("\"icons/icon.ico\""));
    assert!(cargo.contains(&format!("signing-identity = \"{DEVELOPER_ID}\"")));
    assert!(cargo.contains("template = \"packaging/windows/main.wxs\""));
    // The bundled rclone has to load FUSE-T's library under the hardened
    // runtime.
    assert!(cargo.contains("entitlements = \"packaging/macos/entitlements.plist\""));
    assert!(read_file("packaging/macos/entitlements.plist")
        .contains("<key>com.apple.security.cs.disable-library-validation</key>"));
    for icon in [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico",
        "icons/menu-bar-template.png",
    ] {
        assert!(manifest_dir().join(icon).is_file(), "{icon} is missing");
    }
}

#[test]
fn windows_installer_keeps_the_tauri_upgrade_code() {
    // UUIDv5(DNS, "Cloud Drive Mount.exe.app.x64"), what the Tauri MSI used,
    // so the new MSI upgrades an installed copy instead of sitting beside it.
    let template = read_file("packaging/windows/main.wxs");
    assert!(template.contains("UpgradeCode=\"33d54619-321a-5d4e-9f2f-5da42b243cf7\""));
    assert!(template.contains("<MajorUpgrade"));
}

#[test]
fn package_scripts_build_installers_through_cargo_packager() {
    let package = read_json("package.json");
    let scripts = &package["scripts"];

    assert_eq!(
        scripts["prepare:sidecars"].as_str(),
        Some("bun scripts/download-rclone.mjs")
    );
    assert!(scripts["build:installer:mac"]
        .as_str()
        .unwrap()
        .ends_with("cargo packager --release --formats dmg"));
    assert!(scripts["build:installer:windows"]
        .as_str()
        .unwrap()
        .ends_with("cargo packager --release --formats wix"));
    assert_eq!(
        scripts["build:installer:open"].as_str(),
        Some("bun scripts/build-open-installer.mjs")
    );
    assert!(package.get("dependencies").is_none());
    assert!(!read_repo_file("package.json").contains("tauri"));
}

#[test]
fn release_workflow_packages_dmg_and_msi() {
    let workflow = read_repo_file(".github/workflows/build-release.yml");

    assert!(workflow.contains("cargo install cargo-packager"));
    assert!(workflow.contains("SIDECAR_TARGET_TRIPLE: ${{ matrix.rust_target }}"));
    assert!(workflow.contains("cargo packager --release --formats ${{ matrix.packager_format }}"));
    assert!(workflow.contains("packager_format: dmg"));
    assert!(workflow.contains("packager_format: wix"));
    assert!(workflow.contains("working-directory: desktop"));
    assert!(workflow.contains("cargo clippy --locked --all-targets -- -D warnings"));
    assert!(workflow.contains("bundle_dir=\"desktop/target/release/bundle\""));
    assert!(!workflow.contains("signing-identity"));
    assert!(!workflow.contains("tauri"));
}

#[test]
fn macos_releases_are_signed_and_notarized() {
    let workflow = read_repo_file(".github/workflows/build-release.yml");
    let notarize = read_repo_file("scripts/notarize-dmg.sh");
    let package = read_json("package.json");

    for secret in [
        "APPLE_CERTIFICATE",
        "APPLE_CERTIFICATE_PASSWORD",
        "APPLE_API_KEY",
        "APPLE_API_ISSUER",
        "APPLE_API_PRIVATE_KEY_BASE64",
    ] {
        assert!(
            workflow.contains(&format!("${{{{ secrets.{secret} }}}}")),
            "{secret} is not passed to the workflow"
        );
    }
    assert!(workflow.contains("APPLE_API_KEY_PATH=$key_path"));
    assert!(workflow.contains("scripts/notarize-dmg.sh"));
    assert!(workflow.contains("signed with a Developer ID and notarized by Apple"));
    assert!(!workflow.contains("not Developer ID signed"));

    assert!(notarize.contains("xcrun notarytool submit"));
    assert!(notarize.contains("APPLE_KEYCHAIN_PROFILE"));
    assert!(notarize.contains("xcrun stapler staple"));
    assert!(notarize.contains("! -name 'rw.*'"));
    assert_eq!(
        package["scripts"]["build:installer:mac:notarized"].as_str(),
        Some("export APPLE_KEYCHAIN_PROFILE=cloud-drive-mount && bun run build:installer:mac && scripts/notarize-dmg.sh")
    );
}

#[test]
fn sidecar_downloader_supports_ci_targets_without_powershell_args() {
    let script = read_repo_file("scripts/download-rclone.mjs");

    assert!(script.contains("rclone-aarch64-apple-darwin"));
    assert!(!script.contains("x86_64-apple-darwin"));
    assert!(script.contains("rclone-x86_64-pc-windows-msvc.exe"));
    assert!(script.contains("SIDECAR_TARGET_TRIPLE"));
    assert!(script.contains("\"../desktop/binaries\""));
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
    assert!(script.contains("\"desktop\", \"target\", \"release\", \"bundle\""));
    assert!(script.contains("\"desktop\", \"Cargo.toml\""));
    assert!(script.contains("tomlString(cargoToml, \"product-name\")"));
    assert!(script.contains("Start-Process -FilePath ${powerShellSingleQuoted(installerPath)}"));
    assert!(script.contains("return run(\"open\", [installerPath], { cwd: repoRoot });"));
    assert!(script.contains("CFBundleShortVersionString"));
    assert!(script.contains("DisplayVersion"));
    assert!(script.contains("bumpVersionPastInstalled"));
    assert!(script.contains("syncProjectVersion"));
    assert!(script.contains("removeExistingInstallers(current.extension)"));
    assert!(!script.contains("tauri"));
}

#[test]
fn windows_platform_uses_current_windows_crate_api() {
    let source = read_file("src/rclone/platform/windows.rs");

    assert!(source.contains("SHCNE_DRIVEREMOVED"));
    assert!(source.contains("ch.is_ascii_uppercase()"));
    assert!(
        source.contains("RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey, Some(0), KEY_READ, &mut key)")
    );
    assert!(source.contains("GetVolumeInformationW("));
}

#[test]
fn windows_mount_processes_are_hidden_from_user() {
    let process = read_file("src/rclone/process.rs");
    let rclone = read_file("src/rclone/mod.rs");
    let windows = read_file("src/rclone/platform/windows.rs");

    assert!(process.contains("pub const CREATE_NO_WINDOW: u32 = 0x08000000;"));
    assert!(process.contains("command.creation_flags(CREATE_NO_WINDOW);"));
    assert!(rclone.contains("use process::hidden_command;"));
    assert!(rclone.contains("let _ = hidden_command(\"powershell.exe\")"));
    assert!(windows.contains("use crate::rclone::process::hidden_command;"));
    assert_eq!(windows.matches("hidden_command(&rclone)").count(), 2);
    assert!(!rclone.contains("Command::new("));
    assert!(!windows.contains("Command::new("));
}

#[test]
fn the_release_binary_has_no_console_window_on_windows() {
    let main = read_file("src/main.rs");
    assert!(main.contains("#![cfg_attr(not(debug_assertions), windows_subsystem = \"windows\")]"));
}

#[test]
fn quitting_unmounts_with_notifications_after_the_window_closes() {
    let main = read_file("src/main.rs");
    let backend = read_file("src/backend.rs");
    let tray = read_file("src/tray/mod.rs");

    let run = main.find("app.run();").unwrap();
    assert!(run < main.find("tray.set_quitting();").unwrap());
    assert!(run < main.find("backend.shut_down();").unwrap());
    assert!(tray.contains("\"Quitting...\""));

    let shut_down = &backend[backend.find("pub fn shut_down(&self)").unwrap()..];
    let start = shut_down
        .find("show_app_notification(QUIT_START_NOTIFICATION);")
        .unwrap();
    let unmount = shut_down.find("self.unmount_all();").unwrap();
    let done = shut_down
        .find("show_app_notification(QUIT_COMPLETE_NOTIFICATION);")
        .unwrap();
    assert!(start < unmount && unmount < done);
    assert!(backend.contains(
        "pub const QUIT_START_NOTIFICATION: &str = \"Unmounting active drives before quitting. Please wait.\";"
    ));
}

#[test]
fn launch_auto_mount_uses_start_and_finish_notifications() {
    let backend = read_file("src/backend.rs");
    let notifications = read_file("src/notifications.rs");

    assert!(notifications.contains("pub fn show_app_notification(body: &str)"));
    assert!(notifications.contains("\"Cloud Drive Mount\""));
    assert!(backend.contains(
        "const AUTO_MOUNT_START_NOTIFICATION: &str = \"Mounting saved drives on launch. Please wait.\";"
    ));
    assert!(backend.contains(
        "const AUTO_MOUNT_COMPLETE_NOTIFICATION: &str = \"Auto-mount complete. Cloud Drive Mount is ready.\";"
    ));
    assert!(backend.contains(
        "const AUTO_MOUNT_FAILED_NOTIFICATION: &str = \"Auto-mount failed. Open Settings for details.\";"
    ));
    assert!(backend.contains("Auto-mount failed because of network connectivity."));
    let start = backend
        .find("show_app_notification(AUTO_MOUNT_START_NOTIFICATION);")
        .unwrap();
    let after = &backend[start..];
    assert!(after.contains("show_app_notification(AUTO_MOUNT_COMPLETE_NOTIFICATION);"));
    assert!(after.contains("show_app_notification(auto_mount_failed_notification(&err));"));
}

#[test]
fn restart_and_refresh_are_offered_from_the_drives_menu() {
    let main = read_file("src/main.rs");
    let backend = read_file("src/backend.rs");
    let rclone = read_file("src/rclone/mod.rs");
    let drives = read_file("src/ui/drives.rs");
    let providers = read_file("src/ui/providers.rs");
    let macos = read_file("src/rclone/platform/macos.rs");
    let windows = read_file("src/rclone/platform/windows.rs");

    assert!(main.contains("const ARG_CLEAN_RESTART: &str = \"--clean-restart\";"));
    assert!(main.contains("const ARG_SHOW_SETTINGS: &str = \"--show-settings\";"));
    assert!(
        read_file("src/autostart.rs").contains("pub const ARG_AUTOSTART: &str = \"--autostart\";")
    );

    assert!(backend.contains("self.rclone.restart_mount_cleanup(&self.host);"));
    assert!(backend.contains("let Some(request) = saved_mount_request() else"));
    assert!(backend.contains("Mount background processes restarted."));
    assert!(rclone.contains("pub fn restart_mount_cleanup(&self, app: &Host)"));
    assert!(rclone.contains("platform::notify_mount_change(&target, false);"));
    assert!(drives.contains("\"Restart mounts\""));

    assert!(backend.contains("pub fn refresh_mount_caches(&self)"));
    assert!(rclone.contains("pub fn refresh_mount_caches(&self, app: &Host)"));
    assert!(rclone.contains("\"No mounted drives to refresh.\""));
    assert!(macos.contains("\"-HUP\""));
    assert!(windows.contains("\"vfs/forget\""));
    assert!(drives.contains("\"Refresh folder listings\""));
    assert!(providers
        .contains("changes made in OneDrive on the web or another device can take 1 to 5 minutes"));
}

#[test]
fn macos_dev_builds_are_signed_with_the_installers_identity() {
    let config = read_repo_file(".cargo/config.toml");
    let runner = read_repo_file("scripts/macos-dev-runner.sh");

    assert!(config.contains("[target.'cfg(target_os = \"macos\")']"));
    assert!(config.contains("runner = \"scripts/macos-dev-runner.sh\""));
    assert!(runner.contains(&format!("IDENTITY=\"{DEVELOPER_ID}\"")));
    assert!(runner.contains("--sign \"$IDENTITY\" --timestamp=none"));
    assert!(runner.contains("--identifier com.bartek.clouddrivemount"));
    assert!(runner.contains("exec \"$@\""));
    assert!(!manifest_dir()
        .join("../scripts/create-signing-certificate.sh")
        .exists());
}
