use std::path::Path;

pub fn sprite_filename(pattern: &str, index: u32) -> String {
    let pct = pattern.find('%').expect("sprite file pattern must contain '%'");
    let d = pattern[pct..]
        .find('d')
        .map(|i| pct + i)
        .expect("sprite file pattern must contain 'd' after '%'");
    let width: usize = pattern[pct + 1..d].parse().unwrap_or(0);
    format!("{}{:0width$}{}", &pattern[..pct], index, &pattern[d + 1..], width = width)
}

pub fn decode_sprite(path: &Path) -> Result<image::RgbaImage, image::ImageError> {
    Ok(image::open(path)?.into_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn formats_sprite_filenames() {
        assert_eq!(sprite_filename("%04d.webp", 0), "0000.webp");
        assert_eq!(sprite_filename("%04d.webp", 7), "0007.webp");
        assert_eq!(sprite_filename("%04d.webp", 69), "0069.webp");
    }

    #[test]
    fn decodes_a_sample_sprite() {
        let path = Path::new("tests/fixtures/fixture_bundle/sprites/0000.webp");
        let img = decode_sprite(path).unwrap();
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
        assert_eq!(img.as_raw().len(), 16 * 16 * 4);
    }
}
