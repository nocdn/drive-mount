//! The window's behaviour, driven headlessly through Rune's runtime.

use super::form::{Field, Platform};
use super::{spec, Menu, Operation, Pane, Ui};
use crate::host::HostEvent;
use crate::models::{CloudProvider, LogLine, MountEntry, MountState};

fn runtime(ui: Ui) -> rune::Runtime<Ui> {
    let spec = spec();
    let mut rt = rune::Runtime::new(ui, spec.view, (880.0, 640.0), rune::Env::new(true, 2.0));
    rt.frame(0.0);
    rt
}

fn scene(name: &str) -> Ui {
    let mut ui = Ui::fixture();
    let (_, apply) = super::scenes::SCENES
        .iter()
        .find(|(scene, _)| *scene == name)
        .unwrap_or_else(|| panic!("no scene named {name}"));
    apply(&mut ui);
    ui
}

fn click(rt: &mut rune::Runtime<Ui>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("nothing keyed {key} on screen"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn every_scene_draws() {
    for (name, apply) in super::scenes::SCENES {
        let mut ui = Ui::fixture();
        apply(&mut ui);
        let mut rt = runtime(ui);
        rt.frame(1.0);
        assert!(rt.find("pane-title").is_some(), "scene {name} has no title");
    }
}

#[test]
fn the_sidebar_switches_panes_and_remembers_the_provider() {
    let mut rt = runtime(Ui::fixture());
    click(&mut rt, "nav-seedbox");
    assert_eq!(rt.state.pane, Pane::Seedbox);
    assert_eq!(rt.state.form.selected_provider, CloudProvider::Seedbox);
    click(&mut rt, "nav-logs");
    assert_eq!(rt.state.pane, Pane::Logs);
    assert_eq!(rt.state.form.selected_provider, CloudProvider::Seedbox);
}

#[test]
fn mounted_and_unchanged_offers_unmount_not_mount() {
    let rt = runtime(scene("drives"));
    assert!(rt.find("unmount-all").is_some());
    assert!(rt.find("mount-all").is_none());
}

#[test]
fn a_changed_bucket_offers_a_remount() {
    let rt = runtime(scene("remount"));
    assert!(rt.state.has_mount_changes());
    assert!(rt.find("mount-all").is_some());
}

#[test]
fn nothing_mounted_offers_mount_all() {
    let rt = runtime(Ui::fixture());
    assert!(rt.find("mount-all").is_some());
    assert!(rt.find("unmount-all").is_none());
}

#[test]
fn mounting_without_a_key_highlights_the_b2_fields() {
    let mut ui = Ui::fixture();
    ui.form.buckets[0].name.replace_text("photos");
    let mut rt = runtime(ui);
    click(&mut rt, "mount-all");
    assert_eq!(rt.state.pane, Pane::B2);
    assert!(rt.state.errors.contains_key(&Field::KeyId));
    assert!(rt.state.failure.is_some());
    assert_eq!(rt.state.operation, None);
}

#[test]
fn add_and_remove_bucket_rows() {
    let mut ui = Ui::fixture();
    ui.pane = Pane::B2;
    let mut rt = runtime(ui);
    click(&mut rt, "add-bucket");
    assert_eq!(rt.state.form.buckets.len(), 2);
    click(&mut rt, "bucket-1/line/remove");
    assert_eq!(rt.state.form.buckets.len(), 1);
}

#[test]
fn the_overflow_menu_opens_and_closes() {
    let mut rt = runtime(scene("drives"));
    click(&mut rt, "mount-actions-trigger");
    assert_eq!(rt.state.open, Some(Menu::MountActions));
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: rune::Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.open, None);
}

#[test]
fn toggles_flip_their_settings() {
    let mut ui = Ui::fixture();
    ui.pane = Pane::General;
    let mut rt = runtime(ui);
    assert!(!rt.state.form.start_minimized);
    click(&mut rt, "start-minimized");
    assert!(rt.state.form.start_minimized);
}

#[test]
fn mount_state_events_track_the_active_settings() {
    let mut ui = Ui::fixture();
    ui.form.buckets[0].name.replace_text("photos");
    ui.on_host_event(HostEvent::MountState(MountState {
        mounted: true,
        mounts: vec![MountEntry {
            label: "B2 photos".into(),
            target: "/Users/you/Drives/photos".into(),
            provider: CloudProvider::BackblazeB2,
            status: "mounted".into(),
            pid: None,
        }],
        errors: Vec::new(),
    }));
    assert!(ui.mounted());
    assert!(!ui.has_mount_changes());
    ui.form.buckets[0].name.replace_text("archive");
    assert!(ui.has_mount_changes());
    ui.on_host_event(HostEvent::MountState(MountState::default()));
    assert!(!ui.has_mount_changes());
}

#[test]
fn the_log_keeps_its_newest_lines() {
    let mut ui = Ui::fixture();
    for index in 0..1100 {
        ui.on_host_event(HostEvent::Log(LogLine {
            level: "INFO".into(),
            message: format!("line {index}"),
            timestamp: "12:00:00".into(),
        }));
    }
    assert_eq!(ui.logs.len(), 1000);
    assert_eq!(ui.logs.front().unwrap().message, "line 100");
}

#[test]
fn windows_bucket_rows_pick_a_drive_letter_from_a_menu() {
    let mut rt = runtime(scene("b2-windows"));
    assert_eq!(rt.state.platform, Platform::Windows);
    click(&mut rt, "drive-0-field-trigger");
    assert_eq!(rt.state.open, Some(Menu::BucketDrive(0)));
}

#[test]
fn a_running_operation_disables_the_mount_actions() {
    let rt = runtime(scene("mounting"));
    assert_eq!(rt.state.operation, Some(Operation::Mounting));
    assert!(rt.find("progress").is_some());
}

#[test]
fn unmount_all_asks_first_and_cancel_keeps_the_drives() {
    let mut rt = runtime(scene("drives"));
    click(&mut rt, "unmount-all");
    assert!(rt.state.confirm_unmount);
    assert_eq!(rt.state.operation, None);
    assert!(rt.find("unmount-dialog").is_some());
    // Once its entrance has run.
    rt.frame(1.0);
    click(&mut rt, "unmount-dialog/body/buttons/cancel");
    assert!(!rt.state.confirm_unmount);
    assert_eq!(rt.state.operation, None);
    // Once its fade-out has run.
    rt.frame(1.0);
    assert!(rt.find("unmount-dialog").is_none());
}

#[test]
fn escape_dismisses_the_unmount_confirmation() {
    let mut rt = runtime(scene("unmount-confirm"));
    assert!(rt.find("unmount-dialog").is_some());
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: rune::Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.confirm_unmount);
    assert_eq!(rt.state.operation, None);
}

#[test]
fn confirming_unmounts_all() {
    let mut rt = runtime(scene("unmount-confirm"));
    click(&mut rt, "unmount-dialog/body/buttons/confirm");
    assert!(!rt.state.confirm_unmount);
    assert_eq!(rt.state.operation, Some(Operation::Unmounting));
}
