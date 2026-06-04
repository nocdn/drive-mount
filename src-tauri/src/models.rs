use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum CloudProvider {
    #[default]
    #[serde(rename = "B2")]
    BackblazeB2,
    #[serde(rename = "GoogleDrive")]
    GoogleDrive,
    Seedbox,
}

pub const GDRIVE_REMOTE: &str = "gdrive";
pub const SEEDBOX_REMOTE: &str = "seedbox";
#[cfg(windows)]
pub const GOOGLE_DRIVE_WINDOWS_DRIVE: &str = "G";
#[cfg(windows)]
pub const SEEDBOX_WINDOWS_DRIVE: &str = "S";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BucketMount {
    pub bucket_name: String,
    #[serde(default)]
    pub drive_letter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GoogleDriveSettings {
    #[serde(default)]
    pub remote_path: String,
    #[serde(default)]
    pub root_folder_id: String,
}

impl GoogleDriveSettings {
    pub fn normalized(&self) -> Self {
        let mut settings = self.clone();
        settings.remote_path = crate::paths::normalize_google_drive_path(&settings.remote_path);
        settings.root_folder_id = settings.root_folder_id.trim().to_string();
        settings
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedboxSettings {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_seedbox_port")]
    pub port: u16,
    #[serde(default = "default_seedbox_remote_path")]
    pub remote_path: String,
    #[serde(default = "default_true")]
    pub allow_unverified_certificate: bool,
    #[serde(default = "default_true")]
    pub read_only: bool,
}

fn default_seedbox_port() -> u16 {
    21
}

fn default_seedbox_remote_path() -> String {
    "downloads".to_string()
}

impl Default for SeedboxSettings {
    fn default() -> Self {
        Self {
            host: String::new(),
            username: String::new(),
            port: default_seedbox_port(),
            remote_path: default_seedbox_remote_path(),
            allow_unverified_certificate: true,
            read_only: true,
        }
    }
}

impl SeedboxSettings {
    pub fn normalized(&self) -> Self {
        let mut settings = self.clone();
        settings.host = crate::paths::normalize_seedbox_host(&settings.host);
        settings.username = settings.username.trim().to_string();
        settings.remote_path = crate::paths::normalize_remote_path(&settings.remote_path);
        if settings.remote_path.is_empty() {
            settings.remote_path = default_seedbox_remote_path();
        }
        settings
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub selected_provider: CloudProvider,
    #[serde(default = "default_buckets")]
    pub buckets: Vec<BucketMount>,
    #[serde(default)]
    pub google_drive: GoogleDriveSettings,
    #[serde(default)]
    pub seedbox: SeedboxSettings,
    #[serde(default)]
    pub start_at_login: bool,
    #[serde(default)]
    pub start_minimized: bool,
}

fn default_true() -> bool {
    true
}

fn default_buckets() -> Vec<BucketMount> {
    vec![BucketMount::default()]
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            selected_provider: CloudProvider::BackblazeB2,
            buckets: default_buckets(),
            google_drive: GoogleDriveSettings::default(),
            seedbox: SeedboxSettings::default(),
            start_at_login: false,
            start_minimized: false,
        }
    }
}

impl AppSettings {
    pub fn normalized(&self) -> Self {
        let mut settings = self.clone();
        settings.google_drive = settings.google_drive.normalized();
        settings.seedbox = settings.seedbox.normalized();
        settings.buckets = settings
            .buckets
            .iter()
            .map(|bucket| BucketMount {
                bucket_name: bucket.bucket_name.trim().to_string(),
                drive_letter: bucket.drive_letter.trim().to_string(),
            })
            .collect();
        settings
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct B2Credentials {
    pub application_key_id: String,
    pub application_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSettings {
    pub settings: AppSettings,
    pub has_saved_credentials: bool,
    pub is_google_drive_configured: bool,
    pub is_seedbox_configured: bool,
    pub has_saved_seedbox_password: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountRequest {
    pub application_key_id: String,
    pub application_key: String,
    pub buckets: Vec<BucketMount>,
    pub google_drive: GoogleDriveSettings,
    pub seedbox: SeedboxSettings,
    pub seedbox_password: String,
    pub selected_provider: CloudProvider,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountEntry {
    pub label: String,
    pub target: String,
    pub provider: CloudProvider,
    pub status: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MountState {
    pub mounted: bool,
    #[serde(default)]
    pub mounts: Vec<MountEntry>,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_provider_serializes_to_frontend_contract_values() {
        assert_eq!(
            serde_json::to_string(&CloudProvider::BackblazeB2).unwrap(),
            "\"B2\""
        );
        assert_eq!(
            serde_json::to_string(&CloudProvider::GoogleDrive).unwrap(),
            "\"GoogleDrive\""
        );
        assert_eq!(
            serde_json::to_string(&CloudProvider::Seedbox).unwrap(),
            "\"Seedbox\""
        );

        assert_eq!(
            serde_json::from_str::<CloudProvider>("\"B2\"").unwrap(),
            CloudProvider::BackblazeB2
        );
        assert_eq!(
            serde_json::from_str::<CloudProvider>("\"GoogleDrive\"").unwrap(),
            CloudProvider::GoogleDrive
        );
        assert_eq!(
            serde_json::from_str::<CloudProvider>("\"Seedbox\"").unwrap(),
            CloudProvider::Seedbox
        );
    }

    #[test]
    fn app_settings_deserializes_defaults_from_sparse_json() {
        let settings: AppSettings = serde_json::from_str("{}").unwrap();

        assert_eq!(settings.selected_provider, CloudProvider::BackblazeB2);
        assert_eq!(settings.buckets, vec![BucketMount::default()]);
        assert_eq!(settings.seedbox.port, 21);
        assert_eq!(settings.seedbox.remote_path, "downloads");
        assert!(settings.seedbox.allow_unverified_certificate);
        assert!(settings.seedbox.read_only);
        assert!(!settings.start_at_login);
        assert!(!settings.start_minimized);
    }

    #[test]
    fn google_drive_normalized_cleans_paths() {
        let settings = GoogleDriveSettings {
            remote_path: " :/Team\\Shared ".to_string(),
            root_folder_id: " root-id \n".to_string(),
        }
        .normalized();

        assert_eq!(settings.remote_path, "Team/Shared");
        assert_eq!(settings.root_folder_id, "root-id");
    }

    #[test]
    fn seedbox_normalized_cleans_values_and_defaults() {
        let settings = SeedboxSettings {
            host: " FTPS://seedbox.example.com/// ".to_string(),
            username: " user ".to_string(),
            port: 2121,
            remote_path: " :/media\\shows ".to_string(),
            allow_unverified_certificate: false,
            read_only: false,
        }
        .normalized();

        assert_eq!(settings.host, "seedbox.example.com");
        assert_eq!(settings.username, "user");
        assert_eq!(settings.port, 2121);
        assert_eq!(settings.remote_path, "media/shows");
        assert!(!settings.allow_unverified_certificate);
        assert!(!settings.read_only);
    }

    #[test]
    fn seedbox_normalized_uses_default_remote_path_when_blank() {
        let settings = SeedboxSettings {
            remote_path: " //: ".to_string(),
            ..SeedboxSettings::default()
        }
        .normalized();

        assert_eq!(settings.remote_path, "downloads");
    }

    #[test]
    fn loaded_settings_serializes_with_camel_case_fields() {
        let loaded = LoadedSettings {
            settings: AppSettings::default(),
            has_saved_credentials: true,
            is_google_drive_configured: true,
            is_seedbox_configured: false,
            has_saved_seedbox_password: true,
        };

        let value = serde_json::to_value(loaded).unwrap();
        assert_eq!(value["hasSavedCredentials"], true);
        assert!(value.get("applicationKeyId").is_none());
        assert!(value.get("applicationKey").is_none());
        assert_eq!(value["isGoogleDriveConfigured"], true);
        assert_eq!(value["isSeedboxConfigured"], false);
        assert_eq!(value["hasSavedSeedboxPassword"], true);
    }

    #[test]
    fn mount_state_serializes_mounted_flag_and_entries() {
        let state = MountState {
            mounted: true,
            mounts: vec![MountEntry {
                label: "Google Drive".to_string(),
                target: "G:".to_string(),
                provider: CloudProvider::GoogleDrive,
                status: "mounted".to_string(),
                pid: Some(42),
            }],
            errors: Vec::new(),
        };

        let value = serde_json::to_value(state).unwrap();

        assert_eq!(value["mounted"], true);
        assert_eq!(value["mounts"][0]["label"], "Google Drive");
        assert_eq!(value["mounts"][0]["provider"], "GoogleDrive");
        assert_eq!(value["mounts"][0]["pid"], 42);
        assert!(value["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn mount_request_deserializes_frontend_payload_shape() {
        let json = r#"{
            "applicationKeyId": "id",
            "applicationKey": "key",
            "buckets": [{ "bucketName": "bucket", "driveLetter": "P" }],
            "googleDrive": { "remotePath": "docs", "remoteName": "legacy" },
            "seedbox": { "host": "ftp://example.com", "username": "user", "driveLetter": "S" },
            "seedboxPassword": "secret",
            "selectedProvider": "Seedbox"
        }"#;

        let request: MountRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.application_key_id, "id");
        assert_eq!(request.application_key, "key");
        assert_eq!(request.buckets[0].bucket_name, "bucket");
        assert_eq!(request.buckets[0].drive_letter, "P");
        assert_eq!(request.google_drive.remote_path, "docs");
        assert_eq!(request.seedbox.host, "ftp://example.com");
        assert_eq!(request.seedbox.username, "user");
        assert_eq!(request.seedbox.port, 21);
        assert_eq!(request.seedbox_password, "secret");
        assert_eq!(request.selected_provider, CloudProvider::Seedbox);
    }
}
