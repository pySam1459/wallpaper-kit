use super::package::{AssetReader, Package};
use crate::scene::{InspectionError, PageSummary, SceneSource, TextureSummary};

/// Storage delivered to the GPU after container decompression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    /// Tightly packed, top-to-bottom RGBA8 code values; no color conversion.
    Rgba8,
    /// BC3 blocks, preserved compressed for GPU sampling.
    Bc3,
    /// One unsigned normalized byte per pixel.
    R8,
}

/// One validated base-mip TEX page, independent of a graphics device.
///
/// Owns at most 256 MiB of pixels or BC3 blocks. This is not a sprite frame;
/// authored atlas coordinates, durations and material semantics are separate.
#[derive(Debug)]
pub struct TexturePage {
    extent: [u32; 2],
    format: TextureFormat,
    data: Vec<u8>,
}

impl TexturePage {
    /// Width and height of the page in pixels.
    pub fn extent(&self) -> [u32; 2] {
        self.extent
    }

    /// Checked storage layout, without inferred scene color/alpha semantics.
    pub fn format(&self) -> TextureFormat {
        self.format
    }

    /// Packed rows for RGBA8/R8, or row-major 4 × 4 BC3 blocks.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

struct PageLocation {
    offset: u64,
    stored: u64,
    declared: u64,
    compressed: bool,
}

struct Index {
    summary: TextureSummary,
    locations: Vec<PageLocation>,
    png: bool,
}

const MAX_PAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENCODED_BYTES: u64 = 64 * 1024 * 1024;

/// Read-only TEX decoder session retaining one checked package index.
///
/// Indexing, hashing, I/O and decoding block. Use a tool or asset worker,
/// never the host UI thread. A session holds the package open read-only,
/// excluding writes/replacement, and hashes it once. Decoded pages are owned
/// by the caller; concurrent calls and retained pages have no global budget.
pub struct TextureDecoder {
    package: Package,
}

impl TextureDecoder {
    /// Open and validate the package/root, then stream its identity hash once.
    pub fn open(source: &SceneSource) -> Result<Self, InspectionError> {
        Ok(Self {
            package: Package::open(source)?,
        })
    }

    /// SHA-256 of the read-locked package, computed by [`Self::open`].
    pub fn package_sha256(&self) -> &str {
        &self.package.summary.sha256
    }

    /// Decode a base mip after validating the entire TEX metadata structure.
    ///
    /// Package-local assets take precedence over the installed engine root.
    /// Raw/LZ4 RGBA8/R8/BC3 and PNG are supported. Other pages/mips are skipped,
    /// not allocated. Malformed payloads, missing assets/pages and allocation
    /// limits return an asset-named error. Encoded payloads are limited to
    /// 64 MiB and decoded storage to 256 MiB. PNG conversion preserves code
    /// values and alpha, without applying gamma or color profiles.
    pub fn decode_page(&self, asset: &str, page: u32) -> Result<TexturePage, InspectionError> {
        let mut reader = self
            .package
            .reader(asset)?
            .ok_or_else(|| InspectionError::invalid(asset, "texture asset is missing"))?;
        let index = index(&mut reader)?;
        let summary = index
            .summary
            .pages
            .get(page as usize)
            .ok_or_else(|| reader.invalid("texture page index is out of range"))?;
        let location = &index.locations[page as usize];
        if summary.decoded_bytes > MAX_PAGE_BYTES {
            return Err(reader.invalid("decoded page exceeds 256 MiB"));
        }
        if location.compressed {
            let limit = if index.png {
                MAX_ENCODED_BYTES
            } else {
                MAX_PAGE_BYTES
            };
            if location.declared == 0 || location.declared > limit {
                return Err(reader.invalid("LZ4 decoded length exceeds allocation limit"));
            }
        }
        let encoded = index.png || location.compressed;
        reader.seek_to(location.offset)?;
        let payload = reader.bytes(
            location.stored,
            if encoded {
                MAX_ENCODED_BYTES
            } else {
                MAX_PAGE_BYTES
            },
        )?;
        let payload = if location.compressed {
            let mut decoded = vec![0; location.declared as usize];
            let written = lz4_flex::block::decompress_into(&payload, &mut decoded)
                .map_err(|error| reader.invalid(format!("invalid LZ4 payload: {error}")))?;
            if written != decoded.len() {
                return Err(reader.invalid("LZ4 decoded length disagrees with declaration"));
            }
            decoded
        } else {
            payload
        };
        let data = if index.png {
            decode_png(&reader, &payload, summary.extent)?
        } else {
            payload
        };
        if data.len() as u64 != summary.decoded_bytes {
            return Err(reader.invalid("decoded payload disagrees with page layout"));
        }
        Ok(TexturePage {
            extent: summary.extent,
            format: match index.summary.format {
                0 => TextureFormat::Rgba8,
                4 => TextureFormat::Bc3,
                9 => TextureFormat::R8,
                _ => unreachable!("index validated the pixel format"),
            },
            data,
        })
    }
}

fn decode_png(
    reader: &AssetReader,
    payload: &[u8],
    extent: [u32; 2],
) -> Result<Vec<u8>, InspectionError> {
    let failure =
        |error: png::DecodingError| reader.invalid(format!("invalid PNG payload: {error}"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(payload));
    decoder.set_limits(png::Limits {
        bytes: MAX_ENCODED_BYTES as usize,
    });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut image = decoder.read_info().map_err(failure)?;
    if [image.info().width, image.info().height] != extent
        || image.info().animation_control.is_some()
    {
        return Err(reader.invalid("PNG dimensions or animation disagree with TEX page"));
    }
    let length = image
        .output_buffer_size()
        .ok_or_else(|| reader.invalid("PNG output size overflow"))?;
    if length as u64 > layout_bytes(0, extent) {
        return Err(reader.invalid("PNG output exceeds page layout"));
    }
    let mut pixels = vec![0; length];
    let info = image.next_frame(&mut pixels).map_err(failure)?;
    image.finish().map_err(failure)?;
    pixels.truncate(info.buffer_size());
    if info.bit_depth != png::BitDepth::Eight {
        return Err(reader.invalid("unsupported PNG output depth"));
    }
    if info.color_type == png::ColorType::Rgba {
        return Ok(pixels);
    }
    let mut rgba = Vec::with_capacity(layout_bytes(0, extent) as usize);
    for pixel in pixels.chunks_exact(info.color_type.samples()) {
        let value = match info.color_type {
            png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
            png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
            png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
            _ => return Err(reader.invalid("unsupported PNG output color type")),
        };
        rgba.extend_from_slice(&value);
    }
    Ok(rgba)
}

pub(crate) fn inspect(reader: &mut AssetReader) -> Result<TextureSummary, InspectionError> {
    Ok(index(reader)?.summary)
}

fn index(reader: &mut AssetReader) -> Result<Index, InspectionError> {
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
    let mut locations = Vec::new();
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
            if level == 0 {
                locations.push(PageLocation {
                    offset: reader.position,
                    stored,
                    declared,
                    compressed: compression == 1,
                });
                pages.push(PageSummary {
                    extent: mip_extent,
                    mips,
                    stored_bytes: stored,
                    decoded_bytes: decoded,
                    decoded_mip_bytes: 0,
                });
            }
            reader.skip(stored)?;
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
    Ok(Index {
        summary: TextureSummary {
            path: reader.asset.clone(),
            format,
            extent,
            signatures,
            pages,
            frames,
            duration_seconds,
        },
        locations,
        png: codec == 13,
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
