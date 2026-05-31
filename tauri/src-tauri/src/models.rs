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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BucketMount {
    pub bucket_name: String,
    #[serde(default)]
    pub mount_path: String,
    #[serde(default)]
    pub drive_letter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleDriveSettings {
    #[serde(default = "default_gdrive_remote")]
    pub remote_name: String,
    #[serde(default)]
    pub remote_path: String,
    #[serde(default)]
    pub root_folder_id: String,
    #[serde(default)]
    pub mount_path: String,
    #[serde(default = "default_gdrive_drive_letter")]
    pub drive_letter: String,
}

fn default_gdrive_remote() -> String {
    GDRIVE_REMOTE.to_string()
}

fn default_gdrive_drive_letter() -> String {
    "G".to_string()
}

impl Default for GoogleDriveSettings {
    fn default() -> Self {
        Self {
            remote_name: default_gdrive_remote(),
            remote_path: String::new(),
            root_folder_id: String::new(),
            mount_path: String::new(),
            drive_letter: default_gdrive_drive_letter(),
        }
    }
}

impl GoogleDriveSettings {
    pub fn normalized(&self) -> Self {
        let mut settings = self.clone();
        settings.remote_name = GDRIVE_REMOTE.to_string();
        settings.drive_letter = default_gdrive_drive_letter();
        settings.remote_path = crate::paths::normalize_google_drive_path(&settings.remote_path);
        settings.root_folder_id = settings.root_folder_id.trim().to_string();
        settings
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedboxSettings {
    #[serde(default = "default_seedbox_remote")]
    pub remote_name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_seedbox_port")]
    pub port: u16,
    #[serde(default = "default_seedbox_remote_path")]
    pub remote_path: String,
    #[serde(default)]
    pub mount_path: String,
    #[serde(default = "default_seedbox_drive_letter")]
    pub drive_letter: String,
    #[serde(default = "default_true")]
    pub allow_unverified_certificate: bool,
    #[serde(default = "default_true")]
    pub read_only: bool,
}

fn default_seedbox_remote() -> String {
    SEEDBOX_REMOTE.to_string()
}

fn default_seedbox_port() -> u16 {
    21
}

fn default_seedbox_remote_path() -> String {
    "downloads".to_string()
}

fn default_seedbox_drive_letter() -> String {
    "S".to_string()
}

impl Default for SeedboxSettings {
    fn default() -> Self {
        Self {
            remote_name: default_seedbox_remote(),
            host: String::new(),
            username: String::new(),
            port: default_seedbox_port(),
            remote_path: default_seedbox_remote_path(),
            mount_path: String::new(),
            drive_letter: default_seedbox_drive_letter(),
            allow_unverified_certificate: true,
            read_only: true,
        }
    }
}

impl SeedboxSettings {
    pub fn normalized(&self) -> Self {
        let mut settings = self.clone();
        settings.remote_name = SEEDBOX_REMOTE.to_string();
        settings.drive_letter = default_seedbox_drive_letter();
        settings.host = crate::paths::normalize_seedbox_host(&settings.host);
        settings.username = settings.username.trim().to_string();
        settings.remote_path = crate::paths::normalize_remote_path(&settings.remote_path);
        if settings.remote_path.is_empty() {
            settings.remote_path = default_seedbox_remote_path();
        }
        if settings.port == 0 {
            settings.port = default_seedbox_port();
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
    #[serde(default = "default_true")]
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
            start_at_login: true,
            start_minimized: false,
        }
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
    pub application_key_id: String,
    pub application_key: String,
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
pub struct MountState {
    pub mounted: bool,
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
        assert_eq!(settings.google_drive.remote_name, GDRIVE_REMOTE);
        assert_eq!(settings.google_drive.drive_letter, "G");
        assert_eq!(settings.seedbox.remote_name, SEEDBOX_REMOTE);
        assert_eq!(settings.seedbox.port, 21);
        assert_eq!(settings.seedbox.remote_path, "downloads");
        assert_eq!(settings.seedbox.drive_letter, "S");
        assert!(settings.seedbox.allow_unverified_certificate);
        assert!(settings.seedbox.read_only);
        assert!(settings.start_at_login);
        assert!(!settings.start_minimized);
    }

    #[test]
    fn google_drive_normalized_restores_reserved_values_and_cleans_paths() {
        let settings = GoogleDriveSettings {
            remote_name: "user-provided".to_string(),
            remote_path: " :/Team\\Shared ".to_string(),
            root_folder_id: " root-id \n".to_string(),
            mount_path: "  ~/Drives/GDrive ".to_string(),
            drive_letter: "Z".to_string(),
        }
        .normalized();

        assert_eq!(settings.remote_name, GDRIVE_REMOTE);
        assert_eq!(settings.remote_path, "Team/Shared");
        assert_eq!(settings.root_folder_id, "root-id");
        assert_eq!(settings.mount_path, "  ~/Drives/GDrive ");
        assert_eq!(settings.drive_letter, "G");
    }

    #[test]
    fn seedbox_normalized_restores_reserved_values_and_defaults() {
        let settings = SeedboxSettings {
            remote_name: "custom".to_string(),
            host: " FTPS://seedbox.example.com/// ".to_string(),
            username: " user ".to_string(),
            port: 0,
            remote_path: " :/media\\shows ".to_string(),
            mount_path: "/mnt/seedbox".to_string(),
            drive_letter: "X".to_string(),
            allow_unverified_certificate: false,
            read_only: false,
        }
        .normalized();

        assert_eq!(settings.remote_name, SEEDBOX_REMOTE);
        assert_eq!(settings.host, "seedbox.example.com");
        assert_eq!(settings.username, "user");
        assert_eq!(settings.port, 21);
        assert_eq!(settings.remote_path, "media/shows");
        assert_eq!(settings.mount_path, "/mnt/seedbox");
        assert_eq!(settings.drive_letter, "S");
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
            application_key_id: "id".to_string(),
            application_key: "key".to_string(),
            is_google_drive_configured: true,
            is_seedbox_configured: false,
            has_saved_seedbox_password: true,
        };

        let value = serde_json::to_value(loaded).unwrap();
        assert_eq!(value["hasSavedCredentials"], true);
        assert_eq!(value["applicationKeyId"], "id");
        assert_eq!(value["applicationKey"], "key");
        assert_eq!(value["isGoogleDriveConfigured"], true);
        assert_eq!(value["isSeedboxConfigured"], false);
        assert_eq!(value["hasSavedSeedboxPassword"], true);
    }

    #[test]
    fn mount_request_deserializes_frontend_payload_shape() {
        let json = r#"{
            "applicationKeyId": "id",
            "applicationKey": "key",
            "buckets": [{ "bucketName": "bucket", "mountPath": "/mnt/bucket" }],
            "googleDrive": { "remotePath": "docs" },
            "seedbox": { "host": "ftp://example.com", "username": "user" },
            "seedboxPassword": "secret",
            "selectedProvider": "Seedbox"
        }"#;

        let request: MountRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.application_key_id, "id");
        assert_eq!(request.application_key, "key");
        assert_eq!(request.buckets[0].bucket_name, "bucket");
        assert_eq!(request.google_drive.remote_name, GDRIVE_REMOTE);
        assert_eq!(request.google_drive.remote_path, "docs");
        assert_eq!(request.seedbox.host, "ftp://example.com");
        assert_eq!(request.seedbox.username, "user");
        assert_eq!(request.seedbox.port, 21);
        assert_eq!(request.seedbox_password, "secret");
        assert_eq!(request.selected_provider, CloudProvider::Seedbox);
    }
}
