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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        settings.remote_path = crate::paths::normalize_remote_path(&settings.remote_path);
        settings.root_folder_id = settings.root_folder_id.trim().to_string();
        settings
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        if settings.port == 0 || settings.port > 65535 {
            settings.port = default_seedbox_port();
        }
        settings
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub selected_provider: CloudProvider,
    #[serde(default)]
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

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            selected_provider: CloudProvider::BackblazeB2,
            buckets: vec![BucketMount::default()],
            google_drive: GoogleDriveSettings::default(),
            seedbox: SeedboxSettings::default(),
            start_at_login: true,
            start_minimized: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct B2Credentials {
    pub application_key_id: String,
    pub application_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountState {
    pub mounted: bool,
}
