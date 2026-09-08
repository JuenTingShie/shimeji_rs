use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub slug: String,
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("could not read catalog.json: {0}")]
    Read(#[source] std::io::Error),
    #[error("could not write catalog.json: {0}")]
    Write(#[source] std::io::Error),
    #[error("catalog.json is corrupt: {0}")]
    Parse(#[source] serde_json::Error),
    #[error("a mascot with slug '{slug}' is already in the library")]
    DuplicateSlug { slug: String },
}

fn catalog_path(library_root: &Path) -> PathBuf {
    library_root.join("catalog.json")
}

pub fn load_catalog(library_root: &Path) -> Result<Vec<CatalogEntry>, CatalogError> {
    let path = catalog_path(library_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(CatalogError::Read)?;
    let entries: Vec<CatalogEntry> = serde_json::from_str(&text).map_err(CatalogError::Parse)?;

    // A catalog entry whose bundle was deleted or moved out from under it (manually, or by a
    // failed/partial import cleanup) would otherwise keep showing up in the tray menu forever and
    // fail to spawn -- drop it here and persist the pruned list so it doesn't come back.
    let (present, missing): (Vec<_>, Vec<_>) =
        entries.into_iter().partition(|e| e.dir.join("manifest.json").exists());
    if !missing.is_empty() {
        save_catalog(library_root, &present)?;
    }
    Ok(present)
}

pub fn save_catalog(library_root: &Path, entries: &[CatalogEntry]) -> Result<(), CatalogError> {
    std::fs::create_dir_all(library_root).map_err(CatalogError::Write)?;
    let text = serde_json::to_string_pretty(entries).expect("CatalogEntry always serializes");
    std::fs::write(catalog_path(library_root), text).map_err(CatalogError::Write)
}

pub fn add_entry(library_root: &Path, entry: CatalogEntry) -> Result<(), CatalogError> {
    let mut entries = load_catalog(library_root)?;
    if entries.iter().any(|e| e.slug == entry.slug) {
        return Err(CatalogError::DuplicateSlug { slug: entry.slug });
    }
    entries.push(entry);
    save_catalog(library_root, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_catalog_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        assert!(load_catalog(root).unwrap().is_empty());

        let dir = root.join("usagi");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), "{}").unwrap();
        add_entry(root, CatalogEntry {
            slug: "usagi".into(),
            name: "usagi".into(),
            dir,
        }).unwrap();

        let entries = load_catalog(root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].slug, "usagi");
        assert_eq!(entries[0].name, "usagi");
    }

    #[test]
    fn prunes_entries_whose_manifest_is_missing_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let present_dir = root.join("usagi");
        std::fs::create_dir_all(&present_dir).unwrap();
        std::fs::write(present_dir.join("manifest.json"), "{}").unwrap();

        save_catalog(
            root,
            &[
                CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: present_dir },
                CatalogEntry { slug: "neko".into(), name: "neko".into(), dir: root.join("neko") },
            ],
        )
        .unwrap();

        let entries = load_catalog(root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].slug, "usagi");

        // The prune should have been persisted, not just returned in-memory.
        let reloaded_text = std::fs::read_to_string(catalog_path(root)).unwrap();
        assert!(!reloaded_text.contains("neko"));
    }

    #[test]
    fn rejects_duplicate_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dir = root.join("usagi");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), "{}").unwrap();

        add_entry(root, CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: dir.clone() }).unwrap();
        let err = add_entry(root, CatalogEntry { slug: "usagi".into(), name: "usagi again".into(), dir })
            .unwrap_err();
        assert!(matches!(err, CatalogError::DuplicateSlug { .. }));
    }
}
