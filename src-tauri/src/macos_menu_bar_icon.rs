//! macOS menu bar tray icon — uses the same SF Symbol as the legacy native app.

use objc2_app_kit::NSImage;
use objc2_foundation::NSString;
use tauri::image::Image;

pub(crate) const SYMBOL_NAME: &str = "externaldrive.badge.icloud";
const ACCESSIBILITY_DESCRIPTION: &str = "Cloud Drive Mount";

pub(crate) fn load(app: &tauri::App) -> Image<'_> {
    if let Some(icon) = try_load() {
        return icon;
    }

    eprintln!("Warning: Could not load SF Symbol '{SYMBOL_NAME}' for menu bar; using app icon");
    app.default_window_icon()
        .expect("default window icon should exist")
        .clone()
}

fn try_load() -> Option<Image<'static>> {
    let name = NSString::from_str(SYMBOL_NAME);
    let description = NSString::from_str(ACCESSIBILITY_DESCRIPTION);
    let ns_image =
        NSImage::imageWithSystemSymbolName_accessibilityDescription(&name, Some(&description))?;

    ns_image.setTemplate(true);

    let tiff = ns_image.TIFFRepresentation()?;
    let decoded = image::load_from_memory(&tiff.to_vec()).ok()?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();

    Some(Image::new(rgba.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::SYMBOL_NAME;

    #[test]
    fn uses_legacy_mac_app_menu_bar_symbol() {
        assert_eq!(SYMBOL_NAME, "externaldrive.badge.icloud");
    }
}
