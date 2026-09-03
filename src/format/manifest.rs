use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationSchemaRef {
    pub path: String,
    pub schema_id: String,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteSheetInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub base_path: String,
    pub file_pattern: String,
    pub sprite_count: u32,
    pub size: [u32; 2],
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewInfo {
    pub thumbnail: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
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
        let json = std::fs::read_to_string("tests/fixtures/sample_manifest.json").unwrap();
        let manifest: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.name, "usagi");
        assert_eq!(manifest.name_slug, "usagi");
        assert_eq!(manifest.levels, 4);
        assert_eq!(manifest.animation_schema.schema_id, "legacy_default_v1");
        assert_eq!(manifest.sprites.sprite_count, 70);
        assert_eq!(manifest.sprites.size, [512, 512]);
        assert_eq!(manifest.sprites.base_path, "sprites/");
        assert_eq!(manifest.sprites.file_pattern, "%04d.webp");
    }
}
