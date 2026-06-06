//! macOS menu bar tray icon loaded from a dedicated Retina template asset.

use tauri::image::Image;

const MENU_BAR_ICON_PNG: &[u8] = include_bytes!("../icons/menu-bar-template.png");

#[cfg(test)]
pub(crate) const MENU_BAR_ICON_PIXEL_SIZE: u32 = 36;
#[cfg(test)]
pub(crate) const MENU_BAR_ICON_SOURCE: &str = "src-tauri/icons/menu-bar-template.svg";
#[cfg(test)]
pub(crate) const MENU_BAR_ICON_ASSET: &str = "src-tauri/icons/menu-bar-template.png";

pub(crate) fn load() -> Image<'static> {
    Image::from_bytes(MENU_BAR_ICON_PNG)
        .expect("embedded macOS menu bar template icon should be a valid PNG")
}

#[cfg(test)]
mod tests {
    use super::{load, MENU_BAR_ICON_ASSET, MENU_BAR_ICON_PIXEL_SIZE, MENU_BAR_ICON_SOURCE};

    #[test]
    fn uses_retina_template_asset() {
        let image = load();

        assert_eq!(image.width(), MENU_BAR_ICON_PIXEL_SIZE);
        assert_eq!(image.height(), MENU_BAR_ICON_PIXEL_SIZE);
        assert_eq!(
            MENU_BAR_ICON_SOURCE,
            "src-tauri/icons/menu-bar-template.svg"
        );
        assert_eq!(MENU_BAR_ICON_ASSET, "src-tauri/icons/menu-bar-template.png");
    }
}
