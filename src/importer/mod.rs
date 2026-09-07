pub mod catalog;
pub mod shimeji_ee;

use crate::format::bundle::{BundleError, MascotBundle};
use catalog::{add_entry, CatalogEntry, CatalogError};
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("not a valid zip file: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("could not extract the zip: {0}")]
    Io(#[from] std::io::Error),
    #[error("this doesn't look like a mascot bundle: {0}")]
    InvalidBundle(#[from] BundleError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error("could not translate shimeji-ee mascot: {0}")]
    ShimejiEe(#[from] shimeji_ee::ShimejiEeError),
}

#[derive(Debug)]
pub struct ImportResult {
    pub entry: CatalogEntry,
    pub skipped: Vec<String>,
}

pub fn import_zip(zip_bytes: &[u8], library_root: &Path) -> Result<ImportResult, ImportError> {
    let scratch = tempfile::tempdir()?;
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    archive.extract(scratch.path())?;

    let skipped = if !scratch.path().join("manifest.json").exists() {
        match shimeji_ee::try_import(scratch.path())? {
            Some(synthesized) => {
                synthesized.write_into(scratch.path())?;
                synthesized.skipped
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let bundle = MascotBundle::load(scratch.path())?;

    let slug = bundle.manifest.name_slug.clone();
    let target_dir = library_root.join(&slug);
    if target_dir.exists() {
        return Err(ImportError::Catalog(CatalogError::DuplicateSlug { slug }));
    }

    std::fs::create_dir_all(library_root)?;
    copy_dir_recursive(scratch.path(), &target_dir)?;

    let entry = CatalogEntry { slug: slug.clone(), name: bundle.manifest.name.clone(), dir: target_dir };
    add_entry(library_root, entry.clone())?;
    Ok(ImportResult { entry, skipped })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
