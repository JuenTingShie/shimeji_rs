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
    serde_json::from_str(&text).map_err(CatalogError::Parse)
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

        add_entry(root, CatalogEntry {
            slug: "usagi".into(),
            name: "usagi".into(),
            dir: root.join("usagi"),
        }).unwrap();

        let entries = load_catalog(root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].slug, "usagi");
        assert_eq!(entries[0].name, "usagi");
    }

    #[test]
    fn rejects_duplicate_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        add_entry(root, CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: root.join("usagi") }).unwrap();
        let err = add_entry(root, CatalogEntry { slug: "usagi".into(), name: "usagi again".into(), dir: root.join("usagi") })
            .unwrap_err();
        assert!(matches!(err, CatalogError::DuplicateSlug { .. }));
    }
}
