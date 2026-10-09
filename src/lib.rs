//! Blank and diagnostic motion rendering, independent of any host framework.
//!
//! A worker renders on demand. Completed frames are immutable; resize creates a
//! replacement and does not invalidate a frame held by a consumer. This initial
//! API is experimental and does not implement scene playback or a frame pool.

#[cfg(not(windows))]
compile_error!("The initial scene-renderer backend requires Windows.");

mod gpu;
mod motion;

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

#[derive(Clone, Copy)]
enum Content {
    Blank,
    MotionFixture(Duration),
}

struct State {
    requested: Option<(u64, Size, Content)>,
    size: Size,
    content: Content,
    next_id: u64,
    latest: Option<Result<Arc<Frame>, Error>>,
    stopped: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
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
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                requested: Some((1, size, Content::Blank)),
                size,
                content: Content::Blank,
                next_id: 1,
                latest: None,
                stopped: false,
            }),
            changed: Condvar::new(),
        });
        let (notify, events) = mpsc::sync_channel(1);
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("scene-renderer".into())
            .spawn(move || run_worker(worker_shared, notify, mode))
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
        if let Some(content) = content {
            state.content = content;
        }
        state.next_id += 1;
        let id = state.next_id;
        state.requested = Some((id, state.size, state.content));
        self.shared.changed.notify_one();
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

fn run_worker(shared: Arc<Shared>, notify: mpsc::SyncSender<()>, mode: DeviceMode) {
    let gpu = match gpu::Gpu::new(mode) {
        Ok(gpu) => gpu,
        Err(error) => {
            if let Ok(mut state) = shared.state.lock() {
                state.latest = Some(Err(error));
                state.stopped = true;
            }
            let _ = notify.try_send(());
            return;
        }
    };
    loop {
        let (id, size, content) = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            while state.requested.is_none() && !state.stopped {
                let Ok(next) = shared.changed.wait(state) else {
                    return;
                };
                state = next;
            }
            if state.stopped {
                return;
            }
            state
                .requested
                .take()
                .expect("a request exists after the wait")
        };
        let result = gpu.render(id, size, content).map(Arc::new);
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        if state.stopped {
            return;
        }
        if state.requested.is_some() {
            continue;
        }
        state.latest = Some(result);
        drop(state);
        let _ = notify.try_send(());
    }
}
