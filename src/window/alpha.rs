use image::RgbaImage;

/// Converts RGBA to premultiplied BGRA, the byte layout `UpdateLayeredWindow`
/// requires for a 32bpp top-down DIB with per-pixel alpha.
pub fn premultiply(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.as_raw().len());
    for px in img.pixels() {
        let [r, g, b, a] = px.0;
        let scale = a as u32;
        out.push(((b as u32 * scale) / 255) as u8);
        out.push(((g as u32 * scale) / 255) as u8);
        out.push(((r as u32 * scale) / 255) as u8);
        out.push(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn fully_opaque_pixel_becomes_bgra_unchanged() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
        let out = premultiply(&img);
        assert_eq!(out, vec![30, 20, 10, 255]); // B, G, R, A
    }

    #[test]
    fn half_alpha_pixel_scales_color_channels() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([200, 100, 40, 128]));
        let out = premultiply(&img);
        // 200*128/255 ≈ 100, 100*128/255 ≈ 50, 40*128/255 ≈ 20
        assert_eq!(out, vec![20, 50, 100, 128]);
    }

    #[test]
    fn fully_transparent_pixel_becomes_all_zero() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([255, 255, 255, 0]));
        let out = premultiply(&img);
        assert_eq!(out, vec![0, 0, 0, 0]);
    }
}
