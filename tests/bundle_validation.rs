use shimeji::format::bundle::{BundleError, MascotBundle};
use std::fs;
use std::path::{Path, PathBuf};

fn copy_dir_recursive(src: &Path, dst: &Path) {
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

fn sample_copy() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bundle");
    copy_dir_recursive(Path::new("tests/fixtures/fixture_bundle"), &dir);
    (tmp, dir)
}

#[test]
fn loads_the_valid_sample_bundle() {
    let (_tmp, dir) = sample_copy();
    let bundle = MascotBundle::load(&dir).unwrap();
    assert_eq!(bundle.manifest.name, "sample_mascot");
    assert_eq!(bundle.animation.animations.len(), 15);
}

#[test]
fn rejects_unsupported_manifest_schema_version() {
    let (_tmp, dir) = sample_copy();
    let manifest_path = dir.join("manifest.json");
    let text = fs::read_to_string(&manifest_path).unwrap();
    let bumped = text.replacen("\"schemaVersion\": 1", "\"schemaVersion\": 2", 1);
    fs::write(&manifest_path, bumped).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::UnsupportedSchemaVersion { found: 2 }));
}

#[test]
fn rejects_missing_sprite_file() {
    let (_tmp, dir) = sample_copy();
    fs::remove_file(dir.join("sprites/0000.webp")).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::SpriteCountMismatch { declared: 13, found: 12, .. }));
}

#[test]
fn rejects_dangling_transition_target() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let broken = text.replacen("\"to\": \"walk_left\"", "\"to\": \"not_a_real_animation\"", 1);
    fs::write(&animation_path, broken).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::UnknownAnimationKey { .. }));
}
