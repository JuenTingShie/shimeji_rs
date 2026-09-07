use super::animation::Direction;
use image::RgbaImage;
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

pub fn decode_sprite(path: &Path) -> Result<RgbaImage, image::ImageError> {
    Ok(image::open(path)?.into_rgba8())
}

/// Bundle art is authored once per pose and shared between its `_left`/`_right` animation
/// variants (same sprite indices on both) — matching shimeji-ee's own convention where only the
/// left-facing image is drawn and the right-facing one is a horizontal mirror of it, applied at
/// render time based on which way the mascot is currently facing.
pub fn oriented_sprite(frame: RgbaImage, facing: Direction) -> RgbaImage {
    match facing {
        Direction::Right => image::imageops::flip_horizontal(&frame),
        Direction::Left | Direction::Any => frame,
    }
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

    fn asymmetric_frame() -> RgbaImage {
        // 2x1 image: left pixel red, right pixel blue -- flipping must swap them.
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 0, 255, 255]));
        img
    }

    #[test]
    fn left_facing_sprite_is_unchanged() {
        let oriented = oriented_sprite(asymmetric_frame(), Direction::Left);
        assert_eq!(*oriented.get_pixel(0, 0), image::Rgba([255, 0, 0, 255]));
        assert_eq!(*oriented.get_pixel(1, 0), image::Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn any_facing_sprite_is_unchanged() {
        let oriented = oriented_sprite(asymmetric_frame(), Direction::Any);
        assert_eq!(*oriented.get_pixel(0, 0), image::Rgba([255, 0, 0, 255]));
        assert_eq!(*oriented.get_pixel(1, 0), image::Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn right_facing_sprite_is_mirrored_horizontally() {
        let oriented = oriented_sprite(asymmetric_frame(), Direction::Right);
        assert_eq!(*oriented.get_pixel(0, 0), image::Rgba([0, 0, 255, 255]));
        assert_eq!(*oriented.get_pixel(1, 0), image::Rgba([255, 0, 0, 255]));
    }
}
