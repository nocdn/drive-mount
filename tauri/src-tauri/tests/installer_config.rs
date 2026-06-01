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
    let path = manifest_dir().join("../..").join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
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
    assert!(workflow.contains("--bundles dmg"));
    assert!(workflow.contains("--bundles msi"));
    assert!(workflow.contains("actions/upload-artifact@v4"));
    assert!(workflow.contains("actions/download-artifact@v5"));
    assert!(workflow.contains("release/*.dmg"));
    assert!(workflow.contains("release/*.msi"));

    assert!(!workflow.contains("win/installer"));
    assert!(!workflow.contains("mac/CloudDriveMount"));
    assert!(!workflow.contains("release/*.zip"));
    assert!(!workflow.contains("release/*.exe"));
}
