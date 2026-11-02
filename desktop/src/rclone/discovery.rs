use std::path::PathBuf;

use crate::host::Host;

pub fn find_bundled_rclone(app: &Host) -> Option<PathBuf> {
    for name in sidecar_filenames() {
        if let Some(resource_dir) = app.resource_dir() {
            let candidate = resource_dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        // Development builds use the sidecar `bun run prepare:sidecars` downloaded.
        if cfg!(debug_assertions) {
            let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

fn sidecar_filenames() -> &'static [&'static str] {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        &["rclone", "rclone-aarch64-apple-darwin"]
    }
    #[cfg(windows)]
    {
        &["rclone.exe", "rclone-x86_64-pc-windows-msvc.exe"]
    }
    #[cfg(not(any(all(target_os = "macos", target_arch = "aarch64"), windows)))]
    {
        &[] as &[&str]
    }
}

pub fn find_rclone_in_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let name = "rclone.exe";
    #[cfg(not(windows))]
    let name = "rclone";

    if let Ok(path_var) = std::env::var("PATH") {
        #[cfg(windows)]
        let sep = ';';
        #[cfg(not(windows))]
        let sep = ':';

        for dir in path_var.split(sep) {
            let candidate = PathBuf::from(dir.trim()).join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for candidate in [
            "/opt/homebrew/bin/rclone",
            "/usr/local/bin/rclone",
            "/usr/bin/rclone",
        ] {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::sidecar_filenames;

    #[test]
    #[cfg(windows)]
    fn windows_sidecar_filenames_include_installed_name() {
        assert_eq!(
            sidecar_filenames(),
            &["rclone.exe", "rclone-x86_64-pc-windows-msvc.exe"]
        );
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn apple_silicon_sidecar_filename_uses_target_triple() {
        assert_eq!(
            sidecar_filenames(),
            &["rclone", "rclone-aarch64-apple-darwin"]
        );
    }
}
