//! Line icons for the sidebar and rows, drawn on a 24 point grid with the
//! same 1.75 pt round stroke as Hex's (Lucide's shapes).

use rune::Icon;

macro_rules! line_icon {
    ($static:ident, $name:literal, [$($element:literal),+ $(,)?]) => {
        pub(super) static $static: Icon = Icon {
            name: $name,
            view: 24.0,
            body: concat!($(
                $element,
                " fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.75\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
            ),+),
        };
    };
}

/// Minimize: a bar as wide as the theme's close cross beside it
/// (`rune_theme_opencode::icons::CLOSE`), on its 16 point grid.
pub(super) static MINIMIZE: Icon = Icon {
    name: "cdm-minimize",
    view: 16.0,
    body: r#"<path d="M4.5 8H11.5" stroke-linecap="round" stroke="currentColor"/>"#,
};

/// Maximize: a square as wide as the close cross.
pub(super) static MAXIMIZE: Icon = Icon {
    name: "cdm-maximize",
    view: 16.0,
    body: r#"<rect x="4.5" y="4.5" width="7" height="7" rx="1" fill="none" stroke="currentColor"/>"#,
};

/// Restore, for a maximized window: a square in front of another, up and to
/// the right, in the same 7 points as [`MAXIMIZE`].
pub(super) static RESTORE: Icon = Icon {
    name: "cdm-restore",
    view: 16.0,
    body: concat!(
        r#"<rect x="4.5" y="6.5" width="5" height="5" rx="0.75" fill="none" stroke="currentColor"/>"#,
        r#"<path d="M6.5 6.5V5.25C6.5 4.84 6.84 4.5 7.25 4.5H10.75C11.16 4.5 11.5 4.84 11.5 5.25V8.75C11.5 9.16 11.16 9.5 10.75 9.5H9.5" fill="none" stroke-linecap="round" stroke="currentColor"/>"#,
    ),
};

line_icon!(
    DRIVES,
    "cdm-drives",
    [
        "<path d=\"M22 12H2\"",
        "<path d=\"M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z\"",
        "<path d=\"M6 16h.01\"",
        "<path d=\"M10 16h.01\"",
    ]
);
line_icon!(
    BUCKET,
    "cdm-bucket",
    [
        "<ellipse cx=\"12\" cy=\"5\" rx=\"9\" ry=\"3\"",
        "<path d=\"M3 5v14a9 3 0 0 0 18 0V5\"",
        "<path d=\"M3 12a9 3 0 0 0 18 0\"",
    ]
);
line_icon!(
    GOOGLE_DRIVE,
    "cdm-google-drive",
    [
        "<path d=\"M8.5 3h7l6.5 11.5-3.5 6h-13L2 14.5z\"",
        "<path d=\"M8.5 3 15 14.5h7\"",
        "<path d=\"M5.5 20.5 12 9\"",
    ]
);
line_icon!(
    CLOUD,
    "cdm-cloud",
    ["<path d=\"M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z\""]
);
line_icon!(
    SERVER,
    "cdm-server",
    [
        "<rect x=\"2\" y=\"2\" width=\"20\" height=\"8\" rx=\"2\" ry=\"2\"",
        "<rect x=\"2\" y=\"14\" width=\"20\" height=\"8\" rx=\"2\" ry=\"2\"",
        "<path d=\"M6 6h.01\"",
        "<path d=\"M6 18h.01\"",
    ]
);
line_icon!(
    SETTINGS,
    "cdm-settings",
    [
        "<path d=\"M10 5H3\"",
        "<path d=\"M12 19H3\"",
        "<path d=\"M14 3v4\"",
        "<path d=\"M16 17v4\"",
        "<path d=\"M21 12h-9\"",
        "<path d=\"M21 19h-5\"",
        "<path d=\"M21 5h-7\"",
        "<path d=\"M8 10v4\"",
        "<path d=\"M8 12H3\"",
    ]
);
line_icon!(
    LOGS,
    "cdm-logs",
    [
        "<path d=\"M15 12h-5\"",
        "<path d=\"M15 8h-5\"",
        "<path d=\"M19 17V5a2 2 0 0 0-2-2H4\"",
        "<path d=\"M8 21h12a2 2 0 0 0 2-2v-1a1 1 0 0 0-1-1H11a1 1 0 0 0-1 1v1a2 2 0 1 1-4 0V5a2 2 0 1 0-4 0v2a1 1 0 0 0 1 1h3\"",
    ]
);
