use crate::importer::catalog::CatalogEntry;
use std::path::PathBuf;

pub const SPAWN_MENU_ID_BASE: u16 = 1000;
pub const IMPORT_MENU_ID: u16 = 1;
pub const CLOSE_ALL_MENU_ID: u16 = 2;
pub const EXIT_MENU_ID: u16 = 3;

pub fn filter_zip_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("zip")))
        .cloned()
        .collect()
}

pub fn build_spawn_items(catalog: &[CatalogEntry]) -> Vec<(u16, String)> {
    catalog
        .iter()
        .enumerate()
        .map(|(i, entry)| (SPAWN_MENU_ID_BASE + i as u16, entry.name.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::catalog::CatalogEntry;
    use std::path::PathBuf;

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
    fn builds_stable_menu_ids_starting_at_the_spawn_base() {
        let catalog = vec![
            CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: PathBuf::from("usagi") },
            CatalogEntry { slug: "neko".into(), name: "neko".into(), dir: PathBuf::from("neko") },
        ];
        let items = build_spawn_items(&catalog);
        assert_eq!(items, vec![(SPAWN_MENU_ID_BASE, "usagi".to_string()), (SPAWN_MENU_ID_BASE + 1, "neko".to_string())]);
    }
}
