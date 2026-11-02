//! Start at login: a LaunchAgent on macOS, the `Run` registry key on Windows.
//! Uses the same name, arguments and launcher the Tauri build registered, so
//! an existing login item keeps working after the upgrade.

use auto_launch::{AutoLaunch, AutoLaunchBuilder};

pub const ARG_AUTOSTART: &str = "--autostart";
const APP_NAME: &str = "Cloud Drive Mount";

fn launcher() -> Result<AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(APP_NAME)
        .set_app_path(&exe.display().to_string())
        .set_args(&[ARG_AUTOSTART]);
    #[cfg(target_os = "macos")]
    builder.set_use_launch_agent(true);
    builder.build().map_err(|e| e.to_string())
}

/// Turns start at login on or off. Turning it on always registers this
/// executable again, so a login item left by an older build (the Tauri app,
/// a moved bundle) points at the copy that is running now.
pub fn apply(enabled: bool) -> Result<(), String> {
    let launcher = launcher()?;
    // A development build only registers itself when nothing is, so running
    // it never takes the login item from the installed app.
    if enabled && (!cfg!(debug_assertions) || !launcher.is_enabled().unwrap_or(false)) {
        return launcher.enable().map_err(|e| e.to_string());
    }
    if enabled {
        return Ok(());
    }
    if launcher.is_enabled().map_err(|e| e.to_string())? {
        launcher.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}
