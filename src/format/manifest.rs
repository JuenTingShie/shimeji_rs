use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub name: String,
    pub name_slug: String,
    pub category: String,
    pub category_slug: String,
    #[serde(default)]
    pub description: String,
    pub bundle_version: u32,
    pub min_app_version: u32,
    pub levels: u8,
    pub origin: String,
    pub animation_schema: AnimationSchemaRef,
    pub sprites: SpriteSheetInfo,
    pub preview: PreviewInfo,
    pub author: AuthorInfo,
    pub license: LicenseInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationSchemaRef {
    pub path: String,
    pub schema_id: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteSheetInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub base_path: String,
    pub file_pattern: String,
    pub sprite_count: u32,
    pub size: [u32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewInfo {
    pub thumbnail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub attribution: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_manifest() {
        let json = std::fs::read_to_string("tests/fixtures/fixture_manifest.json").unwrap();
        let manifest: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.name, "sample_mascot");
        assert_eq!(manifest.name_slug, "sample_mascot");
        assert_eq!(manifest.levels, 4);
        assert_eq!(manifest.animation_schema.schema_id, "legacy_default_v1");
        assert_eq!(manifest.sprites.sprite_count, 13);
        assert_eq!(manifest.sprites.size, [16, 16]);
        assert_eq!(manifest.sprites.base_path, "sprites/");
        assert_eq!(manifest.sprites.file_pattern, "%04d.webp");
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let json = std::fs::read_to_string("tests/fixtures/fixture_manifest.json").unwrap();
        let manifest: Manifest = serde_json::from_str(&json).unwrap();
        let re_encoded = serde_json::to_string(&manifest).unwrap();
        let round_tripped: Manifest = serde_json::from_str(&re_encoded).unwrap();
        assert_eq!(round_tripped.name, manifest.name);
        assert_eq!(round_tripped.sprites.sprite_count, manifest.sprites.sprite_count);
    }
}
