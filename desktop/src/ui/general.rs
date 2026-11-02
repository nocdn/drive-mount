//! The General pane: how the app starts, and what it depends on.

use rune::El;

use super::controls::{page, section, setting_row, toggle_row, tone_tag, Paint, Tone};
use super::form::Platform;
use super::{failure_banner, scroll_body, Ui};

pub(super) fn pane(ui: &Ui, p: Paint) -> El {
    let f = &ui.form;
    let startup = section(
        p,
        "startup",
        "Startup",
        vec![
            toggle_row(
                p,
                "start-at-login",
                "Start at login",
                Some("Mount your drives when you log in"),
                f.start_at_login,
                |ui: &mut Ui| {
                    ui.form.start_at_login = !ui.form.start_at_login;
                    ui.save_preferences();
                },
            ),
            toggle_row(
                p,
                "start-minimized",
                match ui.platform {
                    Platform::Mac => "Start in the menu bar",
                    Platform::Windows => "Start in the tray",
                },
                Some("Open without showing this window"),
                f.start_minimized,
                |ui: &mut Ui| {
                    ui.form.start_minimized = !ui.form.start_minimized;
                    ui.save_preferences();
                },
            ),
        ],
    );
    let (fuse, fuse_description) = match ui.platform {
        Platform::Mac => ("FUSE-T", "Lets rclone mount drives in Finder"),
        Platform::Windows => ("WinFsp", "Lets rclone mount drives in File Explorer"),
    };
    let fuse_status = match ui.fuse_installed {
        Some(true) => tone_tag(p, "status", "Installed", Tone::Success),
        Some(false) => tone_tag(p, "status", "Not installed", Tone::Danger),
        None => tone_tag(p, "status", "Checking…", Tone::Neutral),
    };
    let system = section(
        p,
        "system",
        "System",
        vec![setting_row(
            p,
            "fuse",
            fuse,
            Some(fuse_description),
            fuse_status,
        )],
    );
    scroll_body(
        "general-body",
        page("general")
            .children(failure_banner(ui, p))
            .child(startup)
            .child(system),
    )
}
