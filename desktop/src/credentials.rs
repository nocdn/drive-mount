use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

use crate::models::B2Credentials;

#[cfg(not(test))]
const CREDENTIALS_SERVICE: &str = "com.bartek.clouddrivemount.credentials";
#[cfg(not(test))]
const CREDENTIALS_ACCOUNT: &str = "cloud-drive-mount";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SecureCredentials {
    b2: Option<B2Credentials>,
    seedbox_password: Option<String>,
    google_drive_config: Option<Vec<String>>,
    one_drive_config: Option<Vec<String>>,
}

impl SecureCredentials {
    fn is_empty(&self) -> bool {
        self.b2.is_none()
            && self
                .seedbox_password
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            && self
                .google_drive_config
                .as_ref()
                .is_none_or(|lines| lines.is_empty())
            && self
                .one_drive_config
                .as_ref()
                .is_none_or(|lines| lines.is_empty())
    }
}

static CREDENTIALS_CACHE: OnceLock<Mutex<Option<SecureCredentials>>> = OnceLock::new();

fn credentials_cache() -> &'static Mutex<Option<SecureCredentials>> {
    CREDENTIALS_CACHE.get_or_init(|| Mutex::new(None))
}

pub fn load_b2_credentials() -> Result<Option<B2Credentials>, String> {
    Ok(load_credentials_bundle()?.b2)
}

pub fn save_b2_credentials(credentials: &B2Credentials) -> Result<(), String> {
    update_credentials_bundle(|bundle| {
        bundle.b2 = Some(credentials.clone());
    })
}

pub fn load_seedbox_password() -> Result<Option<String>, String> {
    Ok(load_credentials_bundle()?
        .seedbox_password
        .filter(|password| !password.is_empty()))
}

pub fn save_seedbox_password(password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Ok(());
    }

    update_credentials_bundle(|bundle| {
        bundle.seedbox_password = Some(password.to_string());
    })
}

pub fn delete_seedbox_password() -> Result<(), String> {
    update_credentials_bundle(|bundle| {
        bundle.seedbox_password = None;
    })
}

pub fn has_saved_seedbox_password() -> Result<bool, String> {
    Ok(load_seedbox_password()?.is_some())
}

#[cfg_attr(test, allow(dead_code))]
pub fn load_google_drive_config() -> Result<Option<Vec<String>>, String> {
    Ok(load_credentials_bundle()?
        .google_drive_config
        .filter(|lines| !lines.is_empty()))
}

pub fn save_google_drive_config(lines: &[String]) -> Result<(), String> {
    update_credentials_bundle(|bundle| {
        bundle.google_drive_config = if lines.is_empty() {
            None
        } else {
            Some(lines.to_vec())
        };
    })
}

pub fn delete_google_drive_config() -> Result<(), String> {
    update_credentials_bundle(|bundle| {
        bundle.google_drive_config = None;
    })
}

#[cfg_attr(test, allow(dead_code))]
pub fn has_saved_google_drive_config() -> Result<bool, String> {
    Ok(load_google_drive_config()?.is_some())
}

#[cfg_attr(test, allow(dead_code))]
pub fn load_one_drive_config() -> Result<Option<Vec<String>>, String> {
    Ok(load_credentials_bundle()?
        .one_drive_config
        .filter(|lines| !lines.is_empty()))
}

pub fn save_one_drive_config(lines: &[String]) -> Result<(), String> {
    update_credentials_bundle(|bundle| {
        bundle.one_drive_config = if lines.is_empty() {
            None
        } else {
            Some(lines.to_vec())
        };
    })
}

pub fn delete_one_drive_config() -> Result<(), String> {
    update_credentials_bundle(|bundle| {
        bundle.one_drive_config = None;
    })
}

#[cfg_attr(test, allow(dead_code))]
pub fn has_saved_one_drive_config() -> Result<bool, String> {
    Ok(load_one_drive_config()?.is_some())
}

fn load_credentials_bundle() -> Result<SecureCredentials, String> {
    let mut cached = credentials_cache().lock().map_err(|e| e.to_string())?;
    if let Some(bundle) = cached.clone() {
        return Ok(bundle);
    }

    let bundle = read_credentials_bundle()?;
    *cached = Some(bundle.clone());
    Ok(bundle)
}

fn read_credentials_bundle() -> Result<SecureCredentials, String> {
    match store::read()? {
        Some(json) if json.trim().is_empty() => Ok(SecureCredentials::default()),
        Some(json) => serde_json::from_str(&json)
            .map_err(|_| "Invalid saved Cloud Drive Mount credentials".to_string()),
        None => Ok(SecureCredentials::default()),
    }
}

fn save_credentials_bundle(bundle: &SecureCredentials) -> Result<(), String> {
    if bundle.is_empty() {
        store::delete()?;
        *credentials_cache().lock().map_err(|e| e.to_string())? =
            Some(SecureCredentials::default());
        return Ok(());
    }

    let json = serde_json::to_string(bundle).map_err(|e| e.to_string())?;
    store::write(&json)?;

    *credentials_cache().lock().map_err(|e| e.to_string())? = Some(bundle.clone());
    Ok(())
}

/// The saved bundle's JSON: in the system keychain (the Keychain on macOS,
/// Credential Manager on Windows).
#[cfg(not(test))]
mod store {
    use super::{CREDENTIALS_ACCOUNT, CREDENTIALS_SERVICE};

    fn entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new(CREDENTIALS_SERVICE, CREDENTIALS_ACCOUNT).map_err(|e| e.to_string())
    }

    pub(super) fn read() -> Result<Option<String>, String> {
        match entry()?.get_password() {
            Ok(json) => Ok(Some(json)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub(super) fn write(json: &str) -> Result<(), String> {
        entry()?.set_password(json).map_err(|e| e.to_string())
    }

    pub(super) fn delete() -> Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Tests keep the bundle in memory: they must never read the user's real
/// keychain item (each rebuilt test binary would make macOS ask for the
/// keychain password again) or overwrite it.
#[cfg(test)]
mod store {
    use std::sync::Mutex;

    static SAVED: Mutex<Option<String>> = Mutex::new(None);

    fn saved() -> std::sync::MutexGuard<'static, Option<String>> {
        SAVED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn read() -> Result<Option<String>, String> {
        Ok(saved().clone())
    }

    pub(super) fn write(json: &str) -> Result<(), String> {
        *saved() = Some(json.to_string());
        Ok(())
    }

    pub(super) fn delete() -> Result<(), String> {
        *saved() = None;
        Ok(())
    }
}

fn update_credentials_bundle<F>(update: F) -> Result<(), String>
where
    F: FnOnce(&mut SecureCredentials),
{
    let mut bundle = load_credentials_bundle()?;
    update(&mut bundle);
    save_credentials_bundle(&bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_credentials_detects_empty_bundle() {
        assert!(SecureCredentials::default().is_empty());
        assert!(SecureCredentials {
            seedbox_password: Some(String::new()),
            google_drive_config: Some(Vec::new()),
            one_drive_config: Some(Vec::new()),
            ..SecureCredentials::default()
        }
        .is_empty());

        assert!(!SecureCredentials {
            seedbox_password: Some("secret".to_string()),
            ..SecureCredentials::default()
        }
        .is_empty());
        assert!(!SecureCredentials {
            google_drive_config: Some(vec!["type = drive".to_string()]),
            ..SecureCredentials::default()
        }
        .is_empty());
        assert!(!SecureCredentials {
            one_drive_config: Some(vec!["type = onedrive".to_string()]),
            ..SecureCredentials::default()
        }
        .is_empty());
    }

    #[test]
    fn tests_keep_credentials_out_of_the_keychain() {
        store::write("{\"seedboxPassword\":\"secret\"}").unwrap();
        assert_eq!(
            read_credentials_bundle()
                .unwrap()
                .seedbox_password
                .as_deref(),
            Some("secret")
        );
        store::delete().unwrap();
        assert!(read_credentials_bundle().unwrap().is_empty());
    }

    #[test]
    fn secure_credentials_deserializes_partial_json() {
        let bundle: SecureCredentials =
            serde_json::from_str(r#"{"b2":{"applicationKeyId":"id","applicationKey":"key"}}"#)
                .unwrap();

        assert_eq!(bundle.b2.unwrap().application_key_id, "id");
        assert_eq!(bundle.seedbox_password, None);
        assert_eq!(bundle.google_drive_config, None);
        assert_eq!(bundle.one_drive_config, None);
    }
}
