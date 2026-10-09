use super::package::AssetReader;
use crate::scene::{InspectionError, PageSummary, TextureSummary};

pub(crate) fn inspect(reader: &mut AssetReader) -> Result<TextureSummary, InspectionError> {
    let mut signatures = vec![reader.signature()?, reader.signature()?];
    if signatures != ["TEXV0005", "TEXI0001"] {
        return Err(reader.invalid("unsupported TEX header signatures"));
    }
    let format = reader.u32()?;
    if !matches!(format, 0 | 4 | 9) {
        return Err(reader.invalid(format!("unsupported TEX pixel format {format}")));
    }
    let flags = reader.u32()?;
    if flags & !7 != 0 {
        return Err(reader.invalid(format!("unsupported TEX flags {flags}")));
    }
    let extent = dimensions(reader)?;
    dimensions(reader)?;
    reader.u32()?; // Authored average color, unrelated to storage lengths.
    let container = reader.signature()?;
    let version = match container.as_str() {
        "TEXB0001" => 1,
        "TEXB0002" => 2,
        "TEXB0003" => 3,
        "TEXB0004" => 4,
        _ => return Err(reader.invalid(format!("unsupported image container {container}"))),
    };
    signatures.push(container);
    let page_count = reader.count(256, "texture pages")?;
    if page_count == 0 {
        return Err(reader.invalid("texture has no pages"));
    }
    let codec = if version >= 3 {
        reader.u32()?
    } else {
        u32::MAX
    };
    if !matches!(codec, u32::MAX | 13) {
        return Err(reader.invalid(format!("unsupported TEX image codec {codec}")));
    }
    if codec == 13 && format != 0 {
        return Err(reader.invalid("PNG payload requires RGBA format"));
    }
    if version == 4 && reader.u32()? != 0 {
        return Err(reader.invalid("unsupported embedded TEX video"));
    }
    let mut pages = Vec::new();
    for page in 0..page_count {
        let mips = reader.count(16, "mip levels")?;
        if mips == 0 {
            return Err(reader.invalid("texture page has no mip levels"));
        }
        let mut previous: Option<[u32; 2]> = None;
        let mut decoded_mip_bytes = 0;
        for level in 0..mips {
            let mip_extent = dimensions(reader)?;
            if let Some([width, height]) = previous
                && mip_extent != [(width / 2).max(1), (height / 2).max(1)]
            {
                return Err(reader.invalid("invalid mip dimensions"));
            }
            if page == 0 && level == 0 && mip_extent != extent {
                return Err(reader.invalid("first page differs from header extent"));
            }
            previous = Some(mip_extent);
            let (compression, declared) = if version >= 2 {
                (reader.u32()?, u64::from(reader.u32()?))
            } else {
                (0, 0)
            };
            if compression > 1 {
                return Err(reader.invalid("unsupported mip compression"));
            }
            let stored = u64::from(reader.u32()?);
            let decoded = layout_bytes(format, mip_extent);
            decoded_mip_bytes += decoded;
            if stored == 0 {
                return Err(reader.invalid("empty mip payload"));
            }
            if codec == u32::MAX
                && ((compression == 1 && declared != decoded)
                    || (compression == 0 && stored != decoded))
            {
                return Err(
                    reader.invalid("mip decoded length disagrees with pixel format and dimensions")
                );
            }
            reader.skip(stored)?;
            if level == 0 {
                pages.push(PageSummary {
                    extent: mip_extent,
                    mips,
                    stored_bytes: stored,
                    decoded_bytes: decoded,
                    decoded_mip_bytes: 0,
                });
            }
        }
        pages
            .last_mut()
            .expect("a nonempty mip page was recorded")
            .decoded_mip_bytes = decoded_mip_bytes;
    }
    let mut duration_seconds = 0.;
    let mut frames = 0;
    if flags & 4 != 0 {
        let sprite = reader.signature()?;
        if sprite != "TEXS0003" {
            return Err(reader.invalid(format!("unsupported sprite container {sprite}")));
        }
        signatures.push(sprite);
        let count = reader.count(65_536, "sprite frames")?;
        if count == 0 {
            return Err(reader.invalid("sprite has no frames"));
        }
        dimensions(reader)?;
        frames = count as usize;
        for frame in 0..count {
            let page = reader.u32()? as usize;
            let duration = reader.f32()?;
            let x = reader.f32()?;
            let y = reader.f32()?;
            let wx = reader.f32()?;
            let wy = reader.f32()?;
            let hx = reader.f32()?;
            let hy = reader.f32()?;
            let Some(page) = pages.get(page) else {
                return Err(reader.invalid(format!("frame {frame} references an invalid page")));
            };
            if !duration.is_finite() || duration <= 0. || duration > 60. {
                return Err(reader.invalid(format!("frame {frame} has invalid duration")));
            }
            if [x, y, wx, wy, hx, hy]
                .iter()
                .any(|value| !value.is_finite())
                || wx * hy - wy * hx == 0.
            {
                return Err(reader.invalid(format!("frame {frame} has an invalid rectangle basis")));
            }
            for (px, py) in [
                (x, y),
                (x + wx, y + wy),
                (x + hx, y + hy),
                (x + wx + hx, y + wy + hy),
            ] {
                if px < 0.
                    || py < 0.
                    || px > page.extent[0] as f32 + 0.001
                    || py > page.extent[1] as f32 + 0.001
                {
                    return Err(reader.invalid(format!("frame {frame} extends beyond its page")));
                }
            }
            duration_seconds += f64::from(duration);
        }
    }
    if reader.position != reader.length {
        return Err(reader.invalid("unexpected trailing TEX data"));
    }
    Ok(TextureSummary {
        path: reader.asset.clone(),
        format,
        extent,
        signatures,
        pages,
        frames,
        duration_seconds,
    })
}

fn dimensions(reader: &mut AssetReader) -> Result<[u32; 2], InspectionError> {
    let size = [reader.u32()?, reader.u32()?];
    if size.iter().any(|&value| value == 0 || value > 16_384) {
        return Err(reader.invalid("texture dimensions must be between 1 and 16384"));
    }
    Ok(size)
}

fn layout_bytes(format: u32, [width, height]: [u32; 2]) -> u64 {
    let (width, height) = (u64::from(width), u64::from(height));
    match format {
        0 => width * height * 4,
        4 => width.div_ceil(4) * height.div_ceil(4) * 16,
        9 => width * height,
        _ => unreachable!("format checked before layout"),
    }
}
