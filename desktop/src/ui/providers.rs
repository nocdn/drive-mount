//! The storage panes: Backblaze B2 (key and buckets), Google Drive and
//! OneDrive (sign in, folder), and Seedbox (SFTP server).

use rune::{col, edit_access, row, text, El, TextEdit, Tx};
use rune_kit::TextInput;

use super::controls::{
    described_row, inset_card, note, page, remove_button, secret_field, section, select,
    setting_row, small_button, text_field, toggle_row, ButtonKind, Choice, Paint, DESCRIPTION,
    FIELD_WIDTH, METRICS,
};
use super::drives::place;
use super::form::{Field, Platform, RESERVED_DRIVE_LETTERS};
use super::{failure_banner, scroll_body, Menu, Task, Ui};

const BUCKET_FIELD_WIDTH: f32 = 220.0;
const DRIVE_SELECT_WIDTH: f32 = 84.0;
const PORT_FIELD_WIDTH: f32 = 96.0;
/// The bucket card's padding on every side: its rows' own top and bottom.
const BUCKET_INSET: f32 = 8.0;

fn error(ui: &Ui, field: Field) -> Option<&str> {
    ui.errors.get(&field).map(String::as_str)
}

/// A row for a text field: its error, if any, as the description.
fn field_row(
    ui: &Ui,
    p: Paint,
    key: &str,
    title: &str,
    field: Option<Field>,
    hint: Option<&str>,
    control: El,
) -> El {
    let description = match field.and_then(|field| error(ui, field)) {
        Some(error) => Some((error.to_string(), p.danger())),
        None => hint.map(|hint| (hint.to_string(), p.muted())),
    };
    described_row(p, key, title, description, control)
}

fn keychain(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "the Keychain",
        Platform::Windows => "Windows Credential Manager",
    }
}

pub(super) fn b2(ui: &Ui, p: Paint, layer: &mut Vec<El>) -> El {
    let f = &ui.form;
    let key_status = if ui.credentials.unlock_failed {
        (
            "Saved credentials couldn't be unlocked. Enter them again to mount.".to_string(),
            p.danger(),
        )
    } else if !ui.credentials.loaded && ui.backend.is_some() {
        ("Unlocking saved credentials…".to_string(), p.muted())
    } else if ui.credentials.has_b2 {
        (
            format!("Saved securely in {}", keychain(ui.platform)),
            p.muted(),
        )
    } else {
        (
            format!("Saved securely in {} when you mount", keychain(ui.platform)),
            p.muted(),
        )
    };
    let key = section(
        p,
        "key",
        "Application key",
        vec![
            field_row(
                ui,
                p,
                "key-id",
                "Key ID",
                Some(Field::KeyId),
                None,
                text_field(
                    p,
                    "key-id-field",
                    &f.b2_key_id,
                    |ui| &mut ui.form.b2_key_id,
                    "keyID",
                    FIELD_WIDTH,
                    error(ui, Field::KeyId).is_some(),
                ),
            ),
            field_row(
                ui,
                p,
                "key-secret",
                "Application key",
                Some(Field::Key),
                None,
                secret_field(
                    p,
                    "key-field",
                    &f.b2_key,
                    |ui| &mut ui.form.b2_key,
                    "applicationKey",
                    error(ui, Field::Key).is_some(),
                ),
            ),
        ],
    )
    .child(note("key-status", &key_status.0, key_status.1));

    let rows = f
        .buckets
        .iter()
        .enumerate()
        .map(|(index, bucket)| bucket_row(ui, p, index, &bucket.name, &bucket.drive, layer))
        .collect();
    let add = small_button(
        p,
        "add-bucket",
        "Add bucket",
        ButtonKind::Secondary,
        Some(Box::new(|ui: &mut Ui, tx: &mut Tx| {
            ui.form
                .add_bucket(ui.platform, &ui.unavailable_letters.clone());
            ui.edited(tx);
        })),
    );
    // Add sits under the last bucket, inside the card, where the next row
    // will appear.
    let add_row = row()
        .key("add-row")
        .px(BUCKET_INSET)
        .pt(4.0)
        // As far below as the first bucket's field is from the card's top.
        .pb(BUCKET_INSET)
        .child(add);
    let buckets = col()
        .key("buckets")
        .gap(METRICS.label_gap)
        .child(p.section_label("Buckets").mx(METRICS.label_inset))
        .child(inset_card(p, "card", rows, BUCKET_INSET).child(add_row))
        .child(note(
            "buckets-help",
            match ui.platform {
                Platform::Mac => {
                    "Each bucket mounts as a folder in ~/Drives, named after the bucket"
                }
                Platform::Windows => "Each bucket mounts at its own drive letter",
            },
            p.muted(),
        ));

    scroll_body(
        "b2-body",
        page("b2")
            .children(failure_banner(ui, p))
            .child(key)
            .child(buckets),
    )
}

fn bucket_row(
    ui: &Ui,
    p: Paint,
    index: usize,
    name: &TextEdit,
    drive: &str,
    layer: &mut Vec<El>,
) -> El {
    let name_error = error(ui, Field::BucketName(index));
    let drive_error = error(ui, Field::BucketDrive(index));
    let field = TextInput::new(name)
        .placeholder("Bucket name")
        .width(Some(BUCKET_FIELD_WIDTH))
        .invalid(name_error.is_some())
        .on_edit(edit_access(move |ui: &mut Ui| ui.bucket_name(index)))
        .on_change(|ui: &mut Ui, tx: &mut Tx| ui.edited(tx))
        .build(p.k)
        .key("name");
    let trimmed = name.text.trim();
    let destination = if trimmed.is_empty() {
        String::new()
    } else {
        place(ui.platform, trimmed, "")
    };
    let mut line = row().key("line").gap(12.0).center_y().child(field).child(
        text(if ui.platform == Platform::Mac {
            destination
        } else {
            String::new()
        })
        .key("destination")
        .font(DESCRIPTION)
        .color(p.faint())
        .grow(1.0)
        .shrink()
        .truncate(),
    );
    if ui.platform == Platform::Windows {
        line = line.child(drive_select(ui, p, index, drive, layer));
    }
    line = line.child(remove_button(p, "remove", move |ui: &mut Ui| {
        let unavailable = ui.unavailable_letters.clone();
        ui.form.remove_bucket(index, ui.platform, &unavailable);
        ui.errors.clear();
        ui.flush_edits();
        ui.save_preferences();
    }));
    let message = name_error.or(drive_error);
    col()
        .key(format!("bucket-{index}"))
        .px(BUCKET_INSET)
        .py(8.0)
        .gap(4.0)
        .child(line)
        .children(message.map(|message| {
            text(message.to_string())
                .key("error")
                .font(DESCRIPTION)
                .color(p.danger())
        }))
}

/// A bucket's drive letter, from the letters no other drive uses.
fn drive_select(ui: &Ui, p: Paint, index: usize, current: &str, layer: &mut Vec<El>) -> El {
    let taken: Vec<String> = ui
        .form
        .buckets
        .iter()
        .enumerate()
        .filter(|(other, _)| *other != index)
        .map(|(_, bucket)| bucket.drive.clone())
        .collect();
    let choices = ('D'..='Z')
        .filter(|letter| {
            !RESERVED_DRIVE_LETTERS.contains(letter)
                && (!ui.unavailable_letters.contains(letter) || current == letter.to_string())
        })
        .map(|letter| {
            let value = letter.to_string();
            Choice {
                label: format!("{letter}:"),
                selected: current == value,
                disabled: taken.contains(&value),
                choose: Box::new(move |ui: &mut Ui| {
                    if let Some(bucket) = ui.form.buckets.get_mut(index) {
                        bucket.drive = value.clone();
                    }
                    ui.save_preferences();
                }),
            }
        })
        .collect();
    let menu = Menu::BucketDrive(index);
    select(
        p,
        &format!("drive-{index}"),
        &if current.is_empty() {
            "–".to_string()
        } else {
            format!("{current}:")
        },
        DRIVE_SELECT_WIDTH,
        ui.open == Some(menu),
        choices,
        move |ui: &mut Ui| ui.open = (ui.open != Some(menu)).then_some(menu),
        layer,
    )
}

/// The Google Drive and OneDrive panes differ in names and a few words.
struct Cloud {
    key: &'static str,
    name: &'static str,
    folder: &'static str,
    letter: &'static str,
    connected: bool,
    connect: Task,
    test: Task,
}

fn account_section(ui: &Ui, p: Paint, cloud: &Cloud, toggle: fn(&mut Ui), test: fn(&mut Ui)) -> El {
    let connecting = ui.busy(cloud.connect);
    let testing = ui.busy(cloud.test);
    let status = match &ui.confirmation {
        Some((task, message)) if *task == cloud.test => (message.clone(), p.success()),
        _ if connecting && !cloud.connected => {
            ("Finish signing in in your browser".to_string(), p.muted())
        }
        _ if cloud.connected => (
            format!(
                "Connected · mounts at {}",
                place(ui.platform, cloud.folder, cloud.letter)
            ),
            p.muted(),
        ),
        _ => ("Not connected".to_string(), p.muted()),
    };
    let mut controls = row().key("controls").gap(8.0).center_y();
    if cloud.connected {
        controls = controls.child(small_button(
            p,
            "test",
            if testing {
                "Testing…"
            } else {
                "Test connection"
            },
            ButtonKind::Secondary,
            (!testing && !connecting)
                .then(|| Box::new(move |ui: &mut Ui, _: &mut Tx| test(ui)) as _),
        ));
    }
    let toggle_label = match (cloud.connected, connecting) {
        (true, true) => "Disconnecting…",
        (true, false) => "Disconnect",
        (false, true) => "Connecting…",
        (false, false) => "Connect…",
    };
    controls = controls.child(small_button(
        p,
        "connect",
        toggle_label,
        ButtonKind::Secondary,
        (!testing && !connecting).then(|| Box::new(move |ui: &mut Ui, _: &mut Tx| toggle(ui)) as _),
    ));
    section(
        p,
        "account",
        "Account",
        vec![described_row(
            p,
            &format!("{}-account", cloud.key),
            cloud.name,
            Some(status),
            controls,
        )],
    )
}

pub(super) fn google_drive(ui: &Ui, p: Paint) -> El {
    let cloud = Cloud {
        key: "google-drive",
        name: "Google Drive",
        folder: "google-drive",
        letter: "G",
        connected: ui.credentials.google_drive,
        connect: Task::ConnectGoogle,
        test: Task::TestGoogle,
    };
    let account = account_section(
        ui,
        p,
        &cloud,
        Ui::toggle_google_drive,
        Ui::test_google_drive,
    )
    .child(note(
        "help",
        "Connect opens your browser to sign in with Google",
        p.muted(),
    ));
    let folder = section(
        p,
        "folder",
        "Folder",
        vec![
            setting_row(
                p,
                "path",
                "Folder path",
                Some("Mount one folder instead of the whole drive"),
                text_field(
                    p,
                    "path-field",
                    &ui.form.gdrive_path,
                    |ui| &mut ui.form.gdrive_path,
                    "Whole drive",
                    FIELD_WIDTH,
                    false,
                ),
            ),
            setting_row(
                p,
                "root",
                "Root folder ID",
                Some("For a shared folder: the ID at the end of its link"),
                text_field(
                    p,
                    "root-field",
                    &ui.form.gdrive_root,
                    |ui| &mut ui.form.gdrive_root,
                    "Optional",
                    FIELD_WIDTH,
                    false,
                ),
            ),
        ],
    );
    let client = section(
        p,
        "client",
        "Google client",
        vec![
            setting_row(
                p,
                "client-id",
                "Client ID",
                Some("From a Desktop app OAuth client in Google Cloud"),
                text_field(
                    p,
                    "client-id-field",
                    &ui.form.gdrive_client_id,
                    |ui| &mut ui.form.gdrive_client_id,
                    "rclone's shared client",
                    FIELD_WIDTH,
                    false,
                ),
            ),
            setting_row(
                p,
                "client-secret",
                "Client secret",
                Some("Or put the downloaded JSON in the app folder as google-oauth-client.json"),
                secret_field(
                    p,
                    "client-secret-field",
                    &ui.form.gdrive_client_secret,
                    |ui| &mut ui.form.gdrive_client_secret,
                    "Optional",
                    false,
                ),
            ),
        ],
    )
    .child(note(
        "client-help",
        "Your own client keeps Google's rate limits to your requests. Disconnect and connect again after changing it",
        p.muted(),
    ));
    scroll_body(
        "google-drive-body",
        page("google-drive")
            .children(failure_banner(ui, p))
            .child(account)
            .child(folder)
            .child(client),
    )
}

pub(super) fn one_drive(ui: &Ui, p: Paint) -> El {
    let cloud = Cloud {
        key: "onedrive",
        name: "OneDrive",
        folder: "onedrive",
        letter: "O",
        connected: ui.credentials.one_drive,
        connect: Task::ConnectOneDrive,
        test: Task::TestOneDrive,
    };
    let account =
        account_section(ui, p, &cloud, Ui::toggle_one_drive, Ui::test_one_drive).child(note(
            "help",
            "Connect opens your browser to sign in with Microsoft",
            p.muted(),
        ));
    let folder = section(
        p,
        "folder",
        "Folder",
        vec![setting_row(
            p,
            "path",
            "Folder path",
            Some("Mount one folder instead of the whole drive"),
            text_field(
                p,
                "path-field",
                &ui.form.onedrive_path,
                |ui| &mut ui.form.onedrive_path,
                "Whole drive",
                FIELD_WIDTH,
                false,
            ),
        )],
    )
    .child(note(
        "cache-help",
        "Because rclone caches folder listings, changes made in OneDrive on the web or another device can take 1 to 5 minutes to appear here in Finder or Explorer",
        p.muted(),
    ));
    scroll_body(
        "onedrive-body",
        page("onedrive")
            .children(failure_banner(ui, p))
            .child(account)
            .child(folder),
    )
}

pub(super) fn seedbox(ui: &Ui, p: Paint) -> El {
    let f = &ui.form;
    let saved_password = ui.credentials.seedbox_password;
    let server = section(
        p,
        "server",
        "Server",
        vec![
            field_row(
                ui,
                p,
                "host",
                "Host",
                Some(Field::SeedboxHost),
                Some("Usually your server's name"),
                text_field(
                    p,
                    "host-field",
                    &f.seedbox_host,
                    |ui| &mut ui.form.seedbox_host,
                    "server.example.com",
                    FIELD_WIDTH,
                    error(ui, Field::SeedboxHost).is_some(),
                ),
            ),
            field_row(
                ui,
                p,
                "port",
                "Port",
                Some(Field::SeedboxPort),
                None,
                text_field(
                    p,
                    "port-field",
                    &f.seedbox_port,
                    |ui| &mut ui.form.seedbox_port,
                    "21",
                    PORT_FIELD_WIDTH,
                    error(ui, Field::SeedboxPort).is_some(),
                ),
            ),
            field_row(
                ui,
                p,
                "username",
                "Username",
                Some(Field::SeedboxUser),
                None,
                text_field(
                    p,
                    "username-field",
                    &f.seedbox_user,
                    |ui| &mut ui.form.seedbox_user,
                    "",
                    FIELD_WIDTH,
                    error(ui, Field::SeedboxUser).is_some(),
                ),
            ),
            field_row(
                ui,
                p,
                "password",
                "Password",
                Some(Field::SeedboxPassword),
                saved_password.then_some("Saved; type to replace it"),
                secret_field(
                    p,
                    "password-field",
                    &f.seedbox_password,
                    |ui| &mut ui.form.seedbox_password,
                    if saved_password {
                        "••••••••"
                    } else {
                        ""
                    },
                    error(ui, Field::SeedboxPassword).is_some(),
                ),
            ),
            setting_row(
                p,
                "remote-folder",
                "Remote folder",
                Some("Usually downloads"),
                text_field(
                    p,
                    "remote-folder-field",
                    &f.seedbox_path,
                    |ui| &mut ui.form.seedbox_path,
                    "downloads",
                    FIELD_WIDTH,
                    false,
                ),
            ),
        ],
    );
    let options = section(
        p,
        "options",
        "Options",
        vec![toggle_row(
            p,
            "read-only",
            "Mount read-only",
            Some("Files can be opened and copied, not changed"),
            f.seedbox_read_only,
            |ui: &mut Ui| {
                ui.form.seedbox_read_only = !ui.form.seedbox_read_only;
                ui.save_preferences();
            },
        )],
    );

    let testing = ui.busy(Task::TestSeedbox);
    let forgetting = ui.busy(Task::ForgetSeedbox);
    let configured = ui.credentials.seedbox || saved_password;
    let status = match &ui.confirmation {
        Some((Task::TestSeedbox, message)) => (message.clone(), p.success()),
        _ if configured => (
            format!("Set up · mounts at {}", place(ui.platform, "seedbox", "S")),
            p.muted(),
        ),
        _ => (
            "Test the connection to save the password".to_string(),
            p.muted(),
        ),
    };
    let mut controls = row().key("controls").gap(8.0).center_y();
    if configured {
        controls = controls.child(small_button(
            p,
            "forget",
            if forgetting {
                "Forgetting…"
            } else {
                "Forget"
            },
            ButtonKind::Secondary,
            (!testing && !forgetting)
                .then(|| Box::new(|ui: &mut Ui, _: &mut Tx| ui.forget_seedbox()) as _),
        ));
    }
    controls = controls.child(small_button(
        p,
        "test",
        if testing {
            "Testing…"
        } else {
            "Test connection"
        },
        ButtonKind::Secondary,
        (!testing && !forgetting)
            .then(|| Box::new(|ui: &mut Ui, _: &mut Tx| ui.test_seedbox()) as _),
    ));
    let connection = section(
        p,
        "connection",
        "Connection",
        vec![described_row(
            p,
            "seedbox-connection",
            "Seedbox",
            Some(status),
            controls,
        )],
    )
    .child(note(
        "help",
        "Use your Ultra.cc SSH login. SFTP runs on port 22",
        p.muted(),
    ));

    scroll_body(
        "seedbox-body",
        page("seedbox")
            .children(failure_banner(ui, p))
            .child(connection)
            .child(server)
            .child(options),
    )
}
