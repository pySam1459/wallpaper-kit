use wallpaper_kit::Pixels;

// Independent pixel oracle, expressed from the documented fixture contract.
// It never calls the renderer's rectangle calculation or trusts frame metadata.
pub fn check_pixels(pixels: &Pixels, expected_ms: u64) -> Result<(), String> {
    let w = pixels.size.width() as usize;
    let h = pixels.size.height() as usize;
    if pixels.rgba.len() != w * h * 4 {
        return Err("incorrect pixel buffer length".into());
    }
    let bar_width = (w / 8).max(1);
    let phase = (expected_ms % 2_000) / 50;
    let x0 = ((w - bar_width) as u64 * phase / 39) as usize;
    let y0 = h / 4;
    let y1 = (h - y0).max(y0 + 1);
    for y in 0..h {
        for x in 0..w {
            let expected = if (x0..x0 + bar_width).contains(&x) && (y0..y1).contains(&y) {
                [224, 160, 64, 255]
            } else {
                [16, 24, 32, 255]
            };
            let offset = (y * w + x) * 4;
            if pixels.rgba[offset..offset + 4] != expected {
                return Err(format!("motion differs at ({x},{y}) for {expected_ms} ms"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wallpaper_kit::Size;

    #[test]
    fn literal_half_loop_reference_and_corruption() {
        // At 1,000 ms on a 320x180 image, the contract puts a 40px bar at x=143.
        let mut pixels = Pixels {
            size: Size::new(320, 180).unwrap(),
            rgba: [16, 24, 32, 255].repeat(320 * 180),
        };
        for y in 45..135 {
            for x in 143..183 {
                let offset = (y * 320 + x) * 4;
                pixels.rgba[offset..offset + 4].copy_from_slice(&[224, 160, 64, 255]);
            }
        }
        assert!(check_pixels(&pixels, 1_000).is_ok());
        assert!(check_pixels(&pixels, 500).is_err());
        pixels.rgba[0] = 0;
        assert!(check_pixels(&pixels, 1_000).is_err());
    }
}
