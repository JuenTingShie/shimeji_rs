use crate::importer::catalog::CatalogEntry;
use std::path::PathBuf;
use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};

pub struct TrayIcon {
    inner: tray_icon::TrayIcon,
}

impl TrayIcon {
    pub fn create() -> tray_icon::Result<TrayIcon> {
        let inner = TrayIconBuilder::new()
            .with_icon(placeholder_icon())
            .with_tooltip("Shimeji")
            .with_menu_on_left_click(false)
            .build()?;
        Ok(TrayIcon { inner })
    }

    /// Replaces the tray icon's menu -- called once after the catalog first loads and again
    /// every time it changes (an import), rather than rebuilt on every right-click as the old
    /// raw Win32 code did, since tray-icon shows whatever menu is currently set automatically.
    pub fn set_menu(&self, menu: Menu) {
        self.inner.set_menu(Some(Box::new(menu)));
    }
}

/// A plain solid-color placeholder icon -- this was already a generic, unbranded OS icon
/// (`IDI_APPLICATION`) before this migration, not a real app icon, so a flat color square is not
/// a visual regression. Swap for a real embedded .ico/.png later if desired; out of scope here.
fn placeholder_icon() -> Icon {
    const SIZE: u32 = 32;
    let mut img = image::RgbaImage::new(SIZE, SIZE);
    for px in img.pixels_mut() {
        *px = image::Rgba([70, 130, 180, 255]);
    }
    Icon::from_rgba(img.into_raw(), SIZE, SIZE).expect("fixed 32x32 opaque buffer is always a valid icon")
}

pub fn build_menu(catalog: &[CatalogEntry]) -> Menu {
    let menu = Menu::new();
    for entry in catalog {
        let item = MenuItem::with_id(format!("spawn:{}", entry.slug), &entry.name, true, None);
        let _ = menu.append(&item);
    }
    let _ = menu.append(&MenuItem::with_id("import", "Import Mascot...", true, None));
    let _ = menu.append(&MenuItem::with_id("close_all", "Close All", true, None));
    let _ = menu.append(&MenuItem::with_id("exit", "Exit", true, None));
    menu
}

pub fn filter_zip_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("zip")))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_zip_paths_case_insensitively() {
        let paths = vec![
            PathBuf::from("C:/drop/usagi.zip"),
            PathBuf::from("C:/drop/readme.txt"),
            PathBuf::from("C:/drop/OTHER.ZIP"),
        ];
        let kept = filter_zip_paths(&paths);
        assert_eq!(kept, vec![PathBuf::from("C:/drop/usagi.zip"), PathBuf::from("C:/drop/OTHER.ZIP")]);
    }

    #[test]
    fn builds_a_menu_item_per_catalog_entry_plus_the_three_fixed_items() {
        let catalog = vec![
            CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: PathBuf::from("usagi") },
            CatalogEntry { slug: "neko".into(), name: "neko".into(), dir: PathBuf::from("neko") },
        ];
        let menu = build_menu(&catalog);
        assert_eq!(menu.items().len(), 5); // 2 catalog entries + import + close_all + exit
    }
}
