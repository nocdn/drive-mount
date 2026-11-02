//! The Drives pane: every configured drive and whether it's mounted, and
//! the mount actions every storage pane shows in its title bar.

use rune::{el, row, text, El, Icon};
use rune_kit::Dialog;

use crate::models::{CloudProvider, MountEntry};

use super::controls::{
    button, icon_row, message, note, overflow, page, pulsing_dot, section, small_button, tone_tag,
    Action, ButtonKind, Paint, Tone,
};
use super::form::Platform;
use super::{failure_banner, icons, scroll_body, Menu, Ui};

/// Asks before Unmount all, which cuts off any app using the drives.
fn confirm_unmount(ui: &Ui, p: Paint, layer: &mut Vec<El>) {
    if !ui.confirm_unmount || !ui.mounted() {
        return;
    }
    let drives = match ui.mount.mounts.len() {
        1 => "your drive".to_string(),
        n => format!("all {n} drives"),
    };
    let description = format!(
        "This unmounts {drives}, so apps using files on them lose access. \
         Anything still uploading finishes the next time you mount."
    );
    let buttons = row()
        .key("buttons")
        .px(16.0)
        .pb(16.0)
        .gap(8.0)
        .child(el().key("spacer").grow(1.0))
        .child(button(
            p,
            "cancel",
            "Cancel",
            ButtonKind::Secondary,
            Some(Box::new(|ui: &mut Ui, _: &mut rune::Tx| {
                ui.confirm_unmount = false
            })),
        ))
        .child(button(
            p,
            "confirm",
            "Unmount all",
            ButtonKind::Primary,
            Some(Box::new(|ui: &mut Ui, _: &mut rune::Tx| {
                ui.confirm_unmount = false;
                ui.unmount_all();
            })),
        ));
    // The kit's own description line doesn't wrap, so the copy goes in the
    // body.
    let copy = text(description)
        .key("description")
        .font(p.k.ty.ui.lh(20.0))
        .color(p.muted())
        .wrap()
        .px(16.0)
        .pb(16.0);
    Dialog::new("unmount", "Unmount all drives?")
        .width(380.0)
        .fit()
        .open(true)
        .child(copy)
        .child(buttons)
        .on_close(|ui: &mut Ui| ui.confirm_unmount = false)
        .build(p.k, layer);
}

/// The title bar's mount actions: a progress line while one runs, Save and
/// mount all as the one primary action, Unmount all while mounted, and the
/// rarer ones in the overflow menu.
pub(super) fn actions(ui: &Ui, p: Paint, layer: &mut Vec<El>) -> Vec<El> {
    let mut actions = Vec::new();
    if let Some(operation) = ui.operation {
        actions.push(
            row()
                .key("progress")
                .gap(8.0)
                .center_y()
                .mr(4.0)
                .child(pulsing_dot(p.k.c.text_muted))
                .child(
                    text(operation.progress())
                        .key("label")
                        .font(p.k.ty.ui)
                        .color(p.muted()),
                ),
        );
    }
    let idle = ui.operation.is_none();
    let mounted = ui.mounted();
    actions.push(overflow(
        p,
        "mount-actions",
        ui.open == Some(Menu::MountActions),
        vec![
            Action {
                label: "Refresh folder listings".into(),
                disabled: !idle || !mounted,
                run: Box::new(|ui: &mut Ui| ui.refresh()),
            },
            Action {
                label: "Restart mounts".into(),
                disabled: !idle,
                run: Box::new(|ui: &mut Ui| ui.restart()),
            },
        ],
        |ui: &mut Ui| {
            ui.open = (ui.open != Some(Menu::MountActions)).then_some(Menu::MountActions);
        },
        layer,
    ));
    if mounted {
        actions.push(button(
            p,
            "unmount-all",
            "Unmount all",
            ButtonKind::Secondary,
            idle.then(|| Box::new(|ui: &mut Ui, _: &mut rune::Tx| ui.confirm_unmount = true) as _),
        ));
    }
    confirm_unmount(ui, p, layer);
    let changes = ui.has_mount_changes();
    if !mounted || changes || ui.operation == Some(super::Operation::Mounting) {
        let label = if mounted && changes {
            "Save and remount"
        } else {
            "Save and mount all"
        };
        actions.push(button(
            p,
            "mount-all",
            label,
            ButtonKind::Primary,
            idle.then(|| Box::new(|ui: &mut Ui, _: &mut rune::Tx| ui.mount_all()) as _),
        ));
    }
    actions
}

/// A drive the settings describe, mounted or not.
struct Drive {
    key: String,
    icon: &'static Icon,
    title: String,
    /// Where it mounts, as the user knows it (`~/Drives/photos`, `Z:`).
    place: String,
    provider: CloudProvider,
    /// The rclone mount's label, to find it among the running mounts.
    label: String,
}

fn configured(ui: &Ui) -> Vec<Drive> {
    let mut drives = Vec::new();
    for (index, bucket) in ui.form.buckets.iter().enumerate() {
        let name = bucket.name.text.trim();
        if name.is_empty() {
            continue;
        }
        drives.push(Drive {
            key: format!("bucket-{index}"),
            icon: &icons::BUCKET,
            title: name.to_string(),
            place: place(ui.platform, name, &bucket.drive),
            provider: CloudProvider::BackblazeB2,
            label: format!("B2 {name}"),
        });
    }
    let fixed = [
        (
            ui.credentials.google_drive,
            "google-drive",
            &icons::GOOGLE_DRIVE,
            "Google Drive",
            "G",
            CloudProvider::GoogleDrive,
        ),
        (
            ui.credentials.one_drive,
            "onedrive",
            &icons::CLOUD,
            "OneDrive",
            "O",
            CloudProvider::OneDrive,
        ),
        (
            ui.credentials.seedbox,
            "seedbox",
            &icons::SERVER,
            "Seedbox",
            "S",
            CloudProvider::Seedbox,
        ),
    ];
    for (on, folder, icon, title, letter, provider) in fixed {
        if on {
            drives.push(Drive {
                key: folder.to_string(),
                icon,
                title: title.to_string(),
                place: place(ui.platform, folder, letter),
                provider,
                label: title.to_string(),
            });
        }
    }
    drives
}

/// Where a drive mounts, in the words Finder or Explorer would use.
pub(super) fn place(platform: Platform, folder: &str, letter: &str) -> String {
    match platform {
        Platform::Mac => format!("~/Drives/{folder}"),
        Platform::Windows if letter.is_empty() => format!("{folder} (no drive letter)"),
        Platform::Windows => format!("{letter}: · {folder}"),
    }
}

fn drive_row(p: Paint, drive: &Drive, running: Option<&MountEntry>) -> El {
    let control = match running {
        Some(entry) => {
            let target = entry.target.clone();
            row()
                .key("controls")
                .gap(8.0)
                .center_y()
                .child(tone_tag(p, "status", "Mounted", Tone::Success))
                .child(small_button(
                    p,
                    "open",
                    "Open",
                    ButtonKind::Secondary,
                    Some(Box::new(move |ui: &mut Ui, _: &mut rune::Tx| {
                        ui.open_target(target.clone())
                    })),
                ))
        }
        None => tone_tag(p, "status", "Not mounted", Tone::Neutral),
    };
    icon_row(
        p,
        &drive.key,
        drive.icon,
        &drive.title,
        Some((drive.place.clone(), p.muted())),
        control,
    )
}

pub(super) fn pane(ui: &Ui, p: Paint) -> El {
    let mut page = page("drives");
    if ui.fuse_installed == Some(false) {
        let (title, detail) = match ui.platform {
            Platform::Mac => (
                "FUSE-T isn't installed",
                "Install it with `brew install --cask fuse-t`, or from fuse-t.org, then restart Cloud Drive Mount.",
            ),
            Platform::Windows => (
                "WinFsp isn't installed",
                "Install WinFsp, then restart Cloud Drive Mount.",
            ),
        };
        page = page.child(super::controls::error_panel(p, "fuse", title, detail, None));
    }
    page = page.children(failure_banner(ui, p));

    let drives = configured(ui);
    let mut rows: Vec<El> = drives
        .iter()
        .map(|drive| {
            let running = ui
                .mount
                .mounts
                .iter()
                .find(|entry| entry.provider == drive.provider && entry.label == drive.label);
            drive_row(p, drive, running)
        })
        .collect();
    // Mounts the fields no longer describe (a bucket renamed while mounted).
    for (index, entry) in ui.mount.mounts.iter().enumerate() {
        let described = drives
            .iter()
            .any(|drive| entry.provider == drive.provider && entry.label == drive.label);
        if !described {
            let drive = Drive {
                key: format!("running-{index}"),
                icon: icon_for(entry.provider),
                title: entry.label.trim_start_matches("B2 ").to_string(),
                place: entry.target.clone(),
                provider: entry.provider,
                label: entry.label.clone(),
            };
            rows.push(drive_row(p, &drive, Some(entry)));
        }
    }
    if rows.is_empty() {
        rows.push(message(
            p,
            "empty",
            "No drives set up yet",
            Some(
                "Add a Backblaze B2 bucket, connect Google Drive or OneDrive, or set up a Seedbox",
            ),
        ));
    }
    let mut drives_section = section(p, "drives-section", "Drives", rows);
    for (index, error) in ui.mount.errors.iter().enumerate() {
        drives_section =
            drives_section.child(note(&format!("mount-error-{index}"), error, p.danger()));
    }
    if ui.credentials.unlock_failed {
        drives_section = drives_section.child(note(
            "unlock-failed",
            "Saved credentials couldn't be unlocked. Enter them again to mount.",
            p.danger(),
        ));
    }
    page = page.child(drives_section);
    scroll_body("drives-body", page)
}

fn icon_for(provider: CloudProvider) -> &'static Icon {
    match provider {
        CloudProvider::BackblazeB2 => &icons::BUCKET,
        CloudProvider::GoogleDrive => &icons::GOOGLE_DRIVE,
        CloudProvider::OneDrive => &icons::CLOUD,
        CloudProvider::Seedbox => &icons::SERVER,
    }
}
