//! Settings controls on `rune-kit`, in OpenCode's design language
//! (`rune-theme-opencode`), following Hex's building blocks: the page, the
//! raised card of rows, the row, switch, select menu, buttons, tags and
//! notes. Colours, type, radii, elevation and motion come from the kit's
//! tokens; the numbers left here are layout metrics.

use rune::{col, edit_access, el, row, text, Color, El, TextEdit, TextStyle, Tx};
use rune_kit::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, Menu, MenuItem,
    Placement, Select, SelectAppearance, Switch, TextInput,
};

use super::Ui;

/// Row descriptions and secondary copy.
pub(super) const DESCRIPTION: TextStyle = TextStyle::new(12.0, 440.0).lh(16.0);
/// Text fields in a row's control slot.
pub(super) const FIELD_WIDTH: f32 = 260.0;
/// Narrower than this, a row moves its control below the copy instead of
/// squeezing the description to a word per line.
const COPY_MIN_WIDTH: f32 = 100.0;
const SECTION_LABEL_LINE: f32 = 16.0;

/// A settings page's measurements: raised cards of rows under muted
/// labels. Cards sit `page_px` in from the panel's edges; the title, its
/// actions, the section labels and the captions under cards sit just inside
/// the cards' edges, `label_inset` further in, while row text keeps the
/// cards' own `row_px` padding.
#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub(super) page_px: f32,
    pub(super) label_inset: f32,
    pub(super) section_gap: f32,
    pub(super) label_gap: f32,
    pub(super) row_px: f32,
    pub(super) row_h: f32,
    pub(super) row_h_described: f32,
}

pub(super) const METRICS: Metrics = Metrics {
    page_px: 16.0,
    label_inset: 4.0,
    section_gap: 24.0,
    label_gap: 8.0,
    row_px: 16.0,
    row_h: 48.0,
    row_h_described: 60.0,
};

/// A settings page's column of sections.
pub(super) fn page(key: &str) -> El {
    col().key(key).px(METRICS.page_px).gap(METRICS.section_gap)
}

/// Everything a control needs to paint itself.
#[derive(Clone, Copy)]
pub(super) struct Paint {
    pub(super) k: &'static Kit,
    pub(super) hair: f32,
}

impl Paint {
    pub(super) fn new(dark: bool, scale: f32) -> Self {
        Self {
            k: rune_theme_opencode::kit(dark),
            hair: 1.0 / scale,
        }
    }
    /// A section label: above its card, on the page's text edge.
    pub(super) fn section_label(self, title: &str) -> El {
        text(title)
            .key("header")
            .font(self.k.ty.ui_medium.lh(SECTION_LABEL_LINE))
            .color(self.k.c.text_muted)
    }
    pub(super) fn text(self) -> Color {
        self.k.c.text_base
    }
    pub(super) fn muted(self) -> Color {
        self.k.c.text_muted
    }
    pub(super) fn faint(self) -> Color {
        self.k.c.text_faint
    }
    pub(super) fn danger(self) -> Color {
        self.k.c.danger.fg
    }
    pub(super) fn success(self) -> Color {
        self.k.c.success.fg
    }
}

/// A titled card of rows with hairline separators between them.
pub(super) fn section(p: Paint, key: &str, title: &str, rows: Vec<El>) -> El {
    col()
        .key(key)
        .gap(METRICS.label_gap)
        .child(p.section_label(title).mx(METRICS.label_inset))
        .child(card(p, "card", rows))
}

/// A raised card of rows with hairline separators between them.
pub(super) fn card(p: Paint, key: &str, rows: Vec<El>) -> El {
    inset_card(p, key, rows, METRICS.row_px)
}

/// A [`card`] whose rows, and so its separators, sit `inset` from its sides.
pub(super) fn inset_card(p: Paint, key: &str, rows: Vec<El>, inset: f32) -> El {
    let mut children = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            children.push(separator(p, format!("sep{}", index - 1), inset));
        }
        children.push(row);
    }
    raised(p, key).children(children)
}

/// The card's raised surface, empty.
pub(super) fn raised(p: Paint, key: &str) -> El {
    let mut surface = col().key(key).fill(p.k.c.bg_base).radius(p.k.radius.lg);
    for shadow in &p.k.elevation.raised {
        surface = surface.box_shadow(*shadow);
    }
    surface
}

fn separator(p: Paint, key: String, inset: f32) -> El {
    el().key(key)
        .h(p.hair)
        .ml(inset)
        .mr(inset)
        .fill(p.k.c.border_muted)
}

/// A setting: title and optional description on the left, its control on
/// the right.
pub(super) fn setting_row(
    p: Paint,
    key: &str,
    title: &str,
    description: Option<&str>,
    control: El,
) -> El {
    described_row(
        p,
        key,
        title,
        description.map(|text| (text.to_string(), p.muted())),
        control,
    )
}

/// A [`setting_row`] whose description has its own colour (errors, status).
pub(super) fn described_row(
    p: Paint,
    key: &str,
    title: &str,
    description: Option<(String, Color)>,
    control: El,
) -> El {
    let description = description.map(|(copy, color)| {
        text(copy)
            .key("description")
            .font(DESCRIPTION)
            .color(color)
            .wrap()
    });
    copy_row(p, key, None, title, description, control)
}

/// A [`setting_row`] led by an icon.
pub(super) fn icon_row(
    p: Paint,
    key: &str,
    glyph: &'static rune::Icon,
    title: &str,
    description: Option<(String, Color)>,
    control: El,
) -> El {
    let description = description.map(|(copy, color)| {
        text(copy)
            .key("description")
            .font(DESCRIPTION)
            .color(color)
            .wrap()
    });
    copy_row(p, key, Some(glyph), title, description, control)
}

fn copy_row(
    p: Paint,
    key: &str,
    glyph: Option<&'static rune::Icon>,
    title: &str,
    description: Option<El>,
    control: El,
) -> El {
    let m = METRICS;
    let height = if description.is_some() {
        m.row_h_described
    } else {
        m.row_h
    };
    let copy = col()
        .key("copy")
        .w(COPY_MIN_WIDTH)
        .min_w(COPY_MIN_WIDTH)
        .grow(1.0)
        .gap(2.0)
        .child(text(title).key("label").font(p.k.ty.ui).color(p.text()))
        .children(description);
    row()
        .key(key)
        .min_h(height)
        .px(m.row_px)
        .py(8.0)
        .gaps(8.0, 24.0)
        .center_y()
        .justify_between()
        .flex_wrap()
        .radius(p.k.radius.md)
        .label(title)
        .child(match glyph {
            None => copy,
            Some(glyph) => row()
                .key("lead")
                .w(COPY_MIN_WIDTH + 28.0)
                .min_w(COPY_MIN_WIDTH + 28.0)
                .grow(1.0)
                .gap(12.0)
                .center_y()
                .child(rune::icon(glyph, 16.0).key("icon").color(p.k.c.icon_base))
                .child(copy),
        })
        .child(control)
}

/// The switch of a row that toggles: the row takes focus and presses, and
/// the switch shows its state.
pub(super) fn row_switch(p: Paint, on: bool) -> El {
    Switch::new(on).build(p.k).key("switch").focusable(false)
}

/// A whole-row switch: the row presses, the switch shows the state.
pub(super) fn toggle_row(
    p: Paint,
    key: &str,
    title: &str,
    description: Option<&str>,
    on: bool,
    flip: impl Fn(&mut Ui) + 'static,
) -> El {
    setting_row(p, key, title, description, row_switch(p, on))
        .role("toggle")
        .focusable(true)
        .on_press(flip)
}

/// One choice of a [`select`].
pub(super) struct Choice {
    pub(super) label: String,
    pub(super) selected: bool,
    pub(super) disabled: bool,
    pub(super) choose: Box<dyn Fn(&mut Ui)>,
}

/// A select field whose choices open in an anchored menu above the page.
#[allow(clippy::too_many_arguments)]
pub(super) fn select(
    p: Paint,
    key: &str,
    label: &str,
    width: f32,
    open: bool,
    choices: Vec<Choice>,
    toggle: impl Fn(&mut Ui) + 'static,
    layer: &mut Vec<El>,
) -> El {
    let trigger = Select::new::<&str>(format!("{key}-field"), &[])
        .appearance(SelectAppearance::Field)
        .width(width)
        .current(Some(label))
        .open(open)
        .on_toggle(toggle)
        .trigger(p.k);
    let mut menu = Menu::new(key)
        .trigger(trigger)
        .placement(Placement::BottomEnd)
        .gutter(4.0)
        .min_width(width)
        .open(open)
        .on_close(|ui: &mut Ui| ui.open = None);
    for choice in choices {
        let choose = choice.choose;
        menu = menu.item(
            MenuItem::new(choice.label)
                .checked(choice.selected)
                .disabled(choice.disabled)
                .on_select(move |ui: &mut Ui| {
                    ui.open = None;
                    choose(ui);
                }),
        );
    }
    menu.build(p.k, layer)
}

/// One entry of an [`overflow`] menu.
pub(super) struct Action {
    pub(super) label: String,
    pub(super) disabled: bool,
    pub(super) run: Box<dyn Fn(&mut Ui)>,
}

/// The "More" button and its menu: the actions that don't earn a place in
/// the title bar.
pub(super) fn overflow(
    p: Paint,
    key: &str,
    open: bool,
    actions: Vec<Action>,
    toggle: impl Fn(&mut Ui) + 'static,
    layer: &mut Vec<El>,
) -> El {
    let mut menu = Menu::new(key)
        .placement(Placement::BottomEnd)
        .gutter(4.0)
        .min_width(200.0)
        .open(open)
        .on_close(|ui: &mut Ui| ui.open = None);
    for action in actions {
        let run = action.run;
        menu = menu.item(
            MenuItem::new(action.label)
                .disabled(action.disabled)
                .on_select(move |ui: &mut Ui| {
                    ui.open = None;
                    run(ui);
                }),
        );
    }
    let trigger = IconButton::new(&rune_theme_opencode::icons::OUTLINE_DOTS)
        .variant(IconButtonVariant::Neutral)
        .control_size(ControlSize::Large)
        .label("More")
        .on_press(toggle)
        .build(p.k)
        .key(format!("{key}-trigger"));
    menu.trigger(trigger).build(p.k, layer)
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ButtonKind {
    /// The one main action on screen: the kit's contrast button.
    Primary,
    Secondary,
}

/// Push buttons at Hex's sizes, which keep tighter padding and corners than
/// the kit's `ControlSize`s of the same height.
pub(super) trait PushSize {
    /// 28 pt, the kit's Large height.
    fn build_push(self, k: &Kit) -> El;
    /// 24 pt, for actions inside rows.
    fn build_small_push(self, k: &Kit) -> El;
}

impl PushSize for Button {
    fn build_push(self, k: &Kit) -> El {
        self.control_size(ControlSize::Large)
            .build(k)
            .px(11.0)
            .radius(k.radius.md)
    }

    fn build_small_push(self, k: &Kit) -> El {
        self.control_size(ControlSize::Regular)
            .build(k)
            .px(9.0)
            .radius(k.radius.sm)
    }
}

fn variant(kind: ButtonKind) -> ButtonVariant {
    match kind {
        ButtonKind::Primary => ButtonVariant::Contrast,
        ButtonKind::Secondary => ButtonVariant::Neutral,
    }
}

/// What a button does; `None` while it can't be pressed.
pub(super) type Press = Option<Box<dyn Fn(&mut Ui, &mut Tx)>>;

/// A push button. `press` is `None` while it can't be pressed.
pub(super) fn button(p: Paint, key: &str, label: &str, kind: ButtonKind, press: Press) -> El {
    let mut button = Button::new(label).variant(variant(kind));
    button = match press {
        Some(press) => button.on_press_tx(move |ui: &mut Ui, tx: &mut Tx| press(ui, tx)),
        None => button.disabled(true),
    };
    button.build_push(p.k).key(key)
}

/// A compact button for actions inside a row.
pub(super) fn small_button(p: Paint, key: &str, label: &str, kind: ButtonKind, press: Press) -> El {
    let mut button = Button::new(label).variant(variant(kind));
    button = match press {
        Some(press) => button.on_press_tx(move |ui: &mut Ui, tx: &mut Tx| press(ui, tx)),
        None => button.disabled(true),
    };
    button.build_small_push(p.k).key(key)
}

/// The × that removes one item of an editable list.
pub(super) fn remove_button(p: Paint, key: &str, press: impl Fn(&mut Ui) + 'static) -> El {
    IconButton::new(&rune_theme_opencode::icons::CLOSE)
        .variant(IconButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .label("Remove")
        .on_press(press)
        .build(p.k)
        .key(key)
}

/// A one-line text field. Edits go to the `TextEdit` `access` returns, and
/// every change marks the form as edited, which saves it once typing pauses.
pub(super) fn text_field(
    p: Paint,
    key: &str,
    edit: &TextEdit,
    access: fn(&mut Ui) -> &mut TextEdit,
    placeholder: &str,
    width: f32,
    invalid: bool,
) -> El {
    TextInput::new(edit)
        .placeholder(placeholder)
        .width(Some(width))
        .invalid(invalid)
        .on_edit(edit_access(access))
        .on_change(|ui: &mut Ui, tx: &mut Tx| ui.edited(tx))
        .build(p.k)
        .key(key)
}

/// A [`text_field`] that hides what is typed.
pub(super) fn secret_field(
    p: Paint,
    key: &str,
    edit: &TextEdit,
    access: fn(&mut Ui) -> &mut TextEdit,
    placeholder: &str,
    invalid: bool,
) -> El {
    TextInput::new(edit)
        .secure()
        .placeholder(placeholder)
        .width(Some(FIELD_WIDTH))
        .invalid(invalid)
        .on_edit(edit_access(access))
        .on_change(|ui: &mut Ui, tx: &mut Tx| ui.edited(tx))
        .build(p.k)
        .key(key)
}

pub(super) fn status_dot(color: Color) -> El {
    el().key("dot").size(8.0, 8.0).radius(4.0).fill(color)
}

/// Breathes between full and this opacity every two seconds; full under
/// Reduce Motion.
const PULSE: rune::LoopSpec = rune::LoopSpec::opacity(2.0, 0.0, 1.0, 0.45);

/// A dot for something under way.
pub(super) fn pulsing_dot(color: Color) -> El {
    status_dot(color).looping(PULSE)
}

/// A caption under a card, on the section label's edge.
pub(super) fn note(key: &str, copy: &str, color: Color) -> El {
    row().key(key).px(METRICS.label_inset).py(8.0).child(
        text(copy)
            .key("copy")
            .font(DESCRIPTION)
            .color(color)
            .wrap()
            .grow(1.0),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tone {
    Neutral,
    Success,
    Danger,
}

/// A small tinted status label, never pressable.
pub(super) fn tone_tag(p: Paint, key: &str, label: &str, tone: Tone) -> El {
    let c = &p.k.c;
    let (fill, border, ink) = match tone {
        Tone::Neutral => (c.layer_02, c.border_base, c.text_muted),
        Tone::Success => (c.success.bg, c.success.border, c.success.fg),
        Tone::Danger => (c.danger.bg, c.danger.border, c.danger.fg),
    };
    row()
        .key(key)
        .role("status")
        .label(label)
        .h(18.0)
        .px(6.0)
        .gap(4.0)
        .center()
        .radius(p.k.radius.xs)
        .fill(fill)
        .border(0.5, border)
        .child(
            text(label)
                .key("label")
                .font(p.k.ty.small_medium.lh(12.0))
                .color(ink),
        )
}

/// A quiet message where a list has nothing to show.
pub(super) fn message(p: Paint, key: &str, title: &str, detail: Option<&str>) -> El {
    col()
        .key(key)
        .px(METRICS.row_px)
        .py(20.0)
        .gap(4.0)
        .child(
            text(title)
                .key("title")
                .font(p.k.ty.ui)
                .color(p.muted())
                .wrap(),
        )
        .children(detail.map(|detail| {
            text(detail)
                .key("detail")
                .font(DESCRIPTION)
                .color(p.faint())
                .wrap()
        }))
}

/// An error title and its detail as a tinted panel, with a close button
/// when it stays until dismissed.
pub(super) fn error_panel(
    p: Paint,
    key: &str,
    title: &str,
    detail: &str,
    dismiss: Option<fn(&mut Ui)>,
) -> El {
    let d = &p.k.c.danger;
    row()
        .key(key)
        .pl(12.0)
        .pr(6.0)
        .py(6.0)
        .gap(8.0)
        .radius(p.k.radius.md)
        .fill(d.bg)
        .border(0.5, d.border)
        .child(
            col()
                .key("message")
                .grow(1.0)
                .shrink()
                .py(4.0)
                .gap(2.0)
                .child(text(title).key("title").font(p.k.ty.ui_medium).color(d.fg))
                .child(
                    text(detail)
                        .key("detail")
                        .font(DESCRIPTION)
                        .color(p.muted())
                        .wrap(),
                ),
        )
        .children(dismiss.map(|dismiss| {
            IconButton::new(&rune_theme_opencode::icons::CLOSE)
                .variant(IconButtonVariant::Ghost)
                .label("Close")
                .on_press(dismiss)
                .build(p.k)
                .key("dismiss")
        }))
}

/// A source list, as in a native sidebar: Tab enters at the selected row,
/// Up and Down select as they move.
pub(super) fn source_list(list: El) -> El {
    list.role("list").focus_group().focus_follows()
}

/// A row of a [`source_list`]; the selected fill shows where focus is.
pub(super) fn source_item(item: El, selected: bool) -> El {
    item.focusable(true).focus_selected(selected)
}
