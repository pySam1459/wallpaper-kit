use super::package::{AssetReader, Package};
use crate::Error;
use crate::scene::{InspectionError, PageSummary, SceneSource, TextureSummary};
use std::{sync::Arc, time::Duration};

mod sprite;
pub use sprite::{SpriteSample, SpriteStats, SpriteStream};

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

/// A checked rectangular view of a page, without copying or decompressing BC3.
#[derive(Debug)]
pub struct TextureRegion {
    page: Arc<TexturePage>,
    origin: [u32; 2],
    extent: [u32; 2],
}

impl TextureRegion {
    /// Select a nonempty, top-left pixel rectangle entirely inside `page`.
    /// BC3 coordinates need not align with blocks; sampling remains on the GPU.
    /// Returns [`Error::InvalidTextureRegion`] for empty/out-of-page rectangles,
    /// including arithmetic overflow. Construction performs no pixel allocation.
    pub fn new(page: Arc<TexturePage>, origin: [u32; 2], extent: [u32; 2]) -> Result<Self, Error> {
        if (0..2).any(|axis| {
            extent[axis] == 0
                || origin[axis]
                    .checked_add(extent[axis])
                    .is_none_or(|end| end > page.extent[axis])
        }) {
            return Err(Error::InvalidTextureRegion);
        }
        Ok(Self {
            page,
            origin,
            extent,
        })
    }

    /// Shared immutable source; regions of the same Arc reuse its GPU upload.
    pub fn page(&self) -> &Arc<TexturePage> {
        &self.page
    }

    /// Top-left source pixel, with rows increasing downward.
    pub fn origin(&self) -> [u32; 2] {
        self.origin
    }

    /// Width and height of the selected rectangle, in source pixels.
    pub fn extent(&self) -> [u32; 2] {
        self.extent
    }
}

/// One authored, axis-aligned TEXS frame rectangle and duration.
///
/// This is metadata only. Decode its page separately, then construct a
/// [`TextureRegion`]. Frame selection does not start a playback timer.
#[derive(Clone, Copy, Debug)]
pub struct SpriteFrame {
    page: u32,
    duration: Duration,
    origin: [u32; 2],
    extent: [u32; 2],
}

impl SpriteFrame {
    /// Zero-based page index for [`TextureDecoder::decode_page`].
    pub fn page_index(self) -> u32 {
        self.page
    }

    /// Authored positive frame duration; not a measured playback interval.
    pub fn duration(self) -> Duration {
        self.duration
    }

    /// Top-left source pixel, with rows increasing downward.
    pub fn origin(self) -> [u32; 2] {
        self.origin
    }

    /// Width and height of the authored source rectangle.
    pub fn extent(self) -> [u32; 2] {
        self.extent
    }
}

struct FrameLocation {
    page: u32,
    duration: f32,
    rectangle: [f32; 6],
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
    frames: Vec<FrameLocation>,
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
    pub(crate) package: Package,
}

impl TextureDecoder {
    /// Open a metadata-indexed, read-locked sprite extraction stream.
    /// Blocks on I/O and validation, but decodes no pixels. The stream owns its
    /// asset handle independently of this decoder and excludes writes/replacement
    /// until dropped. Use its blocking sampling API on an asset worker/tool.
    pub fn sprite_stream(&self, asset: &str) -> Result<SpriteStream, InspectionError> {
        SpriteStream::open(&self.package, asset)
    }
    /// Prepare explicitly selected images as a diagnostic static snapshot.
    /// Performs blocking JSON I/O and serial page decoding on the caller's thread.
    /// Retains authored order, 2D transforms and selected sprite rectangles.
    /// Effects, camera behavior and unselected objects are named in the returned
    /// omissions. Unsupported selected models/materials/transforms fail. This is
    /// not full scene loading, playback or fidelity verification. New unique
    /// page sizes are admitted against 256 MiB before payload decoding.
    pub fn image_snapshot(
        &self,
        objects: &[u64],
        frame_index: u32,
    ) -> Result<crate::ImageSnapshot, InspectionError> {
        crate::scene::snapshot::prepare(self, objects, frame_index)
    }

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

    /// Read the authored frame table in order, without decoding page payloads.
    ///
    /// Performs blocking I/O and full TEX structural validation. Returns an
    /// asset-named error for static textures or rotated, flipped, fractional or
    /// out-of-page rectangles. Only positive integer axis-aligned rectangles
    /// are supported in this component slice. Durations below nanosecond
    /// resolution fail. At most 65,536 records are read.
    pub fn sprite_frames(&self, asset: &str) -> Result<Vec<SpriteFrame>, InspectionError> {
        let mut reader = self
            .package
            .reader(asset)?
            .ok_or_else(|| InspectionError::invalid(asset, "texture asset is missing"))?;
        let index = index(&mut reader)?;
        checked_frames(&reader, &index)
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
        self.decode_page_admitted(asset, page, MAX_PAGE_BYTES)
    }

    pub(crate) fn decode_page_admitted(
        &self,
        asset: &str,
        page: u32,
        remaining: u64,
    ) -> Result<TexturePage, InspectionError> {
        let mut reader = self
            .package
            .reader(asset)?
            .ok_or_else(|| InspectionError::invalid(asset, "texture asset is missing"))?;
        let index = index(&mut reader)?;
        decoded_page(&mut reader, &index, page, remaining)
    }
}

fn checked_frames(
    reader: &AssetReader,
    index: &Index,
) -> Result<Vec<SpriteFrame>, InspectionError> {
    if index.frames.is_empty() {
        return Err(reader.invalid("texture has no sprite frames"));
    }
    let mut frames = Vec::with_capacity(index.frames.len());
    for (number, frame) in index.frames.iter().enumerate() {
        let [x, y, wx, wy, hx, hy] = frame.rectangle;
        if wy != 0.
            || hx != 0.
            || wx <= 0.
            || hy <= 0.
            || [x, y, wx, hy]
                .iter()
                .any(|value| *value < 0. || value.fract() != 0.)
        {
            return Err(reader.invalid(format!(
                "frame {number}: only positive integer axis-aligned rectangles are supported"
            )));
        }
        let origin = [x as u32, y as u32];
        let extent = [wx as u32, hy as u32];
        // The inspector permits a small float tolerance; pixel crops do not.
        let page = &index.summary.pages[frame.page as usize];
        if (0..2).any(|axis| {
            origin[axis]
                .checked_add(extent[axis])
                .is_none_or(|end| end > page.extent[axis])
        }) {
            return Err(reader.invalid(format!("frame {number} extends beyond its page")));
        }
        let duration = Duration::from_secs_f64(f64::from(frame.duration));
        if duration.is_zero() {
            return Err(reader.invalid(format!(
                "frame {number} duration is below nanosecond resolution"
            )));
        }
        frames.push(SpriteFrame {
            page: frame.page,
            duration,
            origin,
            extent,
        });
    }
    Ok(frames)
}

fn decoded_page(
    reader: &mut AssetReader,
    index: &Index,
    page: u32,
    remaining: u64,
) -> Result<TexturePage, InspectionError> {
    let summary = index
        .summary
        .pages
        .get(page as usize)
        .ok_or_else(|| reader.invalid("texture page index is out of range"))?;
    let location = &index.locations[page as usize];
    if summary.decoded_bytes > MAX_PAGE_BYTES {
        return Err(reader.invalid("decoded page exceeds 256 MiB"));
    }
    if summary.decoded_bytes > remaining {
        return Err(reader.invalid(
            "decoded page exceeds remaining composition source budget before decode; streaming is required",
        ));
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
        decode_png(reader, &payload, summary.extent)?
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
    let mut frame_locations = Vec::new();
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
            let page_index = reader.u32()?;
            let duration = reader.f32()?;
            let x = reader.f32()?;
            let y = reader.f32()?;
            let wx = reader.f32()?;
            let wy = reader.f32()?;
            let hx = reader.f32()?;
            let hy = reader.f32()?;
            let Some(page) = pages.get(page_index as usize) else {
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
            frame_locations.push(FrameLocation {
                page: page_index,
                duration,
                rectangle: [x, y, wx, wy, hx, hy],
            });
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
        frames: frame_locations,
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
