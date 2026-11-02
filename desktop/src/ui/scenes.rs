//! Named fixture states for headless renders: `--list-scenes`, then
//! `--snap out.png --scene NAME [--theme light|dark]`. A render never reads
//! or writes the user's settings, keychain or mounts.

use rune::TextEdit;

use crate::models::{
    AppSettings, BucketMount, CloudProvider, GoogleDriveSettings, LogLine, MountEntry, MountState,
    SeedboxSettings,
};

use super::form::{Field, Form, MountSnapshot, Platform};
use super::{Credentials, Failure, Pane, Ui};

type Scene = (&'static str, fn(&mut Ui));

pub(super) const SCENES: &[Scene] = &[
    ("drives", |ui| configured(ui, Platform::Mac)),
    ("drives-empty", |ui| {
        ui.fuse_installed = Some(true);
    }),
    ("drives-no-fuse", |ui| {
        ui.fuse_installed = Some(false);
    }),
    ("drives-windows", |ui| configured(ui, Platform::Windows)),
    ("b2", |ui| {
        configured(ui, Platform::Mac);
        ui.pane = Pane::B2;
    }),
    ("b2-errors", |ui| {
        configured(ui, Platform::Mac);
        ui.pane = Pane::B2;
        ui.form.b2_key.replace_text("");
        ui.form.buckets[1].name.replace_text("photos/2024");
        ui.errors
            .insert(Field::Key, "Enter the application key".into());
        ui.errors.insert(
            Field::BucketName(1),
            "Use one bucket name, without slashes".into(),
        );
        ui.failure = Some(Failure {
            title: "Check the highlighted fields".into(),
            detail: "Fix the highlighted fields before mounting.".into(),
        });
    }),
    ("b2-windows", |ui| {
        configured(ui, Platform::Windows);
        ui.pane = Pane::B2;
    }),
    ("b2-windows-drive-menu", |ui| {
        configured(ui, Platform::Windows);
        ui.pane = Pane::B2;
        ui.open = Some(super::Menu::BucketDrive(0));
    }),
    ("google-drive", |ui| {
        configured(ui, Platform::Mac);
        ui.pane = Pane::GoogleDrive;
        ui.confirmation = Some((super::Task::TestGoogle, "Connection works".into()));
    }),
    ("google-drive-disconnected", |ui| {
        ui.pane = Pane::GoogleDrive;
    }),
    ("onedrive", |ui| {
        ui.pane = Pane::OneDrive;
        ui.tasks.insert(super::Task::ConnectOneDrive);
    }),
    ("seedbox", |ui| {
        configured(ui, Platform::Mac);
        ui.pane = Pane::Seedbox;
    }),
    ("general", |ui| {
        configured(ui, Platform::Mac);
        ui.pane = Pane::General;
    }),
    ("logs", |ui| {
        configured(ui, Platform::Mac);
        ui.pane = Pane::Logs;
        logs(ui);
    }),
    ("mount-menu", |ui| {
        configured(ui, Platform::Mac);
        ui.open = Some(super::Menu::MountActions);
    }),
    ("unmount-confirm", |ui| {
        configured(ui, Platform::Mac);
        ui.confirm_unmount = true;
    }),
    ("mounting", |ui| {
        configured(ui, Platform::Mac);
        ui.mount = MountState::default();
        ui.active = None;
        ui.operation = Some(super::Operation::Mounting);
    }),
    ("remount", |ui| {
        configured(ui, Platform::Mac);
        ui.form.buckets[1].name.replace_text("archive");
    }),
];

/// Two buckets, Google Drive and a Seedbox, all mounted but the Seedbox.
fn configured(ui: &mut Ui, platform: Platform) {
    let settings = AppSettings {
        selected_provider: CloudProvider::BackblazeB2,
        buckets: vec![
            BucketMount {
                bucket_name: "nocdn-main".into(),
                drive_letter: "Z".into(),
            },
            BucketMount {
                bucket_name: "photos".into(),
                drive_letter: "Y".into(),
            },
        ],
        google_drive: GoogleDriveSettings {
            remote_path: "Work".into(),
            ..GoogleDriveSettings::default()
        },
        seedbox: SeedboxSettings {
            host: "lyra.usbx.me".into(),
            username: "bartek".into(),
            ..SeedboxSettings::default()
        },
        start_at_login: true,
        ..AppSettings::default()
    };
    ui.platform = platform;
    ui.form = Form::from_settings(&settings, platform, &[]);
    ui.form.b2_key_id = TextEdit::new("0051f2a9c3e8d7b0000000003");
    ui.form.b2_key = TextEdit::new("K005Xq9mR2vLwP8sT1nYc4hZ").secure();
    ui.credentials = Credentials {
        loaded: true,
        unlock_failed: false,
        has_b2: true,
        google_drive: true,
        one_drive: false,
        seedbox: true,
        seedbox_password: true,
    };
    ui.fuse_installed = Some(true);
    let target = |folder: &str, letter: &str| match platform {
        Platform::Mac => format!("/Users/you/Drives/{folder}"),
        Platform::Windows => format!("{letter}:"),
    };
    let entry = |label: &str, folder: &str, letter: &str, provider| MountEntry {
        label: label.into(),
        target: target(folder, letter),
        provider,
        status: "mounted".into(),
        pid: Some(4242),
    };
    ui.mount = MountState {
        mounted: true,
        mounts: vec![
            entry(
                "B2 nocdn-main",
                "nocdn-main",
                "Z",
                CloudProvider::BackblazeB2,
            ),
            entry("B2 photos", "photos", "Y", CloudProvider::BackblazeB2),
            entry(
                "Google Drive",
                "google-drive",
                "G",
                CloudProvider::GoogleDrive,
            ),
        ],
        errors: Vec::new(),
    };
    ui.active = Some(MountSnapshot::of(&settings, platform));
}

fn logs(ui: &mut Ui) {
    let lines = [
        ("09:41:02", "INFO", "Cloud Drive Mount starting."),
        ("09:41:02", "INFO", "Settings loaded."),
        ("09:41:03", "INFO", "Credential state loaded."),
        (
            "09:41:03",
            "INFO",
            "Attempting auto-mount from saved settings.",
        ),
        (
            "09:41:03",
            "INFO",
            "Mounting B2 nocdn-main at /Users/you/Drives/nocdn-main",
        ),
        (
            "09:41:05",
            "INFO",
            "Mount startup completed for B2 nocdn-main",
        ),
        (
            "09:41:05",
            "INFO",
            "Mounting B2 photos at /Users/you/Drives/photos",
        ),
        ("09:41:06", "INFO", "Mount startup completed for B2 photos"),
        (
            "09:41:06",
            "INFO",
            "Mounting Google Drive at /Users/you/Drives/google-drive",
        ),
        (
            "09:41:09",
            "INFO",
            "Mount startup completed for Google Drive",
        ),
        (
            "09:41:09",
            "ERROR",
            "Seedbox: couldn't connect to lyra.usbx.me:21: connection timed out",
        ),
        ("09:41:09", "INFO", "Auto-mount completed."),
    ];
    for (timestamp, level, message) in lines {
        ui.push_line(LogLine {
            level: level.into(),
            message: message.into(),
            timestamp: timestamp.into(),
        });
    }
}
