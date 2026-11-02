//! The backend's line to the window: mount state changes and log lines go
//! through a [`Host`], which hands them to whatever listener the window
//! installed. The backend never knows about the UI framework.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::models::{LogLine, MountState};

/// Something the backend reports while it works.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    MountState(MountState),
    Log(LogLine),
}

type Listener = Box<dyn Fn(HostEvent) + Send + Sync>;

/// A cheap, cloneable handle the backend reports through.
#[derive(Clone, Default)]
pub struct Host {
    listener: Arc<RwLock<Option<Listener>>>,
}

impl Host {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends every later event to `listener`, replacing any earlier one.
    pub fn set_listener(&self, listener: impl Fn(HostEvent) + Send + Sync + 'static) {
        if let Ok(mut slot) = self.listener.write() {
            *slot = Some(Box::new(listener));
        }
    }

    fn emit(&self, event: HostEvent) {
        if let Ok(slot) = self.listener.read() {
            if let Some(listener) = slot.as_ref() {
                listener(event);
            }
        }
    }

    pub fn emit_mount_state(&self, state: MountState) {
        self.emit(HostEvent::MountState(state));
    }

    pub fn emit_log(&self, line: LogLine) {
        self.emit(HostEvent::Log(line));
    }

    /// Where the packaged app keeps its bundled resources: `Contents/Resources`
    /// in a macOS bundle, the executable's folder elsewhere.
    pub fn resource_dir(&self) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        if cfg!(target_os = "macos") {
            let resources = dir.parent()?.join("Resources");
            if resources.is_dir() {
                return Some(resources);
            }
        }
        Some(dir.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn events_reach_the_installed_listener() {
        let host = Host::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        host.emit_mount_state(MountState::default());
        let sink = seen.clone();
        host.set_listener(move |event| sink.lock().unwrap().push(event));
        host.emit_log(LogLine {
            level: "INFO".into(),
            message: "hello".into(),
            timestamp: "12:00:00".into(),
        });
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "events before a listener are dropped");
        assert!(matches!(&seen[0], HostEvent::Log(line) if line.message == "hello"));
    }
}
