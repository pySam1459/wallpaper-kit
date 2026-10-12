//! Local FFmpeg decoding on a demand-driven worker, with one queued selection.

use crate::{InspectionError, Size, TexturePage, TextureRegion};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Condvar, Mutex, TryLockError},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct State {
    requested: Option<u64>,
    ready: Option<(u64, Arc<TextureRegion>)>,
    error: Option<String>,
    stopped: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    child: Mutex<Option<Child>>,
}
pub(crate) struct VideoPager {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    frames: u64,
    rate: (u32, u32),
    temporary: Option<VideoFile>,
}

static TEMPORARY: AtomicU64 = AtomicU64::new(0);
pub(crate) struct VideoFile {
    path: PathBuf,
    pub extent: [u32; 2],
}
impl VideoFile {
    pub(crate) fn copy_from(
        reader: &mut super::package::AssetReader,
        bytes: u64,
        extent: [u32; 2],
    ) -> Result<Self, InspectionError> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "wallpaper-kit-video-{}-{stamp}-{}.mp4",
            std::process::id(),
            TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| InspectionError::io("temporary video", error))?;
        let file = Self { path, extent };
        reader.copy_to(&mut output, bytes, 256 * 1024 * 1024)?;
        Ok(file)
    }
}
impl Drop for VideoFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path); // Remove only the uniquely owned asset copy.
    }
}

fn command(name: &str) -> Command {
    let mut command = Command::new(name);
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW for background codecs.
    command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped());
    command
}

impl VideoPager {
    pub(crate) fn embedded(file: VideoFile) -> Result<Self, InspectionError> {
        let size = Size::new(file.extent[0], file.extent[1])
            .map_err(|error| InspectionError::invalid("video texture", error.to_string()))?;
        let (mut pager, _) = Self::open(&file.path, size)?;
        pager.temporary = Some(file);
        Ok(pager)
    }
    pub(crate) fn open(path: &Path, size: Size) -> Result<(Self, String), InspectionError> {
        let path = path
            .canonicalize()
            .map_err(|error| InspectionError::io("video", error))?;
        let mut file = File::open(&path).map_err(|error| InspectionError::io("video", error))?;
        if !file
            .metadata()
            .map_err(|error| InspectionError::io("video", error))?
            .is_file()
        {
            return Err(InspectionError::invalid(
                "video",
                "source must be a local file",
            ));
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|error| InspectionError::io("video", error))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let hash = format!("{:x}", hasher.finalize());
        let mut probe = command("ffprobe.exe")
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-show_entries",
                "format=duration:stream=avg_frame_rate,r_frame_rate",
                "-select_streams",
                "v:0",
                "-of",
                "json",
            ])
            .arg(&path)
            .spawn()
            .map_err(|error| {
                InspectionError::invalid("video", format!("ffprobe.exe is required: {error}"))
            })?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match probe
                .try_wait()
                .map_err(|error| InspectionError::io("video metadata", error))?
            {
                Some(status) if status.success() => break,
                Some(status) => {
                    return Err(InspectionError::invalid(
                        "video",
                        format!("ffprobe failed: {status}"),
                    ));
                }
                None if Instant::now() >= deadline => {
                    let _ = probe.kill(); // Terminate only this owned metadata probe.
                    let _ = probe.wait();
                    return Err(InspectionError::invalid(
                        "video",
                        "metadata probe timed out",
                    ));
                }
                None => thread::sleep(Duration::from_millis(5)),
            }
        }
        let mut metadata = Vec::new();
        probe
            .stdout
            .take()
            .expect("piped stdout")
            .take(4096)
            .read_to_end(&mut metadata)
            .map_err(|error| InspectionError::io("video metadata", error))?;
        let value: serde_json::Value = serde_json::from_slice(&metadata)
            .map_err(|error| InspectionError::invalid("video metadata", error.to_string()))?;
        let rate = ["avg_frame_rate", "r_frame_rate"]
            .into_iter()
            .find_map(|key| {
                let (numerator, denominator) =
                    value["streams"][0][key].as_str()?.split_once('/')?;
                let numerator = numerator.parse::<u32>().ok()?;
                let denominator = denominator.parse::<u32>().ok()?;
                (denominator != 0
                    && numerator != 0
                    && u64::from(numerator) <= u64::from(denominator) * 240)
                    .then_some((numerator, denominator))
            })
            .ok_or_else(|| {
                InspectionError::invalid("video", "expected a source frame rate in 0..=240")
            })?;
        let fps = f64::from(rate.0) / f64::from(rate.1);
        let duration = value["format"]["duration"]
            .as_str()
            .and_then(|duration| duration.parse::<f64>().ok())
            .filter(|duration| duration.is_finite() && *duration >= 1. / fps && *duration <= 86400.)
            .ok_or_else(|| {
                InspectionError::invalid(
                    "video",
                    "expected duration between one source frame and one day",
                )
            })?;
        let frames = (duration * fps).ceil() as u64;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                requested: None,
                ready: None,
                error: None,
                stopped: false,
            }),
            changed: Condvar::new(),
            child: Mutex::new(None),
        });
        let thread_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("wallpaper-video".into())
            .spawn(move || {
                if let Err(error) = decode(&thread_shared, &path, size, rate) {
                    thread_shared.state.lock().expect("video state").error =
                        Some(error.to_string());
                }
                stop_child(&thread_shared);
            })
            .map_err(|error| InspectionError::io("video worker", error))?;
        Ok((
            Self {
                shared,
                worker: Some(worker),
                frames,
                rate,
                temporary: None,
            },
            hash,
        ))
    }

    pub(crate) fn try_sample_at(
        &self,
        time: Duration,
    ) -> Result<Option<(Arc<TextureRegion>, usize)>, InspectionError> {
        let frame = ((time.as_nanos() * u128::from(self.rate.0)
            / (1_000_000_000 * u128::from(self.rate.1)))
            % u128::from(self.frames)) as u64;
        let mut state = match self.shared.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                return Err(InspectionError::invalid("video", "worker state poisoned"));
            }
        };
        if let Some(error) = &state.error {
            return Err(InspectionError::invalid("video", error));
        }
        if let Some((ready, region)) = &state.ready
            && *ready == frame
        {
            return Ok(Some((region.clone(), frame as usize)));
        }
        state.requested = Some(frame);
        self.shared.changed.notify_one();
        Ok(None)
    }
}

fn stop_child(shared: &Shared) {
    if let Some(mut child) = shared.child.lock().expect("video process").take() {
        let _ = child.kill(); // A codec may already have exited at end of file.
        let _ = child.wait(); // Reap the owned process even after cancellation.
    }
}

impl Drop for VideoPager {
    fn drop(&mut self) {
        self.shared.state.lock().expect("video state").stopped = true;
        self.shared.changed.notify_one();
        stop_child(&self.shared);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join(); // Explicit failure is observed during sampling.
        }
    }
}

fn start_decoder(
    shared: &Shared,
    path: &PathBuf,
    size: Size,
    frame: u64,
    rate: (u32, u32),
) -> Result<ChildStdout, InspectionError> {
    stop_child(shared);
    let mut child = command("ffmpeg.exe")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-protocol_whitelist",
            "file,pipe",
            "-ss",
        ])
        .arg(format!(
            "{:.9}",
            frame as f64 * f64::from(rate.1) / f64::from(rate.0)
        ))
        .arg("-i")
        .arg(path)
        .args(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf"])
        .arg(format!(
            "fps={}/{},scale={}:{}",
            rate.0,
            rate.1,
            size.width(),
            size.height()
        ))
        .args([
            "-threads", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
        ])
        .spawn()
        .map_err(|error| {
            InspectionError::invalid("video", format!("ffmpeg.exe is required: {error}"))
        })?;
    let stdout = child.stdout.take().expect("piped stdout");
    *shared.child.lock().expect("video process") = Some(child);
    // Drop can race process creation. Check after publishing the handle so it
    // always has either a cancellable child or a worker that observes shutdown.
    if shared.state.lock().expect("video state").stopped {
        stop_child(shared);
    }
    Ok(stdout)
}

fn decode(
    shared: &Shared,
    path: &PathBuf,
    size: Size,
    rate: (u32, u32),
) -> Result<(), InspectionError> {
    let mut stdout = None;
    let mut next = 0;
    loop {
        let requested = {
            let mut state = shared.state.lock().expect("video state");
            while state.requested.is_none() && !state.stopped {
                state = shared.changed.wait(state).expect("video state");
            }
            if state.stopped {
                return Ok(());
            }
            state.requested.take().expect("request available")
        };
        if stdout.is_none()
            || requested < next
            || requested > next + u64::from(rate.0).div_ceil(u64::from(rate.1))
        {
            stdout = Some(start_decoder(shared, path, size, requested, rate)?);
            next = requested;
        }
        let mut pixels = vec![0; size.width() as usize * size.height() as usize * 4];
        loop {
            stdout
                .as_mut()
                .expect("decoder started")
                .read_exact(&mut pixels)
                .map_err(|error| {
                    InspectionError::invalid(
                        "video",
                        format!("FFmpeg frame {next} could not be decoded: {error}"),
                    )
                })?;
            if next == requested {
                next += 1;
                break;
            }
            next += 1;
            let state = shared.state.lock().expect("video state");
            if state.stopped {
                return Ok(());
            }
            if state.requested.is_some_and(|latest| latest != requested) {
                break;
            }
        }
        let page = Arc::new(TexturePage::rgba([size.width(), size.height()], pixels)?);
        let region = Arc::new(
            TextureRegion::new(page, [0, 0], [size.width(), size.height()])
                .map_err(|error| InspectionError::invalid("video", error.to_string()))?,
        );
        let mut state = shared.state.lock().expect("video state");
        if next == requested + 1 && state.requested.is_none_or(|latest| latest == requested) {
            state.ready = Some((requested, region));
            state.requested = None;
        }
    }
}
