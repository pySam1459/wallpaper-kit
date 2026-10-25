use super::*;

const MAX_FRAME_BYTES: u64 = 64 * 1024 * 1024;

/// One selected frame, retaining only its extracted pixels or enclosing blocks.
#[derive(Clone, Debug)]
pub struct SpriteSample {
    index: usize,
    authored: SpriteFrame,
    region: Arc<TextureRegion>,
}
impl SpriteSample {
    /// Zero-based authored frame index.
    pub fn frame_index(&self) -> usize {
        self.index
    }
    /// Original atlas rectangle, page index and positive duration.
    pub fn authored_frame(&self) -> SpriteFrame {
        self.authored
    }
    /// Checked region of the extracted page, ready for the existing GPU API.
    /// Cloning this Arc shares an upload across layer instances.
    pub fn region(&self) -> &Arc<TextureRegion> {
        &self.region
    }
}

/// Per-stream residency and work counters; not total CPU/GPU memory.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpriteStats {
    /// Successful base-page decodes, including reloads after seek/loop.
    pub page_decodes: u64,
    /// Successful extracted-frame copies; repeated current-frame times reuse.
    pub frame_extracts: u64,
    /// Cached decoded atlas payload, at most 256 MiB.
    pub cached_page_bytes: u64,
    /// Cached extracted payload, at most 64 MiB.
    pub cached_frame_bytes: u64,
}

/// Blocking deterministic sprite sampling for one asset worker or tool.
///
/// Owns a validated metadata index, read-locked asset, one decoded atlas page
/// and one extracted frame. No timer, prefetch, async queue or GPU device.
/// Caller-retained samples, separate streams and codec transients are outside
/// this residency bound. Sharing one sample between instances shares its data.
pub struct SpriteStream {
    reader: AssetReader,
    indexed: Index,
    frames: Vec<SpriteFrame>,
    ends: Vec<u128>,
    duration: Duration,
    page: Option<(u32, TexturePage)>,
    current: Option<SpriteSample>,
    page_decodes: u64,
    frame_extracts: u64,
}
impl SpriteStream {
    pub(super) fn batch_admission(
        &self,
        frames: std::ops::Range<usize>,
    ) -> Result<(u64, u64), InspectionError> {
        let mut bytes = 0;
        for frame in &self.frames[frames.clone()] {
            let (_, extent, _) = extraction(self.indexed.summary.format, *frame);
            let size = layout_bytes(self.indexed.summary.format, extent);
            if size > MAX_FRAME_BYTES {
                return Err(self
                    .reader
                    .invalid("extracted sprite frame exceeds 64 MiB before atlas decode"));
            }
            bytes += size;
        }
        if bytes > MAX_PAGE_BYTES {
            return Err(self
                .reader
                .invalid("sprite batch exceeds 256 MiB before decode"));
        }
        let page = &self.indexed.summary.pages[self.frames[frames.start].page as usize];
        if page.decoded_bytes > MAX_PAGE_BYTES {
            return Err(self.reader.invalid("decoded page exceeds 256 MiB"));
        }
        // PNG: stored input, optional LZ4-expanded PNG, decoder workspace.
        // Raw LZ4: stored input plus its final page buffer.
        let workspace = if self.indexed.codec != super::Codec::Raw {
            3 * MAX_ENCODED_BYTES
        } else if self.indexed.locations[self.frames[frames.start].page as usize].compressed {
            MAX_ENCODED_BYTES
        } else {
            0
        };
        Ok((
            bytes,
            (page.decoded_bytes + bytes).max(page.decoded_bytes + workspace),
        ))
    }
    pub(super) fn clear_pixels(&mut self) {
        self.current = None;
        self.page = None;
    }
    pub(super) fn clear_frame(&mut self) {
        self.current = None;
    }
    pub(super) fn open(package: &Package, asset: &str) -> Result<Self, InspectionError> {
        let mut reader = package
            .reader(asset)?
            .ok_or_else(|| InspectionError::invalid(asset, "texture asset is missing"))?;
        let indexed = index(&mut reader)?;
        let frames = checked_frames(&reader, &indexed)?;
        let mut duration = Duration::ZERO;
        let mut ends = Vec::with_capacity(frames.len());
        // At most 65,536 checked durations, each positive and <=60 seconds.
        for frame in &frames {
            duration += frame.duration();
            ends.push(duration.as_nanos());
        }
        Ok(Self {
            reader,
            indexed,
            frames,
            ends,
            duration,
            page: None,
            current: None,
            page_decodes: 0,
            frame_extracts: 0,
        })
    }
    /// Authored checked frames, without decoding payloads.
    pub fn frames(&self) -> &[SpriteFrame] {
        &self.frames
    }
    /// Sum of durations rounded individually to nanoseconds.
    pub fn duration(&self) -> Duration {
        self.duration
    }
    /// Select by cumulative duration; exact boundaries choose the next frame.
    /// Times loop modulo the total, including very large `Duration` values.
    /// Performs no I/O or pixel allocation.
    pub fn frame_index_at(&self, time: Duration) -> usize {
        let local = time.as_nanos() % self.duration.as_nanos();
        self.ends.partition_point(|end| *end <= local)
    }
    /// Decode/extract the selected frame synchronously. Never call on a host UI
    /// thread. RGBA/R8 rows are copied; BC3 enclosing blocks remain compressed,
    /// with a residual crop for unaligned coordinates. New frame payloads are
    /// limited to 64 MiB before atlas decoding; atlas decode remains <=256 MiB.
    /// Repeated requests for the current index return the same region Arc.
    /// Page changes evict the old atlas before decoding. Errors retain no new
    /// sample; old caller-held samples remain valid. This is not full scene
    /// playback, filtering or a global memory budget.
    pub fn sample(&mut self, time: Duration) -> Result<SpriteSample, InspectionError> {
        let selected = self.frame_index_at(time);
        if let Some(current) = &self.current
            && current.index == selected
        {
            return Ok(current.clone());
        }
        let authored = self.frames[selected];
        let (copy_origin, copy_extent, crop_origin) =
            extraction(self.indexed.summary.format, authored);
        if layout_bytes(self.indexed.summary.format, copy_extent) > MAX_FRAME_BYTES {
            return Err(self
                .reader
                .invalid("extracted sprite frame exceeds 64 MiB before atlas decode"));
        }
        self.current = None;
        if self
            .page
            .as_ref()
            .is_none_or(|(page, _)| *page != authored.page_index())
        {
            self.page = None;
            let page = decoded_page(
                &mut self.reader,
                &self.indexed,
                authored.page_index(),
                MAX_PAGE_BYTES,
            )?;
            self.page = Some((authored.page_index(), page));
            self.page_decodes += 1;
        }
        let source = &self.page.as_ref().expect("selected page decoded").1;
        let (unit, divisor) = match source.format() {
            TextureFormat::Rgba8 => (4, 1),
            TextureFormat::R8 => (1, 1),
            TextureFormat::Rg8 => (2, 1),
            TextureFormat::Bc3 => (16, 4),
        };
        let stride = source.extent()[0].div_ceil(divisor) as usize * unit;
        let row_bytes = (copy_extent[0] / divisor) as usize * unit;
        let first_x = (copy_origin[0] / divisor) as usize * unit;
        let first_y = copy_origin[1] / divisor;
        let mut data = Vec::with_capacity(row_bytes * (copy_extent[1] / divisor) as usize);
        for row in first_y..first_y + copy_extent[1] / divisor {
            let offset = row as usize * stride + first_x;
            data.extend_from_slice(&source.data()[offset..offset + row_bytes]);
        }
        let page = Arc::new(TexturePage {
            extent: copy_extent,
            format: source.format(),
            data,
        });
        let region = Arc::new(
            TextureRegion::new(page, crop_origin, authored.extent())
                .expect("extraction encloses checked rectangle"),
        );
        let sample = SpriteSample {
            index: selected,
            authored,
            region,
        };
        self.current = Some(sample.clone());
        self.frame_extracts += 1;
        Ok(sample)
    }
    /// Current cache payloads and cumulative successful work.
    pub fn stats(&self) -> SpriteStats {
        SpriteStats {
            page_decodes: self.page_decodes,
            frame_extracts: self.frame_extracts,
            cached_page_bytes: self
                .page
                .as_ref()
                .map_or(0, |(_, page)| page.data().len() as u64),
            cached_frame_bytes: self
                .current
                .as_ref()
                .map_or(0, |sample| sample.region.page().data().len() as u64),
        }
    }
}

fn extraction(format: u32, frame: SpriteFrame) -> ([u32; 2], [u32; 2], [u32; 2]) {
    let origin = frame.origin();
    let extent = frame.extent();
    if format != 4 {
        return (origin, extent, [0, 0]);
    }
    let start = origin.map(|v| v / 4 * 4);
    let end = [0, 1].map(|axis| (origin[axis] + extent[axis]).div_ceil(4) * 4);
    (
        start,
        [end[0] - start[0], end[1] - start[1]],
        [origin[0] - start[0], origin[1] - start[1]],
    )
}
