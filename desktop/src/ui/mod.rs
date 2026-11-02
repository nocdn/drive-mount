//! Cloud Drive Mount's window, drawn with Rune in OpenCode's design
//! language, following Hex's layout: a deep-background sidebar of panes and
//! a raised, rounded content panel with the pane's title and actions on top.
//! Views use `rune-kit` components and the kit's tokens through `Paint`
//! (`controls.rs`); no colour literals.

mod controls;
mod drives;
mod form;
mod general;
mod icons;
mod logs;
mod providers;
mod scenes;
mod tasks;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use rune::{col, el, icon, row, text, Anim, AppSpec, Cx, El, Icon, ScrollSpec, TextEdit, Tx};

use crate::backend::Backend;
use crate::host::HostEvent;
use crate::models::{AppSettings, CloudProvider, LoadedCredentials, LogLine, MountState};
use controls::{source_item, source_list, Paint, METRICS};
pub use form::Platform;
use form::{Errors, Form, MountSnapshot};

pub const APP_NAME: &str = "Cloud Drive Mount";
pub const WINDOW_SIZE: (f32, f32) = (880.0, 640.0);
/// How long typing pauses before the settings are saved.
const SAVE_DELAY: Duration = Duration::from_millis(600);
/// How long "Copied" replaces "Copy".
const COPIED_FOR: Duration = Duration::from_millis(1500);
/// The log view keeps this many lines.
const MAX_LOG_LINES: usize = 1000;

/// Every pane, in sidebar order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pane {
    /// What is mounted, and the mount actions.
    #[default]
    Drives,
    B2,
    GoogleDrive,
    OneDrive,
    Seedbox,
    General,
    Logs,
}

impl Pane {
    const ALL: [Pane; 7] = [
        Pane::Drives,
        Pane::B2,
        Pane::GoogleDrive,
        Pane::OneDrive,
        Pane::Seedbox,
        Pane::General,
        Pane::Logs,
    ];

    fn title(self) -> &'static str {
        match self {
            Pane::Drives => "Drives",
            Pane::B2 => "Backblaze B2",
            Pane::GoogleDrive => "Google Drive",
            Pane::OneDrive => "OneDrive",
            Pane::Seedbox => "Seedbox",
            Pane::General => "General",
            Pane::Logs => "Logs",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Pane::Drives => "nav-drives",
            Pane::B2 => "nav-b2",
            Pane::GoogleDrive => "nav-google-drive",
            Pane::OneDrive => "nav-onedrive",
            Pane::Seedbox => "nav-seedbox",
            Pane::General => "nav-general",
            Pane::Logs => "nav-logs",
        }
    }

    fn icon(self) -> &'static Icon {
        match self {
            Pane::Drives => &icons::DRIVES,
            Pane::B2 => &icons::BUCKET,
            Pane::GoogleDrive => &icons::GOOGLE_DRIVE,
            Pane::OneDrive => &icons::CLOUD,
            Pane::Seedbox => &icons::SERVER,
            Pane::General => &icons::SETTINGS,
            Pane::Logs => &icons::LOGS,
        }
    }

    /// The provider a pane configures.
    fn provider(self) -> Option<CloudProvider> {
        match self {
            Pane::B2 => Some(CloudProvider::BackblazeB2),
            Pane::GoogleDrive => Some(CloudProvider::GoogleDrive),
            Pane::OneDrive => Some(CloudProvider::OneDrive),
            Pane::Seedbox => Some(CloudProvider::Seedbox),
            _ => None,
        }
    }

    /// Panes after a gap in the sidebar start a new group.
    fn starts_group(self) -> bool {
        matches!(self, Pane::B2 | Pane::General)
    }
}

/// The one anchored menu that is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
    MountActions,
    /// A bucket's drive letter (Windows).
    BucketDrive(usize),
}

/// A mount-wide action under way; only one runs at a time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operation {
    Mounting,
    Unmounting,
    Refreshing,
    Restarting,
}

impl Operation {
    fn progress(self) -> &'static str {
        match self {
            Operation::Mounting => "Mounting…",
            Operation::Unmounting => "Unmounting…",
            Operation::Refreshing => "Refreshing…",
            Operation::Restarting => "Restarting mounts…",
        }
    }
}

/// A provider action under way.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Task {
    ConnectGoogle,
    TestGoogle,
    ConnectOneDrive,
    TestOneDrive,
    TestSeedbox,
    ForgetSeedbox,
}

/// What the window knows of the saved secrets and provider configs.
#[derive(Clone, Copy, Default, Debug)]
pub struct Credentials {
    pub loaded: bool,
    pub unlock_failed: bool,
    pub has_b2: bool,
    pub google_drive: bool,
    pub one_drive: bool,
    pub seedbox: bool,
    pub seedbox_password: bool,
}

/// The last action that failed, shown at the top of the pane until
/// dismissed or another action starts.
#[derive(Clone, Debug)]
pub struct Failure {
    pub title: String,
    pub detail: String,
}

pub struct Ui {
    pub(crate) pane: Pane,
    pub(crate) open: Option<Menu>,
    pub(crate) platform: Platform,
    pub(crate) form: Form,
    pub(crate) errors: Errors,
    pub(crate) credentials: Credentials,
    pub(crate) mount: MountState,
    /// What the running mounts were started with.
    pub(crate) active: Option<MountSnapshot>,
    pub(crate) operation: Option<Operation>,
    pub(crate) tasks: BTreeSet<Task>,
    pub(crate) logs: VecDeque<LogLine>,
    pub(crate) copied_until: Option<Instant>,
    pub(crate) fuse_installed: Option<bool>,
    pub(crate) unavailable_letters: Vec<char>,
    edited_at: Option<Instant>,
    pub(crate) failure: Option<Failure>,
    /// A task's success, shown in its row.
    pub(crate) confirmation: Option<(Task, String)>,
    /// Unmount all is waiting for the user to confirm it.
    pub(crate) confirm_unmount: bool,
    backend: Option<Backend>,
    window: Option<rune::AppHandle<Ui>>,
    /// Where an edit for a row that went away lands.
    scratch: TextEdit,
}

impl Ui {
    /// A window with no backend: scenes, snapshots and tests.
    pub fn fixture() -> Self {
        Self::with(&AppSettings::default(), Platform::current(), None)
    }

    fn with(settings: &AppSettings, platform: Platform, backend: Option<Backend>) -> Self {
        Ui {
            pane: Pane::Drives,
            open: None,
            platform,
            form: Form::from_settings(settings, platform, &[]),
            errors: Errors::new(),
            credentials: Credentials::default(),
            mount: MountState::default(),
            active: None,
            operation: None,
            tasks: BTreeSet::new(),
            logs: VecDeque::new(),
            copied_until: None,
            fuse_installed: None,
            unavailable_letters: Vec::new(),
            edited_at: None,
            failure: None,
            confirmation: None,
            confirm_unmount: false,
            backend,
            window: None,
            scratch: TextEdit::default(),
        }
    }

    /// The live window: loads the saved settings and starts the launch
    /// sequence (credentials, then the auto-mount) off the UI thread.
    pub fn live(backend: Backend) -> Self {
        let platform = Platform::current();
        let unavailable = if platform == Platform::Windows {
            crate::rclone::used_windows_drive_letters()
                .iter()
                .filter_map(|letter| letter.chars().next())
                .collect()
        } else {
            Vec::new()
        };
        let settings = backend.load_settings();
        let mut ui = Self::with(&settings, platform, Some(backend));
        ui.unavailable_letters = unavailable;
        ui.form = Form::from_settings(&settings, platform, &ui.unavailable_letters);
        ui
    }

    pub(crate) fn mounted(&self) -> bool {
        self.mount.mounted
    }

    /// Whether the fields differ from what the running mounts use.
    pub(crate) fn has_mount_changes(&self) -> bool {
        self.active.as_ref().is_some_and(|active| {
            *active != MountSnapshot::of(&self.form.settings(self.platform), self.platform)
        })
    }

    pub(crate) fn busy(&self, task: Task) -> bool {
        self.tasks.contains(&task)
    }

    /// Called by every text field on change: the settings save once typing
    /// pauses.
    pub(crate) fn edited(&mut self, tx: &mut Tx) {
        self.edited_at = Some(Instant::now());
        tx.wake_after(SAVE_DELAY);
    }

    pub(crate) fn select_pane(&mut self, pane: Pane, tx: &mut Tx) {
        if self.pane == pane {
            return;
        }
        tx.instant();
        self.flush_edits();
        self.open = None;
        self.pane = pane;
        if let Some(provider) = pane.provider() {
            self.form.selected_provider = provider;
        }
    }

    /// Saves pending edits now (leaving a pane, closing the window).
    pub(crate) fn flush_edits(&mut self) {
        if self.edited_at.take().is_some() {
            self.save_preferences();
        }
    }

    /// An error from the window itself: logged, and shown on the pane.
    pub(crate) fn fail(&mut self, title: &str, detail: impl Into<String>) {
        let detail = detail.into();
        self.log_error(&detail);
        self.failure = Some(Failure {
            title: title.to_string(),
            detail,
        });
    }

    pub(crate) fn log_error(&mut self, message: &str) {
        match &self.backend {
            Some(backend) => backend.log_error(message),
            None => self.push_log("ERROR", message),
        }
    }

    fn push_log(&mut self, level: &str, message: &str) {
        self.push_line(LogLine {
            level: level.to_string(),
            message: message.to_string(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        });
    }

    pub(crate) fn push_line(&mut self, line: LogLine) {
        self.logs.push_back(line);
        while self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_front();
        }
    }

    /// Takes in what the backend reports.
    pub fn on_host_event(&mut self, event: HostEvent) {
        match event {
            HostEvent::Log(line) => self.push_line(line),
            HostEvent::MountState(state) => self.set_mount_state(state),
        }
    }

    pub(crate) fn set_mount_state(&mut self, state: MountState) {
        if state.mounted {
            if self.active.is_none() {
                self.active = Some(MountSnapshot::of(
                    &self.form.settings(self.platform),
                    self.platform,
                ));
            }
        } else {
            self.active = None;
        }
        self.mount = state;
    }

    pub(crate) fn set_credentials(&mut self, loaded: &LoadedCredentials) {
        if let Some(b2) = &loaded.b2_credentials {
            self.form
                .b2_key_id
                .replace_text(b2.application_key_id.clone());
            self.form.b2_key.replace_text(b2.application_key.clone());
        }
        self.credentials = Credentials {
            loaded: true,
            unlock_failed: false,
            has_b2: loaded.has_saved_credentials,
            google_drive: loaded.is_google_drive_configured,
            one_drive: loaded.is_one_drive_configured,
            seedbox: loaded.is_seedbox_configured,
            seedbox_password: loaded.has_saved_seedbox_password,
        };
    }

    /// The `TextEdit` of bucket row `index`, or a scratch one when that row
    /// is gone.
    pub(crate) fn bucket_name(&mut self, index: usize) -> &mut TextEdit {
        match self.form.buckets.get_mut(index) {
            Some(bucket) => &mut bucket.name,
            None => &mut self.scratch,
        }
    }

    pub(crate) fn copied(&self) -> bool {
        self.copied_until
            .is_some_and(|until| Instant::now() < until)
    }

    pub(crate) fn copy_logs(&mut self, tx: &mut Tx) {
        let text = logs::plain_text(&self.logs);
        if text.is_empty() {
            return;
        }
        tx.copy(text);
        self.copied_until = Some(Instant::now() + COPIED_FOR);
        tx.wake_after(COPIED_FOR);
    }
}

fn poll(ui: &mut Ui, _clock: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    if let Some(at) = ui.edited_at {
        let waited = at.elapsed();
        if waited >= SAVE_DELAY {
            ui.edited_at = None;
            ui.save_preferences();
            changed = true;
        } else {
            tx.wake_after(SAVE_DELAY - waited);
        }
    }
    if let Some(until) = ui.copied_until {
        if Instant::now() >= until {
            ui.copied_until = None;
            changed = true;
        }
    }
    changed
}

fn on_close(ui: &mut Ui, _: &mut Tx) {
    ui.flush_edits();
    ui.open = None;
}

pub fn spec() -> AppSpec<Ui> {
    AppSpec {
        title: APP_NAME,
        size: WINDOW_SIZE,
        min_size: Some((760.0, 520.0)),
        init: Ui::fixture,
        view,
        scenes: scenes::SCENES,
        fonts: rune_theme_opencode::FONTS,
        families: rune_theme_opencode::FAMILIES,
        setup: None,
        poll: Some(poll),
        on_key: None,
    }
}

/// The app for this process: the live window, or a fixture for headless
/// renders (`--snap`, `--tree`, `--list-scenes`).
pub fn app(backend: Option<Backend>, show_on_launch: bool) -> rune::App<Ui> {
    let app = rune::App::new(spec())
        .chrome(rune::Chrome {
            traffic_lights: Some(TRAFFIC_LIGHTS),
            ..rune::Chrome::DEFAULT
        })
        .on_close(on_close);
    let Some(backend) = backend else {
        return app;
    };
    let app = app
        .init({
            let backend = backend.clone();
            move || Ui::live(backend)
        })
        .args(Vec::new())
        .accessory()
        .on_reopen(on_reopen);
    // An accessory app shows no menu bar, but its key equivalents work:
    // Edit gives text fields ⌘C, ⌘V and the rest, and ⌘Q quits.
    #[cfg(target_os = "macos")]
    let app = app.standard_menus().menu_item(
        rune::MenuItem::new(format!("Quit {APP_NAME}"), |_: &mut Ui, tx: &mut Tx| {
            tx.quit()
        })
        .id("quit")
        .key_equivalent("cmd+q".parse().expect("a valid shortcut")),
    );
    let app = if show_on_launch {
        app.keep_running()
    } else {
        app.windowless()
    };
    let window = app.handle();
    backend.host.set_listener({
        let window = window.clone();
        move |event| window.update(move |ui, _| ui.on_host_event(event))
    });
    window.update_if({
        let window = window.clone();
        move |ui, _| {
            ui.window = Some(window);
            ui.launch();
            false
        }
    });
    app
}

/// Opening the app again (Finder, Spotlight) shows the window.
fn on_reopen(ui: &mut Ui, has_visible_windows: bool, _: &mut Tx) -> bool {
    if !has_visible_windows {
        if let Some(window) = &ui.window {
            bring_forward(window);
        }
    }
    true
}

/// Shows the window in front of whatever app is active.
pub fn bring_forward(window: &rune::AppHandle<Ui>) {
    window.open();
    window.focus();
    #[cfg(target_os = "macos")]
    window.update_if(|_, _| {
        if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
            objc2_app_kit::NSApplication::sharedApplication(mtm).activate();
        }
        false
    });
}

fn view(ui: &Ui, cx: &mut Cx<Ui>) -> El {
    let p = Paint::new(cx.system_dark(), cx.env().scale);
    // Menus and popovers mount above the page (rune-kit overlays).
    let mut layer = Vec::new();
    let (actions, body) = match ui.pane {
        Pane::Drives => (drives::actions(ui, p, &mut layer), drives::pane(ui, p)),
        Pane::B2 => (
            drives::actions(ui, p, &mut layer),
            providers::b2(ui, p, &mut layer),
        ),
        Pane::GoogleDrive => (
            drives::actions(ui, p, &mut layer),
            providers::google_drive(ui, p),
        ),
        Pane::OneDrive => (
            drives::actions(ui, p, &mut layer),
            providers::one_drive(ui, p),
        ),
        Pane::Seedbox => (
            drives::actions(ui, p, &mut layer),
            providers::seedbox(ui, p),
        ),
        Pane::General => (Vec::new(), general::pane(ui, p)),
        Pane::Logs => (logs::actions(ui, p), logs::pane(ui, p)),
    };
    let panel = col()
        .key("panel")
        .grow(1.0)
        .shrink()
        .mt(8.0)
        .mr(8.0)
        .mb(8.0)
        .radius(p.k.radius.lg)
        .fill(p.k.c.bg_base)
        .border(p.hair, p.k.c.border_weak)
        // Children sit inside the border, so their fills never paint over it.
        .padding(p.hair)
        .clip_soft(0.0)
        .child(pane_header(p, ui.pane.title(), actions))
        .child(body);
    let window = row()
        .key("window")
        .role("window")
        .fill(p.k.c.bg_deep)
        .hover_motion_within(Anim::INSTANT, Anim::INSTANT)
        .child(sidebar(ui, p))
        .child(panel);
    rune_kit::root(window, layer)
}

/// Where macOS draws the close button's top-left, in points from the
/// window's: a little further in than AppKit's own spot, around (9, 9).
const TRAFFIC_LIGHTS: (f32, f32) = (16.0, 18.0);

/// macOS draws the traffic lights over the sidebar (Rune's window has a
/// transparent, full-size title bar), so the navigation starts below them,
/// as far under them as it was under AppKit's own spot.
const SIDEBAR_TOP: f32 = if cfg!(target_os = "macos") {
    40.0 + (TRAFFIC_LIGHTS.1 - 9.0)
} else {
    12.0
};
const NAV_ROW_HEIGHT: f32 = 28.0;
const NAV_ROW_GAP: f32 = 4.0;
const NAV_GROUP_GAP: f32 = 14.0;

/// On macOS the sidebar and the pane header stand in for the title bar:
/// pressing their background moves the window.
fn title_bar(element: El) -> El {
    if cfg!(target_os = "macos") {
        element.window_drag()
    } else {
        element
    }
}

fn sidebar(ui: &Ui, p: Paint) -> El {
    let mut nav = source_list(col().key("nav"));
    for pane in Pane::ALL {
        let above = if pane.starts_group() {
            NAV_GROUP_GAP
        } else {
            NAV_ROW_GAP / 2.0
        };
        nav = nav.child(nav_slot(ui, p, pane, above, NAV_ROW_GAP / 2.0));
    }
    title_bar(col())
        .key("sidebar")
        .role("navigation")
        .w(208.0)
        .h_full()
        .pt(SIDEBAR_TOP - NAV_ROW_GAP / 2.0)
        .pb(12.0)
        .pl(10.0)
        .pr(6.0)
        .child(nav)
        .child(el().key("spacer").grow(1.0))
        .child(footer(ui, p))
}

/// A sidebar item inside its slot. Slots tile the item column, each reaching
/// into the gaps around its item, so the nearest item takes the hover and a
/// press there selects it.
fn nav_slot(ui: &Ui, p: Paint, pane: Pane, above: f32, below: f32) -> El {
    col()
        .key(format!("{}-slot", pane.key()))
        .pt(above)
        .pb(below)
        .hover_group()
        .on_drag(move |ui: &mut Ui, d: &mut rune::Drag, tx: &mut Tx| {
            select_on_down(ui, d, tx, pane)
        })
        .on_press_tx(move |ui: &mut Ui, tx: &mut Tx| ui.select_pane(pane, tx))
        .child(nav_item(ui, p, pane, ui.pane == pane))
}

/// Switch on pointer down, like a native sidebar; the press keeps keyboard
/// activation.
fn select_on_down(ui: &mut Ui, d: &mut rune::Drag, tx: &mut Tx, pane: Pane) {
    if d.phase == rune::Phase::Down {
        ui.select_pane(pane, tx);
    }
}

fn nav_item(ui: &Ui, p: Paint, pane: Pane, on: bool) -> El {
    let c = &p.k.c;
    let mounted = pane
        .provider()
        .is_some_and(|provider| ui.mount.mounts.iter().any(|m| m.provider == provider));
    let mut item = row()
        .key(pane.key())
        .role("button")
        .label(pane.title())
        .h(NAV_ROW_HEIGHT)
        .px(6.0)
        .gap(8.0)
        .center_y()
        .radius(p.k.radius.sm)
        .press(c.pressed)
        .on_drag(move |ui: &mut Ui, d: &mut rune::Drag, tx: &mut Tx| {
            select_on_down(ui, d, tx, pane)
        })
        .on_press_tx(move |ui: &mut Ui, tx: &mut Tx| ui.select_pane(pane, tx))
        // The slot's hover, drawn on the item.
        .child(
            el().key("hover")
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .radius(p.k.radius.sm)
                .fill(c.hover)
                .group_opacity(0.0, 1.0),
        )
        .child(icon(pane.icon(), 16.0).key("icon").color(if on {
            c.icon_base
        } else {
            c.icon_muted
        }))
        .child(
            text(pane.title())
                .key("label")
                .font(p.k.ty.ui)
                .color(if on { c.text_base } else { c.text_muted })
                .grow(1.0)
                .truncate(),
        )
        .children(mounted.then(|| {
            controls::status_dot(p.success())
                .size(6.0, 6.0)
                .radius(3.0)
                .mr(4.0)
        }));
    if on {
        item = item.fill(c.layer_02);
    }
    source_item(item, on)
}

/// The version, and the mount status in a word.
fn footer(ui: &Ui, p: Paint) -> El {
    let line = |key: &str, words: String, color| {
        row().key(key).h(20.0).px(6.0).center_y().child(
            text(words)
                .key("label")
                .font(p.k.ty.small)
                .color(color)
                .truncate(),
        )
    };
    let status = match (ui.operation, ui.mount.mounts.len()) {
        (Some(operation), _) => operation.progress().to_string(),
        (None, 0) => "Nothing mounted".to_string(),
        (None, 1) => "1 drive mounted".to_string(),
        (None, n) => format!("{n} drives mounted"),
    };
    col()
        .key("footer")
        .mt(8.0)
        .child(line("status", status, p.muted()))
        .child(line(
            "version",
            format!("Version {}", env!("CARGO_PKG_VERSION")),
            p.faint(),
        ))
}

/// The title bar of the content panel: the title and the pane's actions.
fn pane_header(p: Paint, title: &str, actions: Vec<El>) -> El {
    title_bar(row())
        .key("pane-header")
        .h(52.0)
        .px(METRICS.page_px + METRICS.label_inset)
        .gap(8.0)
        .center_y()
        .child(
            text(title)
                .key("pane-title")
                .font(p.k.ty.body_medium)
                .color(p.text())
                .shrink()
                .truncate(),
        )
        .child(el().key("header-spacer").grow(1.0))
        .children(actions)
}

/// How far a scroll body's top and bottom edges fade out.
const SCROLL_FADE: (f32, f32) = (4.0, 24.0);

/// A vertically scrolling body whose edges fade out.
fn scroll_body(key: &str, content: El) -> El {
    let (top, bottom) = SCROLL_FADE;
    col()
        .key(key)
        .grow(1.0)
        .shrink()
        .min_h(0.0)
        .clip_fade(0.0, top, 0.0, bottom)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(col().key("inset").pt(top).pb(bottom).child(content))
}

/// The failure banner every pane shows at its top.
fn failure_banner(ui: &Ui, p: Paint) -> Option<El> {
    ui.failure.as_ref().map(|failure| {
        controls::error_panel(
            p,
            "failure",
            &failure.title,
            &failure.detail,
            Some(|ui: &mut Ui| ui.failure = None),
        )
    })
}
