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
        if let Some(idx) = result.to_lowercase().find(marker) {
            let rest = &result[idx + marker.len()..];
            if let Some(end) = rest.find([' ', '\t', '"']) {
                result.replace_range(idx + marker.len()..idx + marker.len() + end, "***");
            }
        }
    }
    result
}
