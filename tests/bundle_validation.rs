use shimeji::format::animation::EngineEventKind;
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

#[test]
fn accepts_pc_import_v1_animation_schema() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let swapped = text.replacen("\"schema_id\": \"legacy_default_v1\"", "\"schema_id\": \"pc_import_v1\"", 1);
    fs::write(&animation_path, swapped).unwrap();

    let bundle = MascotBundle::load(&dir).unwrap();
    assert_eq!(bundle.animation.schema_id, "pc_import_v1");
}

#[test]
fn rejects_unknown_animation_schema_id() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let swapped = text.replacen("\"schema_id\": \"legacy_default_v1\"", "\"schema_id\": \"totally_unknown_v9\"", 1);
    fs::write(&animation_path, swapped).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::UnsupportedAnimationSchema { found } if found == "totally_unknown_v9"));
}

#[test]
fn accepts_pc_import_v1_bundle_with_frames_missing_dx_dy() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let swapped = text
        .replacen("\"schema_id\": \"legacy_default_v1\"", "\"schema_id\": \"pc_import_v1\"", 1)
        .replacen("{ \"sprite\": 0, \"dx\": 0, \"dy\": 15, \"durationTicks\": 20 }", "{ \"sprite\": 0, \"durationTicks\": 20 }", 1);
    fs::write(&animation_path, swapped).unwrap();

    let bundle = MascotBundle::load(&dir).unwrap();
    let frame = &bundle.animation.animations[0].frames[0];
    assert_eq!(frame.dx, 0);
    assert_eq!(frame.dy, 0);
}

#[test]
fn merges_pc_import_v1_inline_event_transitions_into_top_level_events() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    // pc_import_v1 exporters attach DRAG_END to the `drag` animation itself instead of the
    // schema's top-level `events` array -- move the fixture's rule there to reproduce that shape.
    let swapped = text
        .replacen("\"schema_id\": \"legacy_default_v1\"", "\"schema_id\": \"pc_import_v1\"", 1)
        .replacen("{ \"event\": \"DRAG_END\", \"from\": \"drag\", \"to\": \"fall\", \"setFacing\": \"RANDOM\" },\r\n", "", 1)
        .replacen(
            "\"direction\": \"ANY\",\r\n      \"frames\": [\r\n        { \"sprite\": 9, \"dx\": 0, \"dy\": 0, \"durationTicks\": 8 }\r\n      ]\r\n    },",
            "\"direction\": \"ANY\",\r\n      \"frames\": [\r\n        { \"sprite\": 9, \"dx\": 0, \"dy\": 0, \"durationTicks\": 8 }\r\n      ],\r\n      \"eventTransitions\": [\r\n        { \"event\": \"DRAG_END\", \"from\": \"drag\", \"to\": \"fall\", \"setFacing\": \"RANDOM\" }\r\n      ]\r\n    },",
            1,
        );
    fs::write(&animation_path, swapped).unwrap();

    let bundle = MascotBundle::load(&dir).unwrap();

    let drag = bundle.animation.animations.iter().find(|a| a.key == "drag").unwrap();
    assert_eq!(drag.event_transitions.len(), 1);

    let merged = bundle
        .animation
        .events
        .iter()
        .find(|e| e.event == EngineEventKind::DragEnd && e.from.as_deref() == Some("drag"));
    assert!(merged.is_some(), "inline eventTransitions on 'drag' must be merged into top-level events");
    assert_eq!(merged.unwrap().to.as_deref(), Some("fall"));
}
