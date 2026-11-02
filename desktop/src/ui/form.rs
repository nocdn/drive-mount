//! The window's editable copy of the settings and the rules it checks
//! before mounting or testing a connection. No drawing here, so it's tested
//! on its own.

use std::collections::{BTreeMap, BTreeSet};

use rune::TextEdit;

use crate::models::{
    AppSettings, BucketMount, CloudProvider, GoogleDriveSettings, OneDriveSettings, SeedboxSettings,
};

/// Fixed Windows drive letters of the single-drive providers: Google Drive,
/// OneDrive and Seedbox.
pub const RESERVED_DRIVE_LETTERS: [char; 3] = ['G', 'O', 'S'];
const DEFAULT_SEEDBOX_PORT: u16 = 22;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Mac,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Mac
        }
    }
}

/// One bucket row: its name and, on Windows, the letter it mounts at.
pub struct BucketDraft {
    pub name: TextEdit,
    pub drive: String,
}

/// What the fields show, which may differ from what is saved.
pub struct Form {
    pub selected_provider: CloudProvider,
    pub b2_key_id: TextEdit,
    pub b2_key: TextEdit,
    pub buckets: Vec<BucketDraft>,
    pub gdrive_path: TextEdit,
    pub gdrive_root: TextEdit,
    pub onedrive_path: TextEdit,
    pub seedbox_host: TextEdit,
    pub seedbox_port: TextEdit,
    pub seedbox_user: TextEdit,
    pub seedbox_password: TextEdit,
    pub seedbox_path: TextEdit,
    pub seedbox_read_only: bool,
    pub start_at_login: bool,
    pub start_minimized: bool,
}

impl Form {
    pub fn from_settings(settings: &AppSettings, platform: Platform, unavailable: &[char]) -> Self {
        let mut form = Form {
            selected_provider: settings.selected_provider,
            b2_key_id: TextEdit::new(""),
            b2_key: TextEdit::new("").secure(),
            buckets: Vec::new(),
            gdrive_path: TextEdit::new(settings.google_drive.remote_path.clone()),
            gdrive_root: TextEdit::new(settings.google_drive.root_folder_id.clone()),
            onedrive_path: TextEdit::new(settings.one_drive.remote_path.clone()),
            seedbox_host: TextEdit::new(settings.seedbox.host.clone()),
            seedbox_port: TextEdit::new(settings.seedbox.normalized().port.to_string()),
            seedbox_user: TextEdit::new(settings.seedbox.username.clone()),
            seedbox_password: TextEdit::new("").secure(),
            seedbox_path: TextEdit::new(settings.seedbox.remote_path.clone()),
            seedbox_read_only: settings.seedbox.read_only,
            start_at_login: settings.start_at_login,
            start_minimized: settings.start_minimized,
        };
        for bucket in &settings.buckets {
            let drive = match platform {
                Platform::Mac => String::new(),
                Platform::Windows if !normalize_drive_letter(&bucket.drive_letter).is_empty() => {
                    normalize_drive_letter(&bucket.drive_letter)
                }
                Platform::Windows => form.next_drive_letter(platform, unavailable),
            };
            form.buckets.push(BucketDraft {
                name: TextEdit::new(bucket.bucket_name.clone()),
                drive,
            });
        }
        if form.buckets.is_empty() {
            form.add_bucket(platform, unavailable);
        }
        form
    }

    /// The letter a new bucket row gets on Windows: the last free one.
    pub fn next_drive_letter(&self, platform: Platform, unavailable: &[char]) -> String {
        if platform == Platform::Mac {
            return String::new();
        }
        let mut used: Vec<String> = RESERVED_DRIVE_LETTERS
            .iter()
            .chain(unavailable)
            .map(|c| c.to_string())
            .collect();
        used.extend(self.buckets.iter().map(|bucket| bucket.drive.clone()));
        next_available_drive_letter(used.iter().map(String::as_str))
            .map(String::from)
            .unwrap_or_default()
    }

    pub fn add_bucket(&mut self, platform: Platform, unavailable: &[char]) {
        let drive = self.next_drive_letter(platform, unavailable);
        self.buckets.push(BucketDraft {
            name: TextEdit::new(""),
            drive,
        });
    }

    /// Removes a bucket row; the last one is emptied instead, so there is
    /// always a row to type in.
    pub fn remove_bucket(&mut self, index: usize, platform: Platform, unavailable: &[char]) {
        if index >= self.buckets.len() {
            return;
        }
        if self.buckets.len() == 1 {
            self.buckets[0].name.replace_text("");
            self.buckets[0].drive = String::new();
            self.buckets[0].drive = self.next_drive_letter(platform, unavailable);
            return;
        }
        self.buckets.remove(index);
    }

    pub fn seedbox_port(&self) -> u16 {
        parse_seedbox_port(&self.seedbox_port.text).unwrap_or(0)
    }

    pub fn google_drive(&self) -> GoogleDriveSettings {
        GoogleDriveSettings {
            remote_path: self.gdrive_path.text.trim().to_string(),
            root_folder_id: self.gdrive_root.text.trim().to_string(),
        }
    }

    pub fn one_drive(&self) -> OneDriveSettings {
        OneDriveSettings {
            remote_path: self.onedrive_path.text.trim().to_string(),
        }
    }

    pub fn seedbox(&self) -> SeedboxSettings {
        SeedboxSettings {
            host: self.seedbox_host.text.trim().to_string(),
            username: self.seedbox_user.text.trim().to_string(),
            port: self.seedbox_port(),
            remote_path: self.seedbox_path.text.trim().to_string(),
            read_only: self.seedbox_read_only,
        }
    }

    pub fn bucket_mounts(&self, platform: Platform) -> Vec<BucketMount> {
        self.buckets
            .iter()
            .map(|bucket| BucketMount {
                bucket_name: bucket.name.text.trim().to_string(),
                drive_letter: match platform {
                    Platform::Mac => String::new(),
                    Platform::Windows => bucket.drive.trim().to_string(),
                },
            })
            .collect()
    }

    /// The settings as the fields show them.
    pub fn settings(&self, platform: Platform) -> AppSettings {
        AppSettings {
            selected_provider: self.selected_provider,
            buckets: self.bucket_mounts(platform),
            google_drive: self.google_drive(),
            one_drive: self.one_drive(),
            seedbox: self.seedbox(),
            start_at_login: self.start_at_login,
            start_minimized: self.start_minimized,
        }
    }

    /// Strips a scheme and trailing slashes from the Seedbox host, as typed
    /// from a browser's address bar.
    pub fn normalize_seedbox_host(&mut self) {
        let host = normalized_host(&self.seedbox_host.text);
        if host != self.seedbox_host.text {
            self.seedbox_host.replace_text(host);
        }
    }
}

pub fn normalized_host(host: &str) -> String {
    let mut host = host.trim().to_string();
    for scheme in ["https://", "http://", "ftps://", "ftp://"] {
        if host.to_lowercase().starts_with(scheme) {
            host = host[scheme.len()..].trim().to_string();
            break;
        }
    }
    while host.ends_with('/') {
        host.pop();
    }
    host
}

/// An empty port means the default; anything not all digits is invalid.
pub fn parse_seedbox_port(raw: &str) -> Option<u16> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Some(DEFAULT_SEEDBOX_PORT);
    }
    if !raw.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u32>()
        .ok()
        .and_then(|port| u16::try_from(port).ok())
}

pub fn normalize_drive_letter(value: &str) -> String {
    value.trim().trim_end_matches(':').to_uppercase()
}

/// The last free letter, from Z down.
pub fn next_available_drive_letter<'a>(used: impl IntoIterator<Item = &'a str>) -> Option<char> {
    let used: BTreeSet<String> = used
        .into_iter()
        .map(normalize_drive_letter)
        .filter(|letter| letter.len() == 1 && letter.chars().all(|c| c.is_ascii_uppercase()))
        .collect();
    ('A'..='Z')
        .rev()
        .find(|letter| !used.contains(&letter.to_string()))
}

/// What a running mount depends on: changing any of it needs a remount.
/// Login preferences and blank bucket rows don't count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountSnapshot {
    buckets: Vec<(String, String)>,
    google_drive: (String, String),
    one_drive: String,
    seedbox: (String, String, u16, String, bool),
}

impl MountSnapshot {
    pub fn of(settings: &AppSettings, platform: Platform) -> Self {
        let s = &settings.seedbox.normalized();
        MountSnapshot {
            buckets: settings
                .buckets
                .iter()
                .filter(|bucket| !bucket.bucket_name.trim().is_empty())
                .map(|bucket| {
                    (
                        bucket.bucket_name.trim().to_string(),
                        match platform {
                            Platform::Mac => String::new(),
                            Platform::Windows => normalize_drive_letter(&bucket.drive_letter),
                        },
                    )
                })
                .collect(),
            google_drive: (
                settings.google_drive.remote_path.trim().to_string(),
                settings.google_drive.root_folder_id.trim().to_string(),
            ),
            one_drive: settings.one_drive.remote_path.trim().to_string(),
            seedbox: (
                s.host.trim().to_string(),
                s.username.trim().to_string(),
                s.port,
                s.remote_path.trim().to_string(),
                s.read_only,
            ),
        }
    }
}

/// A field that can carry an error message.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Field {
    KeyId,
    Key,
    BucketName(usize),
    BucketDrive(usize),
    SeedboxHost,
    SeedboxPort,
    SeedboxUser,
    SeedboxPassword,
}

pub type Errors = BTreeMap<Field, String>;

fn is_single_folder_name(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains(['/', '\\']) && value != "." && value != ".."
}

/// Checks the B2 fields before mounting.
pub fn validate_b2(form: &Form, platform: Platform, has_saved_credentials: bool) -> Errors {
    let mut errors = Errors::new();
    let mut seen_buckets = BTreeSet::new();
    let mut seen_drives = BTreeSet::new();
    let mut named = 0;

    for (index, bucket) in form.buckets.iter().enumerate() {
        let name = bucket.name.text.trim();
        if name.is_empty() {
            continue;
        }
        named += 1;
        if !is_single_folder_name(name) {
            errors.insert(
                Field::BucketName(index),
                "Use one bucket name, without slashes".into(),
            );
        } else if !seen_buckets.insert(name.to_lowercase()) {
            errors.insert(
                Field::BucketName(index),
                "This bucket is already listed".into(),
            );
        }

        if platform == Platform::Windows {
            let letter = normalize_drive_letter(&bucket.drive);
            if letter.len() != 1 || !letter.chars().all(|c| c.is_ascii_uppercase()) {
                errors.insert(Field::BucketDrive(index), "Choose a drive letter".into());
            } else if RESERVED_DRIVE_LETTERS.contains(&letter.chars().next().unwrap_or('?')) {
                errors.insert(Field::BucketDrive(index), format!("{letter}: is reserved"));
            } else if !seen_drives.insert(letter) {
                errors.insert(
                    Field::BucketDrive(index),
                    "This drive is already used".into(),
                );
            }
        }
    }

    let key_id = form.b2_key_id.text.trim();
    let key = form.b2_key.text.trim();
    if (!key_id.is_empty() || !key.is_empty()) && (key_id.is_empty() || key.is_empty()) {
        if key_id.is_empty() {
            errors.insert(Field::KeyId, "Enter the key ID".into());
        }
        if key.is_empty() {
            errors.insert(Field::Key, "Enter the application key".into());
        }
    } else if named > 0 && key_id.is_empty() && key.is_empty() && !has_saved_credentials {
        errors.insert(Field::KeyId, "Enter your B2 key to mount buckets".into());
        errors.insert(Field::Key, "Enter your B2 key to mount buckets".into());
    }
    errors
}

/// Checks the Seedbox fields before testing or mounting.
pub fn validate_seedbox(form: &Form, require_password: bool, has_saved_password: bool) -> Errors {
    let mut errors = Errors::new();
    let host = normalized_host(&form.seedbox_host.text);
    if host.is_empty() || host.contains(['/', '\\']) {
        errors.insert(
            Field::SeedboxHost,
            "Enter a host name without a path".into(),
        );
    }
    if form.seedbox_user.text.trim().is_empty() {
        errors.insert(Field::SeedboxUser, "Enter the Seedbox username".into());
    }
    match parse_seedbox_port(&form.seedbox_port.text) {
        Some(port) if port >= 1 => {}
        _ => {
            errors.insert(Field::SeedboxPort, "Use a port from 1 to 65535".into());
        }
    }
    if require_password && form.seedbox_password.text.trim().is_empty() && !has_saved_password {
        errors.insert(Field::SeedboxPassword, "Enter the Seedbox password".into());
    }
    errors
}

/// Whether the Seedbox section has anything typed into it, which makes it
/// part of a mount.
pub fn seedbox_in_use(form: &Form) -> bool {
    !form.seedbox_host.text.trim().is_empty() || !form.seedbox_user.text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> AppSettings {
        AppSettings {
            buckets: vec![BucketMount {
                bucket_name: String::new(),
                drive_letter: "Z".into(),
            }],
            ..AppSettings::default()
        }
    }

    fn form(settings: &AppSettings, platform: Platform) -> Form {
        Form::from_settings(settings, platform, &[])
    }

    #[test]
    fn reserves_fixed_provider_drive_letters() {
        assert_eq!(RESERVED_DRIVE_LETTERS, ['G', 'O', 'S']);
    }

    #[test]
    fn prefers_z_when_it_is_available() {
        assert_eq!(next_available_drive_letter(["G", "O", "S"]), Some('Z'));
    }

    #[test]
    fn skips_reserved_configured_and_system_drive_letters() {
        assert_eq!(
            next_available_drive_letter(["G", "O", "S", "C", "Y", "z:"]),
            Some('X')
        );
    }

    #[test]
    fn normalizes_lowercase_letters_with_optional_colon() {
        assert_eq!(normalize_drive_letter(" z: "), "Z");
    }

    #[test]
    fn new_bucket_rows_take_the_next_free_windows_letter() {
        let mut form = form(&settings(), Platform::Windows);
        form.add_bucket(Platform::Windows, &['Y']);
        assert_eq!(form.buckets[1].drive, "X");
        let mut mac = Form::from_settings(&settings(), Platform::Mac, &[]);
        mac.add_bucket(Platform::Mac, &[]);
        assert_eq!(mac.buckets[1].drive, "");
    }

    #[test]
    fn removing_the_last_bucket_leaves_an_empty_row() {
        let mut form = form(&settings(), Platform::Mac);
        form.buckets[0].name.replace_text("photos");
        form.remove_bucket(0, Platform::Mac, &[]);
        assert_eq!(form.buckets.len(), 1);
        assert_eq!(form.buckets[0].name.text, "");
    }

    #[test]
    fn ignores_newly_added_blank_bucket_rows() {
        let before = settings();
        let mut after = settings();
        after.buckets.push(BucketMount {
            bucket_name: "   ".into(),
            drive_letter: "P".into(),
        });
        assert_eq!(
            MountSnapshot::of(&before, Platform::Mac),
            MountSnapshot::of(&after, Platform::Mac)
        );
    }

    #[test]
    fn detects_a_newly_typed_bucket() {
        let before = settings();
        let mut after = settings();
        after.buckets.push(BucketMount {
            bucket_name: "nocdn-main".into(),
            drive_letter: "P".into(),
        });
        assert_ne!(
            MountSnapshot::of(&before, Platform::Mac),
            MountSnapshot::of(&after, Platform::Mac)
        );
    }

    #[test]
    fn login_preferences_are_not_mount_changes() {
        let before = settings();
        let after = AppSettings {
            start_at_login: true,
            start_minimized: true,
            ..settings()
        };
        assert_eq!(
            MountSnapshot::of(&before, Platform::Mac),
            MountSnapshot::of(&after, Platform::Mac)
        );
    }

    #[test]
    fn detects_one_drive_remote_path_changes() {
        let before = settings();
        let after = AppSettings {
            one_drive: OneDriveSettings {
                remote_path: " Personal Docs ".into(),
            },
            ..settings()
        };
        assert_ne!(
            MountSnapshot::of(&before, Platform::Mac),
            MountSnapshot::of(&after, Platform::Mac)
        );
    }

    #[test]
    fn normalizes_windows_drive_letters_for_comparison() {
        let bucket = |name: &str, letter: &str| AppSettings {
            buckets: vec![BucketMount {
                bucket_name: name.into(),
                drive_letter: letter.into(),
            }],
            ..settings()
        };
        assert_eq!(
            MountSnapshot::of(&bucket("photos", "p"), Platform::Windows),
            MountSnapshot::of(&bucket(" photos ", "P:"), Platform::Windows)
        );
    }

    #[test]
    fn form_round_trips_settings() {
        let saved = AppSettings {
            buckets: vec![BucketMount {
                bucket_name: "nocdn-main".into(),
                drive_letter: String::new(),
            }],
            seedbox: SeedboxSettings {
                host: "box.example.com".into(),
                username: "me".into(),
                port: 2121,
                ..SeedboxSettings::default()
            },
            start_at_login: true,
            ..AppSettings::default()
        };
        assert_eq!(form(&saved, Platform::Mac).settings(Platform::Mac), saved);
    }

    #[test]
    fn b2_needs_both_halves_of_a_key() {
        let mut form = form(&settings(), Platform::Mac);
        form.b2_key_id.replace_text("id");
        let errors = validate_b2(&form, Platform::Mac, false);
        assert_eq!(
            errors.get(&Field::Key).unwrap(),
            "Enter the application key"
        );
        assert!(!errors.contains_key(&Field::KeyId));
    }

    #[test]
    fn b2_buckets_need_a_key_unless_one_is_saved() {
        let mut form = form(&settings(), Platform::Mac);
        form.buckets[0].name.replace_text("photos");
        assert!(validate_b2(&form, Platform::Mac, false).contains_key(&Field::KeyId));
        assert!(validate_b2(&form, Platform::Mac, true).is_empty());
    }

    #[test]
    fn b2_rejects_paths_duplicates_and_reserved_letters() {
        let mut form = form(&settings(), Platform::Windows);
        form.buckets[0].name.replace_text("a/b");
        form.add_bucket(Platform::Windows, &[]);
        form.buckets[1].name.replace_text("Photos");
        form.add_bucket(Platform::Windows, &[]);
        form.buckets[2].name.replace_text("photos");
        form.buckets[2].drive = "G".into();
        let errors = validate_b2(&form, Platform::Windows, true);
        assert!(errors.contains_key(&Field::BucketName(0)));
        assert_eq!(
            errors.get(&Field::BucketName(2)).unwrap(),
            "This bucket is already listed"
        );
        assert_eq!(
            errors.get(&Field::BucketDrive(2)).unwrap(),
            "G: is reserved"
        );
    }

    #[test]
    fn seedbox_checks_host_user_port_and_password() {
        let mut form = form(&settings(), Platform::Mac);
        form.seedbox_host.replace_text("https://box.example.com/");
        form.seedbox_port.replace_text("70000");
        let errors = validate_seedbox(&form, true, false);
        assert!(!errors.contains_key(&Field::SeedboxHost));
        assert!(errors.contains_key(&Field::SeedboxUser));
        assert!(errors.contains_key(&Field::SeedboxPort));
        assert!(errors.contains_key(&Field::SeedboxPassword));
        assert!(!validate_seedbox(&form, true, true).contains_key(&Field::SeedboxPassword));
    }

    #[test]
    fn seedbox_host_drops_scheme_and_slashes() {
        assert_eq!(
            normalized_host(" ftps://box.example.com// "),
            "box.example.com"
        );
    }

    #[test]
    fn seedbox_port_defaults_when_empty() {
        assert_eq!(parse_seedbox_port(""), Some(22));
        assert_eq!(parse_seedbox_port("2a"), None);
        assert_eq!(parse_seedbox_port("2121"), Some(2121));
    }
}
