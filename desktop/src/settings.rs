use std::fs;
use std::path::Path;

use crate::models::AppSettings;
use crate::paths::settings_path;

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if !path.exists() {
        return AppSettings::default();
    }

    match fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

pub fn ensure_app_data_dir() -> Result<(), String> {
    let dir = crate::paths::app_data_dir();
    ensure_dir(&dir)
}

pub fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BucketMount, CloudProvider, OneDriveSettings};

    #[test]
    fn load_settings_returns_default_when_file_is_missing() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        let settings = load_settings();

        assert_eq!(settings.selected_provider, CloudProvider::BackblazeB2);
        assert_eq!(settings.buckets.len(), 1);
        assert_eq!(settings.one_drive, OneDriveSettings::default());
        assert!(!crate::paths::settings_path().exists());

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn save_settings_creates_parent_directory_and_round_trips_pretty_json() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        let app_data = temp.path().join("nested").join("app-data");
        crate::test_support::set_test_dirs(&app_data, &temp.path().join("logs"));

        let settings = AppSettings {
            selected_provider: CloudProvider::Seedbox,
            buckets: vec![BucketMount {
                bucket_name: "photos".to_string(),
                drive_letter: "P".to_string(),
            }],
            start_at_login: false,
            start_minimized: true,
            ..AppSettings::default()
        };

        save_settings(&settings).unwrap();

        let raw = fs::read_to_string(crate::paths::settings_path()).unwrap();
        assert!(raw.contains("\n  \"selectedProvider\": \"Seedbox\""));
        assert!(raw.contains("\n  \"oneDrive\":"));
        assert!(raw.contains("\"bucketName\": \"photos\""));
        assert_eq!(load_settings().selected_provider, CloudProvider::Seedbox);
        assert_eq!(load_settings().buckets[0].bucket_name, "photos");
        assert!(!load_settings().start_at_login);
        assert!(load_settings().start_minimized);

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn load_settings_uses_defaults_for_invalid_json() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));
        fs::create_dir_all(crate::paths::app_data_dir()).unwrap();
        fs::write(crate::paths::settings_path(), "{not json").unwrap();

        let settings = load_settings();

        assert_eq!(settings, AppSettings::default());

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn load_settings_fills_missing_fields_from_defaults() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));
        fs::create_dir_all(crate::paths::app_data_dir()).unwrap();
        fs::write(
            crate::paths::settings_path(),
            r#"{
                "selectedProvider": "GoogleDrive",
                "buckets": [{ "bucketName": "docs" }],
                "startMinimized": true
            }"#,
        )
        .unwrap();

        let settings = load_settings();

        assert_eq!(settings.selected_provider, CloudProvider::GoogleDrive);
        assert_eq!(settings.buckets[0].bucket_name, "docs");
        assert_eq!(settings.buckets[0].drive_letter, "");
        assert_eq!(settings.one_drive, OneDriveSettings::default());
        assert_eq!(settings.seedbox.remote_path, "downloads");
        assert!(!settings.start_at_login);
        assert!(settings.start_minimized);

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn ensure_helpers_create_requested_directories() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        let app_data = temp.path().join("app");
        crate::test_support::set_test_dirs(&app_data, &temp.path().join("logs"));

        ensure_app_data_dir().unwrap();
        assert!(app_data.is_dir());

        let nested = temp.path().join("a").join("b").join("c");
        ensure_dir(&nested).unwrap();
        assert!(nested.is_dir());

        crate::test_support::clear_test_dirs();
    }
}
