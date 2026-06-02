use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;
use tauri::{AppHandle, Emitter};

use crate::models::LogLine;
use crate::paths::log_dir;

static LOG_LOCK: Mutex<()> = Mutex::new(());
const MAX_LOG_SIZE_BYTES: u64 = 5 * 1024 * 1024;

pub struct LogEmitter {
    app: Option<AppHandle>,
}

impl LogEmitter {
    pub fn new(app: Option<AppHandle>) -> Self {
        Self { app }
    }

    pub fn set_app(&mut self, app: AppHandle) {
        self.app = Some(app);
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.write("INFO", message.as_ref());
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.write("ERROR", message.as_ref());
    }

    pub fn write(&self, level: &str, message: &str) {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        let line = format!("[{timestamp}] [{level}] {message}");

        if let Ok(_guard) = LOG_LOCK.lock() {
            if let Err(e) = write_to_file(&line) {
                eprintln!("Log write failed: {e}");
            }
        }

        if let Some(app) = &self.app {
            let _ = app.emit(
                "log-line",
                LogLine {
                    level: level.to_string(),
                    message: message.to_string(),
                    timestamp,
                },
            );
        }
    }

    pub fn clear(&self) -> Result<(), String> {
        let _guard = LOG_LOCK.lock().map_err(|e| e.to_string())?;
        let dir = log_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(log_file_path(), "").map_err(|e| e.to_string())?;
        let old = old_log_file_path();
        if old.exists() {
            let _ = fs::remove_file(old);
        }
        Ok(())
    }
}

fn log_file_path() -> PathBuf {
    log_dir().join("app.log")
}

fn old_log_file_path() -> PathBuf {
    log_dir().join("app.old.log")
}

fn write_to_file(line: &str) -> Result<(), String> {
    let dir = log_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    rotate_if_needed()?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path())
        .map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())
}

fn rotate_if_needed() -> Result<(), String> {
    let path = log_file_path();
    if !path.exists() {
        return Ok(());
    }
    let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if size <= MAX_LOG_SIZE_BYTES {
        return Ok(());
    }
    let old = old_log_file_path();
    if old.exists() {
        fs::remove_file(&old).map_err(|e| e.to_string())?;
    }
    fs::rename(&path, old).map_err(|e| e.to_string())
}

pub fn redact_sensitive_line(line: &str) -> String {
    let mut result = line.to_string();
    for marker in ["key=", "pass=", "token=", "secret="] {
        let mut search_from = 0;
        loop {
            let lower = result.to_lowercase();
            let Some(relative_idx) = lower[search_from..].find(marker) else {
                break;
            };
            let value_start = search_from + relative_idx + marker.len();
            let rest = &result[value_start..];
            let value_len = rest.find([' ', '\t', '"', '\'', '&']).unwrap_or(rest.len());
            if value_len == 0 {
                search_from = value_start;
                continue;
            }
            result.replace_range(value_start..value_start + value_len, "***");
            search_from = value_start + 3;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_sensitive_line_masks_common_secret_markers() {
        assert_eq!(
            redact_sensitive_line("account=abc key=super-secret bucket=photos"),
            "account=abc key=*** bucket=photos"
        );
        assert_eq!(
            redact_sensitive_line("pass=hunter2 token=abc123 secret=top"),
            "pass=*** token=*** secret=***"
        );
        assert_eq!(
            redact_sensitive_line("TOKEN=abc&key=def"),
            "TOKEN=***&key=***"
        );
        assert_eq!(redact_sensitive_line("key=end-of-line"), "key=***");
    }

    #[test]
    fn redact_sensitive_line_preserves_non_secret_text() {
        assert_eq!(
            redact_sensitive_line("notice: mounted bucket at /Volumes/photos"),
            "notice: mounted bucket at /Volumes/photos"
        );
        assert_eq!(
            redact_sensitive_line("key= pass= token="),
            "key= pass= token="
        );
    }

    #[test]
    fn log_emitter_writes_info_error_and_clear_resets_files() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        let emitter = LogEmitter::new(None);
        emitter.info("started");
        emitter.error("failed");

        let content = fs::read_to_string(log_file_path()).unwrap();
        assert!(content.contains("[INFO] started"));
        assert!(content.contains("[ERROR] failed"));

        fs::write(old_log_file_path(), "old").unwrap();
        emitter.clear().unwrap();

        assert_eq!(fs::read_to_string(log_file_path()).unwrap(), "");
        assert!(!old_log_file_path().exists());

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn oversized_log_rotates_before_next_write() {
        let _guard = crate::test_support::env_lock();
        crate::test_support::clear_test_dirs();

        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));
        fs::create_dir_all(log_dir()).unwrap();
        fs::write(
            log_file_path(),
            vec![b'x'; (MAX_LOG_SIZE_BYTES + 1) as usize],
        )
        .unwrap();

        LogEmitter::new(None).info("after rotation");

        assert!(old_log_file_path().exists());
        assert_eq!(
            fs::metadata(old_log_file_path()).unwrap().len(),
            MAX_LOG_SIZE_BYTES + 1
        );
        let content = fs::read_to_string(log_file_path()).unwrap();
        assert!(content.contains("[INFO] after rotation"));

        crate::test_support::clear_test_dirs();
    }
}
