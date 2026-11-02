// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod backend;
mod credentials;
mod host;
mod logging;
mod models;
mod notifications;
mod paths;
mod rclone;
mod settings;
mod single_instance;
mod tray;
mod ui;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn set_test_dirs(app_data_dir: &std::path::Path, log_dir: &std::path::Path) {
        std::env::set_var("CLOUD_DRIVE_MOUNT_APP_DATA_DIR", app_data_dir);
        std::env::set_var("CLOUD_DRIVE_MOUNT_LOG_DIR", log_dir);
    }

    pub(crate) fn clear_test_dirs() {
        std::env::remove_var("CLOUD_DRIVE_MOUNT_APP_DATA_DIR");
        std::env::remove_var("CLOUD_DRIVE_MOUNT_LOG_DIR");
    }
}

use std::sync::Arc;

use backend::Backend;
use host::Host;
use single_instance::Instance;
use tray::{Tray, TrayAction};

const ARG_CLEAN_RESTART: &str = "--clean-restart";
const ARG_SHOW_SETTINGS: &str = "--show-settings";
/// Rune's headless tooling: render fixture scenes instead of running.
const TOOLING_ARGS: &[&str] = &["--snap", "--tree", "--list-scenes", "--video", "--script"];

fn should_show_on_launch(launch_args: &[String], start_minimized: bool) -> bool {
    launch_args.iter().any(|arg| arg == ARG_SHOW_SETTINGS) || !start_minimized
}

fn is_tooling(launch_args: &[String]) -> bool {
    launch_args
        .iter()
        .any(|arg| TOOLING_ARGS.contains(&arg.as_str()))
}

fn main() {
    let launch_args: Vec<String> = std::env::args().skip(1).collect();
    if is_tooling(&launch_args) {
        // Fixtures only: never reads the user's settings, keychain or mounts.
        ui::app(None, true).args(launch_args).run();
        return;
    }

    let listener = match single_instance::acquire() {
        Instance::Secondary => return,
        Instance::Primary(listener) => listener,
    };

    let backend = Backend::new(Host::new());
    if launch_args.iter().any(|arg| arg == ARG_CLEAN_RESTART) {
        if let Err(err) = backend.clear_logs() {
            eprintln!("Could not clear logs during clean restart: {err}");
        }
    }
    backend.log_info("Cloud Drive Mount starting.");

    let show_on_launch =
        should_show_on_launch(&launch_args, settings::load_settings().start_minimized);
    let app = ui::app(Some(backend.clone()), show_on_launch);
    let window = app.handle();

    if let Some(listener) = listener {
        let window = window.clone();
        Instance::serve(listener, move || ui::bring_forward(&window));
    }
    let tray = Tray::install(Arc::new({
        let window = window.clone();
        move |action| match action {
            TrayAction::OpenSettings => ui::bring_forward(&window),
            TrayAction::Quit => window.quit(),
        }
    }));

    app.run();

    // The window is gone; unmount before the process ends.
    tray.set_quitting();
    backend.shut_down();
    backend.log_info("Cloud Drive Mount quit.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn launch_visibility_respects_explicit_show_and_start_minimized() {
        assert!(!should_show_on_launch(&args(&[]), true));
        assert!(should_show_on_launch(&args(&[]), false));
        assert!(should_show_on_launch(&args(&["--show-settings"]), true));
        assert!(!should_show_on_launch(&args(&["--autostart"]), true));
        assert!(should_show_on_launch(&args(&["--autostart"]), false));
        assert!(should_show_on_launch(
            &args(&["--show-settings", "--clean-restart"]),
            true,
        ));
    }

    #[test]
    fn rune_tooling_flags_render_fixtures_instead_of_running() {
        assert!(is_tooling(&args(&[
            "--snap",
            "/tmp/x.png",
            "--scene",
            "drives"
        ])));
        assert!(is_tooling(&args(&["--list-scenes"])));
        assert!(!is_tooling(&args(&["--autostart"])));
    }
}
