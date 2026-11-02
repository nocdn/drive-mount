//! The Logs pane: what the app and rclone reported, newest at the bottom.

use std::collections::VecDeque;

use rune::{col, text, El, ScrollSpec, Span, Tx};

use super::controls::{button, message, raised, ButtonKind, Paint, METRICS};
use super::Ui;
use crate::models::LogLine;

/// The log as plain text, one line per entry, as Copy puts it.
pub(super) fn plain_text(lines: &VecDeque<LogLine>) -> String {
    lines
        .iter()
        .map(|line| format!("[{}] [{}] {}\n", line.timestamp, line.level, line.message))
        .collect()
}

pub(super) fn actions(ui: &Ui, p: Paint) -> Vec<El> {
    let empty = ui.logs.is_empty();
    vec![
        button(
            p,
            "clear-logs",
            "Clear",
            ButtonKind::Secondary,
            (!empty).then(|| Box::new(|ui: &mut Ui, _: &mut Tx| ui.clear_logs()) as _),
        ),
        button(
            p,
            "copy-logs",
            if ui.copied() { "Copied" } else { "Copy" },
            ButtonKind::Secondary,
            (!empty).then(|| Box::new(|ui: &mut Ui, tx: &mut Tx| ui.copy_logs(tx)) as _),
        ),
    ]
}

pub(super) fn pane(ui: &Ui, p: Paint) -> El {
    let body = if ui.logs.is_empty() {
        message(p, "empty", "Nothing logged yet", None)
    } else {
        // One selectable text, so a drag can select across lines; spans dim
        // the timestamps and colour the levels.
        let all = plain_text(&ui.logs);
        let mut spans = Vec::new();
        let mut at = 0;
        for line in &ui.logs {
            let stamp = line.timestamp.len() + 2;
            let level = line.level.len() + 2;
            spans.push(Span {
                color: Some(p.faint()),
                ..Span::new(at, at + stamp)
            });
            let level_color = if line.level == "ERROR" {
                p.danger()
            } else {
                p.muted()
            };
            spans.push(Span {
                color: Some(level_color),
                ..Span::new(at + stamp + 1, at + stamp + 1 + level)
            });
            at += stamp + 1 + level + 1 + line.message.len() + 1;
        }
        text(all.trim_end().to_string())
            .key("log-text")
            .font(p.k.ty.mono)
            .color(p.text())
            .spans(spans)
            .wrap_anywhere()
            .selectable(true)
            .selection_color(p.k.selection())
            .px(METRICS.row_px)
            .py(12.0)
    };
    col()
        .key("logs-body")
        .grow(1.0)
        .shrink()
        .min_h(0.0)
        .px(METRICS.page_px)
        .pb(METRICS.page_px)
        .child(
            raised(p, "log-card")
                .grow(1.0)
                .shrink()
                .min_h(0.0)
                .clip_soft(p.k.radius.lg)
                .child(
                    col()
                        .key("log-scroll")
                        .grow(1.0)
                        .shrink()
                        .min_h(0.0)
                        .scroll(ScrollSpec { stick_bottom: true })
                        .child(body),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_matches_the_log_file_format() {
        let lines = VecDeque::from(vec![LogLine {
            level: "INFO".into(),
            message: "Mounting".into(),
            timestamp: "09:41:00".into(),
        }]);
        assert_eq!(plain_text(&lines), "[09:41:00] [INFO] Mounting\n");
    }
}
