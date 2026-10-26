//! Diagnostic Direct3D 11 rendering and read-only scene inspection.
//!
//! A worker renders on demand. Completed frames are immutable; resize creates a
//! replacement and does not invalidate a frame held by a consumer. A bounded
//! output pool reuses textures after leases and GPU access have finished. This
//! experimental API includes selected-image playback. Full scene effects,
//! particles and camera behavior remain unsupported.

#[cfg(not(windows))]
compile_error!("The initial wallpaper-kit backend requires Windows.");

mod assets;
mod gpu;
mod motion;
mod scene;

pub use assets::texture::{
    SpriteFrame, SpritePager, SpritePagingStats, SpriteResult, SpriteSample, SpriteStats,
    SpriteStream, SpriteWorker, TextureDecoder, TextureFormat, TexturePage, TextureRegion,
};

pub use scene::{
    AssetSummary, CloudMotion, CloudMotionReport, CloudMotionSettings, DependencySummary,
    Diagnostic, DiagnosticSeverity, EffectBindingSummary, EffectPassSummary, EffectSummary,
    GaussianBlur, ImageComposition, ImageLayer, ImagePlayback, ImagePlaybackReport,
    ImagePlaybackSample, ImageQuality, ImageSnapshot, InspectionError, MaterialPassSummary,
    MaterialSummary, ObjectKind, ObjectSummary, PackageSummary, PageSummary, PlaybackSettings,
    PreciseBlur, SceneInspection, SceneSource, SceneSummary, TextureSummary, inspect_scene,
};

use std::sync::{Arc, Condvar, Mutex, TryLockError, mpsc};
use std::thread::{self, JoinHandle};

pub use gpu::{AdapterInfo, Frame, Pixels};
use std::time::Duration;

/// Exact opaque RGBA8 output of the blank renderer, in SDR UNORM encoding.
pub const BLANK_RGBA: [u8; 4] = [16, 24, 32, 255];

/// Validated physical output dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    width: u32,
    height: u32,
}

impl Size {
    /// Rejects zero dimensions, dimensions over 16,384, and textures over 64 MiB.
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        if width == 0
            || height == 0
            || width > 16_384
            || height > 16_384
            || u64::from(width) * u64::from(height) * 4 > 64 * 1024 * 1024
        {
            return Err(Error::InvalidSize);
        }
        Ok(Self { width, height })
    }

    /// Width in physical pixels.
    pub fn width(self) -> u32 {
        self.width
    }

    /// Height in physical pixels.
    pub fn height(self) -> u32 {
        self.height
    }
}

/// Select a real GPU or explicitly request the software rasterizer for CI.
#[derive(Clone, Copy, Debug)]
pub enum DeviceMode {
    /// Use the default hardware adapter. Failure does not silently select WARP.
    Hardware,
    /// Use Windows WARP. This does not establish hardware performance.
    Software,
}

/// Invalid input, native graphics failure, or worker failure.
#[derive(Clone, Debug)]
pub enum Error {
    /// Output must satisfy [`Size::new`]'s bounds.
    InvalidSize,
    /// A source crop is empty, overflows, or extends beyond its page.
    InvalidTextureRegion,
    /// Layer geometry or appearance is outside [`ImageLayer::new`]'s bounds.
    InvalidImageLayer,
    /// Blur parameters or scratch dimensions exceed the documented bounds.
    InvalidBlur,
    /// Cloud settings exceed [`CloudMotionSettings::new`]'s bounds.
    InvalidCloudMotion,
    /// Image count or unique source bytes exceed [`ImageComposition::new`]'s bounds.
    ImageCompositionLimit,
    /// A Windows graphics operation failed.
    Graphics(windows::core::Error),
    /// The renderer's worker or state could not be accessed.
    Worker(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSize => {
                f.write_str("output dimensions are invalid or exceed the 64 MiB texture limit")
            }
            Self::InvalidTextureRegion => {
                f.write_str("texture region must be nonempty and entirely inside its page")
            }
            Self::InvalidImageLayer => f.write_str("image layer geometry or appearance is invalid"),
            Self::InvalidBlur => f.write_str("blur parameters or scratch dimensions exceed the selected profile's documented bounds"),
            Self::InvalidCloudMotion => f.write_str("cloud-motion settings exceed the documented bounds"),
            Self::ImageCompositionLimit => f.write_str("image composition requires 1..=32 layers and at most 256 MiB of unique source pages"),
            Self::Graphics(error) => write!(f, "Direct3D operation failed: {error}"),
            Self::Worker(message) => write!(f, "renderer worker failed: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Graphics(error) => Some(error),
            _ => None,
        }
    }
}

impl From<windows::core::Error> for Error {
    fn from(error: windows::core::Error) -> Self {
        Self::Graphics(error)
    }
}

#[derive(Clone)]
enum Content {
    Blank,
    MotionFixture(Duration),
    TexturePage(Arc<TexturePage>),
    TextureRegion(Arc<TextureRegion>),
    Images(Arc<ImageComposition>),
}

impl Content {
    fn texture(&self) -> Option<(&Arc<TexturePage>, [u32; 2], [u32; 2])> {
        match self {
            Self::TexturePage(page) => Some((page, [0, 0], page.extent())),
            Self::TextureRegion(region) => Some((region.page(), region.origin(), region.extent())),
            Self::Blank | Self::MotionFixture(_) | Self::Images(_) => None,
        }
    }
}

#[derive(Clone)]
struct Request {
    id: u64,
    size: Size,
    content: Content,
}

/// Diagnostic snapshot of output resources owned by the render worker.
///
/// Resident output counts cover the three-slot pool, including old sizes.
/// Source-page and effect scratch bytes are reported separately. These exclude consumer textures,
/// driver allocations and diagnostic readback.
#[derive(Clone, Copy, Debug, Default)]
pub struct OutputStats {
    /// Opt-in execution diagnostics, absent for [`Renderer::new`].
    pub profile: Option<RenderProfile>,
    /// Currently owned output textures, at most three.
    pub resident_textures: usize,
    /// RGBA8 payload bytes of those textures, at most 192 MiB.
    pub resident_bytes: u64,
    /// Cumulative output texture creations, including resize replacements.
    pub texture_allocations: u64,
    /// Cumulative submitted renders, including superseded requests.
    pub submitted_frames: u64,
    /// Completed frames accepted for publication; also the latest sequence.
    pub published_frames: u64,
    /// A pending request is waiting because all output slots have frame leases.
    pub waiting_for_frame_release: bool,
    /// GPU payload bytes for selected source pages, outside the output cap.
    pub source_texture_bytes: u64,
    /// GPU effect targets, separate from source/output: blur scratch, image-chain
    /// intermediates, HDR/refraction targets and the bloom pyramid. See
    /// [`ImageComposition`] for individual bounds.
    pub effect_texture_bytes: u64,
    /// Cumulative source-page uploads; repeated use of the same Arc reuses it.
    pub texture_uploads: u64,
}

/// Cumulative diagnostics for [`Renderer::new_profiled`], including startup.
/// Worker durations are wall time, not CPU accounting. GPU duration excludes
/// source upload, readback, completion polling and consumer presentation.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderProfile {
    /// Submitted frames whose worker preparation/submission was measured.
    pub submitted_frames: u64,
    /// Frames with reliable, completed GPU timestamp queries.
    pub gpu_frames: u64,
    /// Disjoint, unavailable or invalid timestamp samples; excluded from time.
    pub invalid_gpu_frames: u64,
    /// Sum of reliable GPU clear/draw/copy execution intervals in nanoseconds.
    pub gpu_draw_ns: u64,
    /// Worker resource preparation wall time, including initial compilation/upload.
    pub worker_prepare_ns: u64,
    /// Worker command submission wall time, including Flush.
    pub worker_submit_ns: u64,
}

struct State {
    requested: Option<Request>,
    size: Size,
    content: Content,
    next_id: u64,
    latest: Option<Result<Arc<Frame>, Error>>,
    stopped: bool,
    released_epoch: u64,
    output_stats: OutputStats,
    trim_requested: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn frame_released(&self) {
        // Pair notification with the metadata lock so a release cannot be lost
        // between the worker's resource check and its condition-variable wait.
        if let Ok(mut state) = self.state.lock() {
            state.released_epoch = state.released_epoch.wrapping_add(1);
            self.changed.notify_one();
        }
    }
}

/// Owns the render worker and its latest completed output.
///
/// Notifications are coalesced wakeups, not frames. A consumer should inspect
/// [`Self::try_frame`] after waking. No recurring render timer is installed.
pub struct Renderer {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Renderer {
    /// Starts the worker and schedules one blank frame. GPU startup errors are
    /// delivered by [`Self::try_frame`]. The receiver has capacity one.
    pub fn new(size: Size, mode: DeviceMode) -> Result<(Self, mpsc::Receiver<()>), Error> {
        Self::start(size, mode, false)
    }

    /// Starts a renderer with bounded, reusable GPU timestamp queries and worker
    /// timing. Read cumulative results with [`Self::try_output_stats`]. Profiling
    /// adds query overhead; use ordinary [`Self::new`] for production benchmarks.
    /// Query/device errors arrive through [`Self::try_frame`], like render errors.
    pub fn new_profiled(size: Size, mode: DeviceMode) -> Result<(Self, mpsc::Receiver<()>), Error> {
        Self::start(size, mode, true)
    }

    fn start(
        size: Size,
        mode: DeviceMode,
        profile: bool,
    ) -> Result<(Self, mpsc::Receiver<()>), Error> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                requested: Some(Request {
                    id: 1,
                    size,
                    content: Content::Blank,
                }),
                size,
                content: Content::Blank,
                next_id: 1,
                latest: None,
                stopped: false,
                released_epoch: 0,
                output_stats: OutputStats::default(),
                trim_requested: false,
            }),
            changed: Condvar::new(),
        });
        let (notify, events) = mpsc::sync_channel(1);
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("wallpaper-kit".into())
            .spawn(move || run_worker(worker_shared, notify, mode, profile))
            .map_err(|error| Error::Worker(error.to_string()))?;
        Ok((
            Self {
                shared,
                worker: Some(worker),
            },
            events,
        ))
    }

    /// Schedules a replacement, coalescing requests not yet started.
    /// Returns its identity so consumers can recognize completion. This locks
    /// only request metadata; rendering never takes place under that lock.
    /// If all three output slots are leased, the request remains pending until
    /// a frame is released. Retained old sizes count toward the same limit.
    pub fn resize(&self, size: Size) -> Result<u64, Error> {
        self.request(Some(size), None)
    }

    /// Renders the diagnostic moving-bar fixture at an explicit playback time.
    /// Its two-second loop has 40 steps of 50 ms. No timer is started; consumers
    /// control time. Resize preserves the most recently requested content/time.
    /// This fixture is not a scene loader or a production playback API.
    pub fn render_fixture_at(&self, time: Duration) -> Result<u64, Error> {
        self.request(None, Some(Content::MotionFixture(time)))
    }

    /// Schedules a diagnostic full-page draw, stretched to the output size.
    ///
    /// Decoding happens separately through [`TextureDecoder`]. This call
    /// only enqueues an Arc; shader compilation and upload run on the worker.
    /// Point sampling preserves code values without color conversion. RGBA/BC3
    /// use straight alpha over [`BLANK_RGBA`]; R8 displays opaque grayscale.
    /// Output is opaque. These explicit diagnostic conventions do not implement
    /// authored scene geometry, color/alpha semantics, effects or sprite timing.
    /// Resize preserves the page; reusing the same Arc avoids another upload.
    /// The worker retains one GPU source page, at most 256 MiB, outside the
    /// three-output cap. Caller-owned pages have no aggregate memory budget.
    pub fn render_texture_page(&self, page: Arc<TexturePage>) -> Result<u64, Error> {
        self.request(None, Some(Content::TexturePage(page)))
    }

    /// Enqueue a diagnostic cropped draw, stretching the region to the output.
    /// Uses the same integer point sampling, straight-alpha composition and
    /// source memory bounds as [`Self::render_texture_page`]. Regions sharing
    /// the same source Arc reuse its upload, including across crop/resize changes.
    /// No CPU crop, sprite timer or scene placement is performed. Resize retains
    /// the selected region. This method locks only coalesced request metadata.
    pub fn render_texture_region(&self, region: Arc<TextureRegion>) -> Result<u64, Error> {
        self.request(None, Some(Content::TextureRegion(region)))
    }

    /// Enqueue ordered orthographic image draws over the opaque blank color.
    /// The canvas fills the output; resize retains the composition. Geometry,
    /// point sampling, straight-alpha blend and RGB brightness are diagnostic
    /// conventions, not full Wallpaper Engine material compatibility. The worker
    /// compiles shaders/uploads sources; same source Arcs reuse their uploads,
    /// including across composition replacement. At most 256 MiB of unique
    /// source payloads are retained on the GPU, outside the three-output cap.
    /// No timer starts. Controls lock only coalesced request metadata.
    pub fn render_images(&self, composition: Arc<ImageComposition>) -> Result<u64, Error> {
        self.request(None, Some(Content::Images(composition)))
    }

    fn request(&self, size: Option<Size>, content: Option<Content>) -> Result<u64, Error> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|error| Error::Worker(error.to_string()))?;
        if state.stopped {
            return Err(Error::Worker("worker has stopped".into()));
        }
        if let Some(size) = size {
            state.size = size;
        }
        // Keep the previous content alive until metadata is unlocked: replacing
        // a pending page must not free its large buffer under the control lock.
        let previous_content =
            content.map(|content| std::mem::replace(&mut state.content, content));
        state.next_id += 1;
        state.trim_requested = false;
        let id = state.next_id;
        state.requested = Some(Request {
            id,
            size: state.size,
            content: state.content.clone(),
        });
        self.shared.changed.notify_one();
        drop(state);
        drop(previous_content);
        Ok(id)
    }

    /// Returns the latest immutable frame without waiting for the worker or GPU.
    /// Returns `None` if startup is pending or metadata is temporarily locked.
    pub fn try_frame(&self) -> Result<Option<Arc<Frame>>, Error> {
        match self.shared.state.try_lock() {
            Ok(state) => match &state.latest {
                Some(Ok(frame)) => Ok(Some(frame.clone())),
                Some(Err(error)) => Err(error.clone()),
                None => Ok(None),
            },
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(error)) => Err(Error::Worker(error.to_string())),
        }
    }

    /// Returns output-pool telemetry without waiting for metadata or the GPU.
    /// Returns `None` when metadata is temporarily locked. This is a diagnostic
    /// allocation snapshot, not total process or GPU memory usage.
    pub fn try_output_stats(&self) -> Result<Option<OutputStats>, Error> {
        match self.shared.state.try_lock() {
            Ok(state) => Ok(Some(state.output_stats)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(error)) => Err(Error::Worker(error.to_string())),
        }
    }

    /// Asynchronously releases GPU source/effect caches and unleased spare
    /// outputs after pending GPU work completes. Keeps the latest frame, all
    /// consumer-held frames and content needed for a still resize. Stop scheduling
    /// renders first. A new render request supersedes a pending trim; later
    /// rendering recreates resources on the worker without reducing quality.
    /// Inspect [`Self::try_output_stats`] for completion. Device errors arrive
    /// through [`Self::try_frame`]. No render or GPU wait occurs on the caller.
    pub fn trim_memory(&self) -> Result<(), Error> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|e| Error::Worker(e.to_string()))?;
        if state.stopped {
            return Err(Error::Worker("worker has stopped".into()));
        }
        state.trim_requested = true;
        self.shared.changed.notify_one();
        Ok(())
    }

    /// Stops and joins the worker. Call outside a latency-critical UI operation.
    pub fn shutdown(mut self) -> Result<(), Error> {
        self.stop();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::Worker("worker panicked".into()))?;
        }
        Ok(())
    }

    fn stop(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.stopped = true;
            self.shared.changed.notify_one();
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(shared: Arc<Shared>, notify: mpsc::SyncSender<()>, mode: DeviceMode, profile: bool) {
    let result =
        gpu::Gpu::new(mode, profile).and_then(|mut gpu| worker_loop(&shared, &notify, &mut gpu));
    if let Err(error) = result {
        let previous = if let Ok(mut state) = shared.state.lock() {
            let previous = state.latest.replace(Err(error));
            state.stopped = true;
            state.output_stats.resident_textures = 0;
            state.output_stats.resident_bytes = 0;
            state.output_stats.source_texture_bytes = 0;
            state.output_stats.effect_texture_bytes = 0;
            state.output_stats.waiting_for_frame_release = false;
            previous
        } else {
            None
        };
        // Frame destruction wakes the worker through metadata; never drop the
        // final frame reference while that metadata lock is held.
        drop(previous);
        let _ = notify.try_send(()); // Coalesced wakeup may already be queued.
    }
}

fn worker_loop(
    shared: &Arc<Shared>,
    notify: &mpsc::SyncSender<()>,
    gpu: &mut gpu::Gpu,
) -> Result<(), Error> {
    loop {
        let (request, desired, sequence, released, trim_requested) = {
            let state = shared
                .state
                .lock()
                .map_err(|error| Error::Worker(error.to_string()))?;
            if state.stopped {
                return Ok(());
            }
            (
                state.requested.clone(),
                state.next_id,
                state.output_stats.published_frames + 1,
                state.released_epoch,
                state.trim_requested,
            )
        };
        if let Some(frame) = gpu.poll(desired, sequence, Arc::downgrade(shared))? {
            let mut state = shared
                .state
                .lock()
                .map_err(|error| Error::Worker(error.to_string()))?;
            let previous = if !state.stopped && state.next_id == frame.id() {
                state.output_stats.published_frames += 1;
                state.latest.replace(Ok(frame))
            } else {
                drop(state);
                drop(frame);
                continue;
            };
            drop(state);
            drop(previous);
            let _ = notify.try_send(()); // Notification is a capacity-one hint.
        }
        if trim_requested && !gpu.has_pending() {
            let trim = {
                let mut state = shared
                    .state
                    .lock()
                    .map_err(|e| Error::Worker(e.to_string()))?;
                if state.requested.is_none() {
                    std::mem::take(&mut state.trim_requested)
                } else {
                    false
                }
            };
            if trim {
                gpu.trim_memory()?;
            }
        }
        let mut waiting_for_release = false;
        let mut submitted_now = false;
        let mut poll_again = gpu.has_pending();
        if !poll_again && let Some(request) = request {
            match gpu.try_submit(&request)? {
                gpu::Submission::Submitted => {
                    poll_again = true;
                    submitted_now = true;
                    let mut state = shared
                        .state
                        .lock()
                        .map_err(|error| Error::Worker(error.to_string()))?;
                    if state
                        .requested
                        .as_ref()
                        .is_some_and(|current| current.id == request.id)
                    {
                        state.requested = None;
                    }
                }
                gpu::Submission::Leased => waiting_for_release = true,
                gpu::Submission::Busy => poll_again = true,
            }
        }
        let mut state = shared
            .state
            .lock()
            .map_err(|error| Error::Worker(error.to_string()))?;
        let published_frames = state.output_stats.published_frames;
        state.output_stats = gpu.stats();
        state.output_stats.published_frames = published_frames;
        state.output_stats.waiting_for_frame_release = waiting_for_release;
        if state.stopped {
            return Ok(());
        }
        if submitted_now
            || (state.trim_requested && state.requested.is_none() && !poll_again)
            || state.next_id != desired
            || state.released_epoch != released
        {
            continue;
        }
        // Only submitted queries or consumer GPU access need periodic polling.
        // Idle and lease-saturated workers sleep until a request/release/stop.
        if poll_again {
            drop(state);
            // On the tested Windows runtime, a 1 ms Condvar timeout rounded to
            // roughly 16 ms. Sleep polls pending GPU work without that delay;
            // the next iteration still handles requests and shutdown first.
            thread::sleep(Duration::from_millis(1));
        } else {
            drop(
                shared
                    .changed
                    .wait(state)
                    .map_err(|error| Error::Worker(error.to_string()))?,
            );
        }
    }
}
