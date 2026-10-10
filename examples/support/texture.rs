//! Generated TEX/PKG fixtures; no third-party artwork.

pub fn tex(format: u32, extent: [u32; 2], data: &[u8], lz4: bool, png: bool) -> Vec<u8> {
    let mut bytes = b"TEXV0005\0TEXI0001\0".to_vec();
    for value in [format, 0, extent[0], extent[1], extent[0], extent[1], 0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(b"TEXB0004\0");
    for value in [
        1,
        if png { 13 } else { u32::MAX },
        0,
        1,
        extent[0],
        extent[1],
        u32::from(lz4),
        data.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let payload = if lz4 {
        lz4_flex::block::compress(data)
    } else {
        data.to_vec()
    };
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

pub fn package(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(b"PKGV0024");
    bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    let mut offset = 0_u32;
    for (name, data) in entries {
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in entries {
        bytes.extend_from_slice(data);
    }
    bytes
}

pub fn sprite(mut texture: Vec<u8>, frames: &[(u32, f32, [f32; 6])]) -> Vec<u8> {
    texture[22..26].copy_from_slice(&4_u32.to_le_bytes());
    texture.extend_from_slice(b"TEXS0003\0");
    texture.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    for dimension in [2_u32, 2] {
        texture.extend_from_slice(&dimension.to_le_bytes());
    }
    for (page, duration, rectangle) in frames {
        texture.extend_from_slice(&page.to_le_bytes());
        texture.extend_from_slice(&duration.to_le_bytes());
        for coordinate in rectangle {
            texture.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    texture
}

// The oracle copies CPU rows independently of the shader's integer addressing.
pub fn crop(rgba: &[u8], page: [u32; 2], origin: [u32; 2], extent: [u32; 2]) -> Vec<u8> {
    let mut result = Vec::new();
    for y in origin[1]..origin[1] + extent[1] {
        let start = (y as usize * page[0] as usize + origin[0] as usize) * 4;
        result.extend_from_slice(&rgba[start..start + extent[0] as usize * 4]);
    }
    result
}

pub fn png(extent: [u32; 2], color: png::ColorType, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, extent[0], extent[1]);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(data)
        .unwrap();
    bytes
}

pub fn rgba() -> Vec<u8> {
    // Four distinct corners, including hidden RGB under zero alpha.
    [
        [255, 0, 0, 255],
        [0, 255, 0, 128],
        [0, 0, 255, 0],
        [240, 160, 80, 255],
    ]
    .concat()
}

pub fn bc3() -> Vec<u8> {
    // Four 4x4 blocks: red/green/blue/white with distinct alpha endpoints.
    let mut bytes = Vec::new();
    for (color, alpha) in [(0xf800_u16, 255), (0x07e0, 128), (0x001f, 0), (0xffff, 255)] {
        bytes.extend_from_slice(&[alpha, 0, 0, 0, 0, 0, 0, 0]);
        bytes.extend_from_slice(&color.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
    }
    bytes
}

pub struct Case {
    pub name: &'static str,
    pub tex: Vec<u8>,
    pub extent: [u32; 2],
    pub rgba: Vec<u8>,
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (name, compressed, encoded) in [
        ("rgba-raw", false, false),
        ("rgba-lz4", true, false),
        ("rgba-png", false, true),
        ("rgba-png-lz4", true, true),
    ] {
        let rgba = rgba();
        let payload = if encoded {
            png([2, 2], png::ColorType::Rgba, &rgba)
        } else {
            rgba.clone()
        };
        cases.push(Case {
            name,
            tex: tex(0, [2, 2], &payload, compressed, encoded),
            extent: [2, 2],
            rgba,
        });
    }
    let mask = [0, 64, 128, 192, 224, 255];
    for (name, compressed) in [("r8-raw", false), ("r8-lz4", true)] {
        cases.push(Case {
            name,
            tex: tex(9, [3, 2], &mask, compressed, false),
            extent: [3, 2],
            rgba: mask
                .iter()
                .flat_map(|&value| [value, value, value, 255])
                .collect(),
        });
    }
    let mut rgba = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            rgba.extend_from_slice(
                &[
                    [255, 0, 0, 255],
                    [0, 255, 0, 128],
                    [0, 0, 255, 0],
                    [255, 255, 255, 255],
                ][(y / 4) * 2 + x / 4],
            );
        }
    }
    for (name, compressed) in [("bc3-raw", false), ("bc3-lz4", true)] {
        cases.push(Case {
            name,
            tex: tex(4, [8, 8], &bc3(), compressed, false),
            extent: [8, 8],
            rgba: rgba.clone(),
        });
    }
    cases
        .into_iter()
        .map(|mut case| {
            let [width, height] = case.extent;
            case.tex = sprite(
                case.tex,
                &[
                    (0, 0.06, [1., 0., (width - 1) as f32, 0., 0., height as f32]),
                    (0, 0.12, [0., (height - 1) as f32, width as f32, 0., 0., 1.]),
                ],
            );
            case
        })
        .collect()
}

pub fn check(pixels: &wallpaper_kit::Pixels, extent: [u32; 2], rgba: &[u8]) -> Result<(), String> {
    let width = pixels.size.width();
    let height = pixels.size.height();
    for y in 0..height {
        for x in 0..width {
            let sx =
                ((u64::from(x) * 2 + 1) * u64::from(extent[0]) / (u64::from(width) * 2)) as usize;
            let sy =
                ((u64::from(y) * 2 + 1) * u64::from(extent[1]) / (u64::from(height) * 2)) as usize;
            let source = &rgba[(sy * extent[0] as usize + sx) * 4..][..4];
            let actual = &pixels.rgba[(y as usize * width as usize + x as usize) * 4..][..4];
            for channel in 0..4 {
                let expected = if channel == 3 {
                    255
                } else {
                    let background = [16, 24, 32][channel];
                    ((u32::from(source[channel]) * u32::from(source[3])
                        + background * (255 - u32::from(source[3]))
                        + 127)
                        / 255) as u8
                };
                let tolerance = u8::from(channel != 3);
                if actual[channel].abs_diff(expected) > tolerance {
                    return Err(format!(
                        "texture differs at ({x},{y}) channel {channel}: expected {expected}, got {}",
                        actual[channel]
                    ));
                }
            }
        }
    }
    Ok(())
}
