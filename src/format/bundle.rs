use super::animation::AnimationSchema;
use super::manifest::Manifest;
use super::sprites::sprite_filename;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("could not read {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("invalid manifest.json: {0}")]
    ManifestParse(#[source] serde_json::Error),
    #[error("invalid animation.json: {0}")]
    AnimationParse(#[source] serde_json::Error),
    #[error("unsupported manifest schemaVersion {found} (expected 1)")]
    UnsupportedSchemaVersion { found: u32 },
    #[error("unsupported animation schemaId '{found}' (expected legacy_default_v1 or pc_import_v1)")]
    UnsupportedAnimationSchema { found: String },
    #[error("manifest declares {declared} sprites but {found} were found under {base_path}")]
    SpriteCountMismatch { declared: u32, found: usize, base_path: PathBuf },
    #[error("animation '{animation}' frame references sprite index {index}, but only {count} sprites exist")]
    SpriteIndexOutOfRange { animation: String, index: u32, count: u32 },
    #[error("{context} references unknown animation key '{key}'")]
    UnknownAnimationKey { context: String, key: String },
}

const SUPPORTED_ANIMATION_SCHEMAS: &[&str] = &["legacy_default_v1", "pc_import_v1"];

#[derive(Debug)]
pub struct MascotBundle {
    pub manifest: Manifest,
    pub animation: AnimationSchema,
    pub base_path: PathBuf,
}

impl MascotBundle {
    pub fn load(dir: &Path) -> Result<MascotBundle, BundleError> {
        let manifest_path = dir.join("manifest.json");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .map_err(|source| BundleError::Io { path: manifest_path.clone(), source })?;
        let manifest: Manifest =
            serde_json::from_str(&manifest_text).map_err(BundleError::ManifestParse)?;

        if manifest.schema_version != 1 {
            return Err(BundleError::UnsupportedSchemaVersion { found: manifest.schema_version });
        }

        let animation_path = dir.join(&manifest.animation_schema.path);
        let animation_text = std::fs::read_to_string(&animation_path)
            .map_err(|source| BundleError::Io { path: animation_path.clone(), source })?;
        let animation: AnimationSchema =
            serde_json::from_str(&animation_text).map_err(BundleError::AnimationParse)?;

        if !SUPPORTED_ANIMATION_SCHEMAS.contains(&animation.schema_id.as_str()) {
            return Err(BundleError::UnsupportedAnimationSchema { found: animation.schema_id.clone() });
        }

        let sprites_dir = dir.join(&manifest.sprites.base_path);
        let found = (0..manifest.sprites.sprite_count)
            .filter(|i| sprites_dir.join(sprite_filename(&manifest.sprites.file_pattern, *i)).is_file())
            .count();
        if found != manifest.sprites.sprite_count as usize {
            return Err(BundleError::SpriteCountMismatch {
                declared: manifest.sprites.sprite_count,
                found,
                base_path: sprites_dir,
            });
        }

        for anim in &animation.animations {
            for frame in &anim.frames {
                if frame.sprite >= manifest.sprites.sprite_count {
                    return Err(BundleError::SpriteIndexOutOfRange {
                        animation: anim.key.clone(),
                        index: frame.sprite,
                        count: manifest.sprites.sprite_count,
                    });
                }
            }
        }

        let known_keys: HashSet<&str> = animation.animations.iter().map(|a| a.key.as_str()).collect();
        let check = |context: &str, key: &str| -> Result<(), BundleError> {
            if known_keys.contains(key) {
                Ok(())
            } else {
                Err(BundleError::UnknownAnimationKey { context: context.to_string(), key: key.to_string() })
            }
        };

        check("default_animation", &animation.default_animation)?;
        for key in &animation.initial_candidates {
            check("initial_candidates", key)?;
        }
        for anim in &animation.animations {
            if let Some(auto) = &anim.auto {
                for choice in &auto.on_finish {
                    check(&format!("{}.auto.onFinish", anim.key), &choice.to)?;
                }
                for rule in &auto.on_timer {
                    for choice in &rule.choices {
                        check(&format!("{}.auto.onTimer", anim.key), &choice.to)?;
                    }
                }
            }
            for bt in &anim.border_transitions {
                for choice in &bt.choices {
                    check(&format!("{}.borderTransitions", anim.key), &choice.to)?;
                }
            }
        }
        for event in &animation.events {
            if let Some(to) = &event.to {
                check(&format!("events[{:?}]", event.event), to)?;
            }
            if let Some(choices) = &event.choices {
                for choice in choices {
                    check(&format!("events[{:?}]", event.event), &choice.to)?;
                }
            }
        }

        Ok(MascotBundle { manifest, animation, base_path: dir.to_path_buf() })
    }
}
