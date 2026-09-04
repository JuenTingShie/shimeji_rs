use shimeji::importer::{import_zip, ImportError};
use shimeji::importer::catalog::load_catalog;
use std::fs;

#[test]
fn imports_the_sample_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();
    let zip_bytes = fs::read("tests/fixtures/fixture_bundle.zip").unwrap();

    let entry = import_zip(&zip_bytes, library_root).unwrap();
    assert_eq!(entry.slug, "sample_mascot");
    assert_eq!(entry.name, "sample_mascot");
    assert!(entry.dir.join("manifest.json").is_file());
    assert!(entry.dir.join("sprites/0000.webp").is_file());

    let catalog = load_catalog(library_root).unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].slug, "sample_mascot");
}

#[test]
fn rejects_a_corrupt_zip_without_writing_anything() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();

    let err = import_zip(b"not a zip file", library_root).unwrap_err();
    assert!(matches!(err, ImportError::Zip(_)));
    assert!(load_catalog(library_root).unwrap().is_empty());
    assert!(!library_root.join("usagi").exists());
}

#[test]
fn rejects_importing_the_same_slug_twice() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();
    let zip_bytes = fs::read("tests/fixtures/fixture_bundle.zip").unwrap();

    import_zip(&zip_bytes, library_root).unwrap();
    let err = import_zip(&zip_bytes, library_root).unwrap_err();
    assert!(matches!(err, ImportError::Catalog(_)));
}
