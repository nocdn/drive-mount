//! The menu bar item (macOS) or tray icon (Windows): Settings and Quit.
//! Rune's tray covers Windows; macOS gets a native `NSStatusItem`, as Hex
//! does.

use std::sync::Arc;

#[cfg(target_os = "macos")]
mod macos;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayAction {
    OpenSettings,
    Quit,
}

pub type OnAction = Arc<dyn Fn(TrayAction) + Send + Sync>;

/// Keeps the icon alive; dropping it removes the icon.
pub struct Tray {
    #[cfg(not(target_os = "macos"))]
    inner: Option<rune::Tray>,
}

#[cfg(not(target_os = "macos"))]
fn menu(quitting: bool) -> Vec<rune::TrayItem> {
    vec![
        rune::TrayItem::action("settings", "Settings"),
        rune::TrayItem::Separator,
        if quitting {
            rune::TrayItem::action("quit", "Quitting...").enabled(false)
        } else {
            rune::TrayItem::action("quit", "Quit")
        },
    ]
}

impl Tray {
    /// Installs the icon. On macOS it appears once the app's run loop has
    /// started (AppKit owns the status bar), so call this before `run`.
    pub fn install(on_action: OnAction) -> Tray {
        #[cfg(target_os = "macos")]
        {
            macos::install_when_running(on_action);
            Tray {}
        }
        #[cfg(not(target_os = "macos"))]
        {
            let icon = decode_png(include_bytes!("../../icons/32x32.png"));
            let inner = icon.and_then(|(width, height, rgba)| {
                let spec = rune::TraySpec::new(
                    "com.bartek.clouddrivemount",
                    crate::ui::APP_NAME,
                    rune::TrayIcon::rgba(width, height, rgba),
                )
                .tooltip(crate::ui::APP_NAME)
                .menu(menu(false));
                rune::Tray::new(spec, move |event| match event {
                    rune::TrayEvent::Activate { .. } => on_action(TrayAction::OpenSettings),
                    rune::TrayEvent::Menu(id) if id == "settings" => {
                        on_action(TrayAction::OpenSettings)
                    }
                    rune::TrayEvent::Menu(id) if id == "quit" => on_action(TrayAction::Quit),
                    _ => {}
                })
                .map_err(|err| eprintln!("Could not create the tray icon: {err}"))
                .ok()
            });
            Tray { inner }
        }
    }

    /// Shows that quitting is under way (unmounting can take a while).
    pub fn set_quitting(&self) {
        #[cfg(target_os = "macos")]
        macos::set_quitting();
        #[cfg(not(target_os = "macos"))]
        if let Some(tray) = &self.inner {
            tray.set_menu(menu(true));
        }
    }
}

/// An RGBA image from PNG bytes.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        _ => return None,
    };
    Some((info.width, info.height, rgba))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_tray_icon_decodes_to_rgba() {
        let (width, height, rgba) =
            super::decode_png(include_bytes!("../../icons/32x32.png")).unwrap();
        assert_eq!((width, height), (32, 32));
        assert_eq!(rgba.len(), 32 * 32 * 4);
    }
}
