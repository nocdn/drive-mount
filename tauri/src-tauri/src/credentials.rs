use crate::models::B2Credentials;

const SERVICE: &str = "com.bartek.clouddrivemount.b2credentials";
const ACCOUNT: &str = "backblaze-b2";

pub fn load_b2_credentials() -> Result<Option<B2Credentials>, String> {
    if let Some(creds) = load_from_keyring()? {
        return Ok(Some(creds));
    }

    #[cfg(windows)]
    {
        if let Some(creds) = load_legacy_dpapi()? {
            let _ = save_b2_credentials(&creds);
            return Ok(Some(creds));
        }
    }

    Ok(None)
}

pub fn save_b2_credentials(credentials: &B2Credentials) -> Result<(), String> {
    let json = serde_json::to_string(credentials).map_err(|e| e.to_string())?;
    keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| e.to_string())?
        .set_password(&json)
        .map_err(|e| e.to_string())
}

fn load_from_keyring() -> Result<Option<B2Credentials>, String> {
    match keyring::Entry::new(SERVICE, ACCOUNT) {
        Ok(entry) => match entry.get_password() {
            Ok(json) => {
                let creds: B2Credentials =
                    serde_json::from_str(&json).map_err(|_| "Invalid saved B2 credentials".to_string())?;
                Ok(Some(creds))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        },
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(windows)]
fn load_legacy_dpapi() -> Result<Option<B2Credentials>, String> {
    use std::fs;
    use std::path::PathBuf;

    let path: PathBuf = crate::paths::app_data_dir().join("credentials").join("b2.bin");
    if !path.exists() {
        return Ok(None);
    }

    let encrypted = fs::read(&path).map_err(|e| e.to_string())?;
    let entropy = b"CloudDriveMount.WindowsSecureStore.v1";
    let decrypted = dpapi_decrypt(&encrypted, Some(entropy)).map_err(|e| e.to_string())?;
    let json = String::from_utf8(decrypted).map_err(|_| "Invalid legacy credential file".to_string())?;
    let creds: B2Credentials =
        serde_json::from_str(&json).map_err(|_| "Invalid legacy credential JSON".to_string())?;
    Ok(Some(creds))
}

#[cfg(windows)]
fn dpapi_decrypt(data: &[u8], entropy: Option<&[u8]>) -> Result<Vec<u8>, String> {
    use std::ptr;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    unsafe {
        let mut input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        let mut entropy_blob = entropy.map(|e| CRYPT_INTEGER_BLOB {
            cbData: e.len() as u32,
            pbData: e.as_ptr() as *mut u8,
        });

        let result = CryptUnprotectData(
            &mut input,
            None,
            entropy_blob.as_mut().map(|b| b as *mut _),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );

        if result.is_err() {
            return Err("DPAPI decrypt failed".to_string());
        }

        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let out = slice.to_vec();
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(output.pbData as _));
        Ok(out)
    }
}

#[cfg(not(windows))]
fn load_legacy_dpapi() -> Result<Option<B2Credentials>, String> {
    Ok(None)
}
