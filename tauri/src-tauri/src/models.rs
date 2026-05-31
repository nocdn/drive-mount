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
pub struct AppSettings {
    #[serde(default)]
    pub selected_provider: CloudProvider,
    #[serde(default)]
    pub buckets: Vec<BucketMount>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountRequest {
    pub application_key_id: String,
    pub application_key: String,
    pub buckets: Vec<BucketMount>,
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
