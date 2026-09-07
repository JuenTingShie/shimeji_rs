use shimeji::importer::{import_zip, ImportError};
use shimeji::importer::catalog::load_catalog;
use std::fs;

#[test]
fn imports_the_sample_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();
    let zip_bytes = fs::read("tests/fixtures/fixture_bundle.zip").unwrap();

    let result = import_zip(&zip_bytes, library_root).unwrap();
    assert_eq!(result.entry.slug, "sample_mascot");
    assert_eq!(result.entry.name, "sample_mascot");
    assert!(result.entry.dir.join("manifest.json").is_file());
    assert!(result.entry.dir.join("sprites/0000.webp").is_file());
    assert!(result.skipped.is_empty());

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

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else {
            fs::copy(entry.path(), &dst_path).unwrap();
        }
    }
}

fn write_placeholder_png(path: &std::path::Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::new(4, 4).save(path).unwrap();
}

fn build_shimeji_ee_source_tree(root: &std::path::Path) {
    copy_dir_recursive(std::path::Path::new("tests/fixtures/fixture_shimeji_ee/conf"), &root.join("conf"));
    for name in ["stand.png", "walk1.png", "walk2.png", "falling.png", "pinched_left.png", "pinched_right.png", "ieaction.png"] {
        write_placeholder_png(&root.join("img/TestMascot").join(name));
    }
}

fn zip_dir_with_prefix(src: &std::path::Path, prefix: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    add_dir_to_zip(&mut writer, src, prefix, options);
    writer.finish().unwrap().into_inner()
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    dir: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) {
    use std::io::Write;
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.file_type().unwrap().is_dir() {
            add_dir_to_zip(writer, &entry.path(), &format!("{name}/"), options);
        } else {
            writer.start_file(&name, options).unwrap();
            writer.write_all(&fs::read(entry.path()).unwrap()).unwrap();
        }
    }
}

#[test]
fn imports_a_bare_shimeji_ee_character_folder_zip() {
    let source = tempfile::tempdir().unwrap();
    build_shimeji_ee_source_tree(source.path());
    let zip_bytes = zip_dir_with_prefix(source.path(), "");

    let tmp = tempfile::tempdir().unwrap();
    let result = import_zip(&zip_bytes, tmp.path()).unwrap();

    assert_eq!(result.entry.name, "TestMascot");
    assert!(!result.skipped.is_empty(), "the fixture is designed to exercise several degraded/dropped paths");

    let bundle = shimeji::format::bundle::MascotBundle::load(&result.entry.dir).unwrap();
    assert_eq!(bundle.animation.default_animation, "Falling");
}

#[test]
fn imports_a_shimeji_ee_zip_nested_under_a_full_app_distribution_prefix() {
    let source = tempfile::tempdir().unwrap();
    build_shimeji_ee_source_tree(&source.path().join("shimejiee"));
    let zip_bytes = zip_dir_with_prefix(source.path(), "");

    let tmp = tempfile::tempdir().unwrap();
    let result = import_zip(&zip_bytes, tmp.path()).unwrap();

    let bundle = shimeji::format::bundle::MascotBundle::load(&result.entry.dir).unwrap();
    assert_eq!(bundle.animation.default_animation, "Falling");
}
