pub mod detect;
pub mod mapping;
pub mod physics;
pub mod xml;

use crate::format::animation::AnimationSchema;
use crate::format::manifest::{AnimationSchemaRef, AuthorInfo, LicenseInfo, Manifest, PreviewInfo, SpriteSheetInfo};
use mapping::SpriteFile;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ShimejiEeError {
    #[error("invalid shimeji-ee XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("required action '{0}' is missing or could not be translated")]
    MissingRequiredAction(String),
    #[error("could not locate a usable img/<character> sprite folder for this actions.xml")]
    NoSprites,
    #[error("could not write synthesized bundle: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ShimejiEeImport {
    manifest: Manifest,
    animation: AnimationSchema,
    sprites: Vec<SpriteFile>,
    pub skipped: Vec<String>,
}

pub fn try_import(extracted_root: &Path) -> Result<Option<ShimejiEeImport>, ShimejiEeError> {
    let Some(detected) = detect::detect(extracted_root)? else { return Ok(None) };

    let actions = xml::parse_actions(&std::fs::read_to_string(&detected.actions_xml)?)?;
    let behaviors = match &detected.behaviors_xml {
        Some(path) => xml::parse_behaviors(&std::fs::read_to_string(path)?)?,
        None => Vec::new(),
    };

    let character_name = detected
        .img_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("mascot")
        .to_string();

    let (animation, sprites, skipped) = mapping::map_to_schema(&actions, &behaviors, &detected.img_dir)?;

    let manifest = Manifest {
        schema_version: 1,
        name: character_name.clone(),
        name_slug: slugify(&character_name),
        category: "imported".to_string(),
        category_slug: "imported".to_string(),
        description: String::new(),
        bundle_version: 1,
        min_app_version: 1,
        levels: 1,
        origin: "SHIMEJI_EE_IMPORT".to_string(),
        animation_schema: AnimationSchemaRef {
            path: "animation.json".to_string(),
            schema_id: animation.schema_id.clone(),
            version: animation.version,
        },
        sprites: SpriteSheetInfo {
            kind: "SEQUENCE".to_string(),
            base_path: "sprites/".to_string(),
            file_pattern: "%04d.png".to_string(),
            sprite_count: sprites.len() as u32,
            size: [128, 128],
        },
        preview: PreviewInfo { thumbnail: String::new() },
        author: AuthorInfo { name: "Unknown".to_string() },
        license: LicenseInfo { kind: "UNKNOWN".to_string(), text: String::new(), attribution: String::new() },
    };

    Ok(Some(ShimejiEeImport { manifest, animation, sprites, skipped }))
}

fn slugify(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
}

impl ShimejiEeImport {
    pub fn write_into(&self, dir: &Path) -> Result<(), ShimejiEeError> {
        std::fs::write(dir.join("manifest.json"), serde_json::to_string_pretty(&self.manifest).expect("Manifest always serializes"))?;
        std::fs::write(dir.join("animation.json"), serde_json::to_string_pretty(&self.animation).expect("AnimationSchema always serializes"))?;

        let sprites_dir = dir.join("sprites");
        std::fs::create_dir_all(&sprites_dir)?;
        for sprite in &self.sprites {
            std::fs::copy(&sprite.source, sprites_dir.join(format!("{:04}.png", sprite.index)))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::animation::EngineEventKind;
    use crate::format::bundle::MascotBundle;
    use std::fs;

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

    fn write_placeholder_png(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::new(4, 4).save(path).unwrap();
    }

    fn build_fixture_source() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        copy_dir_recursive(Path::new("tests/fixtures/fixture_shimeji_ee/conf"), &tmp.path().join("conf"));
        for name in ["stand.png", "walk1.png", "walk2.png", "falling.png", "pinched_left.png", "pinched_right.png", "ieaction.png"] {
            write_placeholder_png(&tmp.path().join("img/TestMascot").join(name));
        }
        tmp
    }

    #[test]
    fn try_import_translates_and_loads_the_fixture_mascot() {
        let src = build_fixture_source();

        let import = try_import(src.path()).unwrap().expect("fixture must be detected as shimeji-ee");
        assert!(import.skipped.iter().any(|s| s == "Pinched"));
        assert!(import.skipped.iter().any(|s| s == "WalkWithIe"));
        assert!(import.skipped.iter().any(|s| s == "FallFromWall"));

        let dest = tempfile::tempdir().unwrap();
        import.write_into(dest.path()).unwrap();

        let bundle = MascotBundle::load(dest.path()).unwrap();
        assert_eq!(bundle.animation.default_animation, "Falling");
        assert!(bundle.animation.events.iter().any(|e| e.event == EngineEventKind::DragStart && e.to.as_deref() == Some("Pinched")));
        assert!(bundle.animation.events.iter().any(|e| e.event == EngineEventKind::DragEnd && e.to.as_deref() == Some("Falling")));
    }

    #[test]
    fn try_import_returns_none_for_a_non_shimeji_ee_directory() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("manifest.json"), b"{}").unwrap();
        assert!(try_import(tmp.path()).unwrap().is_none());
    }
}
