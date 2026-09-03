use image::RgbaImage;

const OPAQUE_THRESHOLD: u8 = 8;

pub fn is_opaque_at(frame: &RgbaImage, local_x: i32, local_y: i32) -> bool {
    if local_x < 0 || local_y < 0 || local_x as u32 >= frame.width() || local_y as u32 >= frame.height() {
        return false;
    }
    frame.get_pixel(local_x as u32, local_y as u32).0[3] > OPAQUE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn frame_with_one_opaque_pixel() -> RgbaImage {
        let mut img = RgbaImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        img.put_pixel(2, 2, Rgba([255, 0, 0, 255]));
        img
    }

    #[test]
    fn opaque_pixel_hits() {
        let img = frame_with_one_opaque_pixel();
        assert!(is_opaque_at(&img, 2, 2));
    }

    #[test]
    fn transparent_pixel_passes_through() {
        let img = frame_with_one_opaque_pixel();
        assert!(!is_opaque_at(&img, 0, 0));
    }

    #[test]
    fn out_of_bounds_passes_through() {
        let img = frame_with_one_opaque_pixel();
        assert!(!is_opaque_at(&img, -1, 0));
        assert!(!is_opaque_at(&img, 100, 0));
    }
}
