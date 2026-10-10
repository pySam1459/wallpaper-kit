use super::{
    InspectionError, SceneSource, SpriteSample, SpriteStats, SpriteStream, TextureDecoder,
};
use std::{
    sync::{Arc, Condvar, Mutex, TryLockError, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

/// One completed preparation, tagged with the accepted request identity/time.
#[derive(Debug)]
pub struct SpriteResult {
    id: u64,
    time: Duration,
    sample: Result<SpriteSample, InspectionError>,
    stats: SpriteStats,
}
impl SpriteResult {
    /// Identity returned by [`SpriteWorker::try_request_at`].
    pub fn request_id(&self) -> u64 {
        self.id
    }
    /// Caller-supplied time, including completed failures.
    pub fn time(&self) -> Duration {
        self.time
    }
    /// Stream residency/work after this preparation; zero before a stream opens.
    pub fn stats(&self) -> SpriteStats {
        self.stats
    }
    /// Take the prepared sample or its contextual asset/decode error.
    pub fn into_sample(self) -> Result<SpriteSample, InspectionError> {
        self.sample
    }
}

#[derive(Clone, Copy)]
struct Request {
    id: u64,
    time: Duration,
}
#[derive(Default)]
struct State {
    generation: u64,
    pending: Option<Request>,
    ready: Option<SpriteResult>,
    stopped: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

/// Owns one background sprite asset worker, with no playback timer or prefetch.
///
/// At most one preparation is active, one latest request is pending and one
/// result is ready. Opening/indexing/hashing and decoding run on this thread.
/// Its [`SpriteStream`] retains one atlas and extracted frame; caller-held
/// samples, separate workers and codec transients remain outside that bound.
/// Superseded results cannot be published. On a miss, keep the last completed
/// GPU frame rather than waiting. Dropping or shutting down joins this worker:
/// do that outside latency-critical UI work. Active codecs are not interrupted.
pub struct SpriteWorker {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl SpriteWorker {
    /// Start an idle thread without opening assets on the caller. Thread spawn
    /// failures return here; asset errors arrive in [`SpriteResult`]. The
    /// capacity-one receiver delivers coalesced wakeups, not samples. Dropping
    /// or leaving notifications undrained does not stop result publication.
    /// No I/O begins until a time request is accepted.
    pub fn new(
        source: SceneSource,
        asset: String,
    ) -> Result<(Self, mpsc::Receiver<()>), InspectionError> {
        let mut stream: Option<SpriteStream> = None;
        Self::spawn(move |time| {
            let sample = (|| {
                if stream.is_none() {
                    stream = Some(TextureDecoder::open(&source)?.sprite_stream(&asset)?);
                }
                stream.as_mut().expect("stream opened").sample(time)
            })();
            let stats = stream
                .as_ref()
                .map_or(SpriteStats::default(), SpriteStream::stats);
            (sample, stats)
        })
    }

    fn spawn(
        prepare: impl FnMut(Duration) -> (Result<SpriteSample, InspectionError>, SpriteStats)
        + Send
        + 'static,
    ) -> Result<(Self, mpsc::Receiver<()>), InspectionError> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let (notify, events) = mpsc::sync_channel(1);
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("scene-sprite-asset".into())
            .spawn(move || run(worker_shared, notify, prepare))
            .map_err(|error| InspectionError::io("sprite worker", error))?;
        Ok((
            Self {
                shared,
                worker: Some(worker),
            },
            events,
        ))
    }

    /// Accept a time request without waiting for metadata, asset I/O or codecs.
    /// `None` means busy metadata: no request was accepted; retry on another
    /// host tick. `Some(id)` replaces pending work and clears an unconsumed old
    /// result. Active work may finish, but its superseded result is discarded.
    /// Repeated current-frame times reuse the stream's sample Arc. Requests are
    /// explicit seeks; pausing means stop submitting them. Returns an error if
    /// the worker panicked/stopped or metadata was poisoned.
    pub fn try_request_at(&self, time: Duration) -> Result<Option<u64>, InspectionError> {
        if self.worker.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(failed("worker has stopped"));
        }
        let mut state = match self.shared.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(error)) => return Err(failed(error.to_string())),
        };
        state.generation += 1;
        let id = state.generation;
        state.pending = Some(Request { id, time });
        let previous = state.ready.take();
        self.shared.changed.notify_one();
        drop(state);
        // Large old samples must be freed outside the metadata lock.
        drop(previous);
        Ok(Some(id))
    }

    /// Take the latest completed result without waiting. `None` means pending,
    /// idle, already consumed or temporarily busy metadata. A result is taken
    /// once; errors are tagged with their request just like successful samples.
    /// Returns an error on worker failure rather than silently polling forever.
    pub fn try_take(&self) -> Result<Option<SpriteResult>, InspectionError> {
        match self.shared.state.try_lock() {
            Ok(mut state) => {
                let result = state.ready.take();
                if result.is_none() && self.worker.as_ref().is_none_or(JoinHandle::is_finished) {
                    return Err(failed("worker has stopped"));
                }
                Ok(result)
            }
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(error)) => Err(failed(error.to_string())),
        }
    }

    /// Cancel pending work and join. Active hashing/I/O/codecs may finish first,
    /// but cannot publish afterward. Call outside latency-critical UI work.
    /// Releases the asset handle; already returned samples remain valid.
    pub fn shutdown(mut self) -> Result<(), InspectionError> {
        self.stop();
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| failed("worker panicked"))?;
        }
        Ok(())
    }
    fn stop(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.stopped = true;
            state.pending = None;
            self.shared.changed.notify_one();
        }
    }
}
impl Drop for SpriteWorker {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            // Explicit shutdown reports panics; Drop still joins for cleanup.
            let _ = worker.join();
        }
    }
}
fn failed(detail: impl Into<String>) -> InspectionError {
    InspectionError::invalid("sprite worker", detail)
}
fn run(
    shared: Arc<Shared>,
    notify: mpsc::SyncSender<()>,
    mut prepare: impl FnMut(Duration) -> (Result<SpriteSample, InspectionError>, SpriteStats),
) {
    loop {
        let request = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            while !state.stopped && state.pending.is_none() {
                let Ok(next) = shared.changed.wait(state) else {
                    return;
                };
                state = next;
            }
            if state.stopped {
                return;
            }
            state.pending.take().expect("pending request")
        };
        let (sample, stats) = prepare(request.time);
        let result = SpriteResult {
            id: request.id,
            time: request.time,
            sample,
            stats,
        };
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        if state.stopped {
            return;
        }
        if state.generation == request.id {
            let previous = state.ready.replace(result);
            drop(state);
            drop(previous);
            // A full wakeup channel already represents this latest result.
            let _ = notify.try_send(());
        } else {
            drop(state);
            drop(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Instant,
    };

    fn request(worker: &SpriteWorker, time: Duration) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(id) = worker.try_request_at(time).unwrap() {
                return id;
            }
            assert!(Instant::now() < deadline, "request remained busy");
            thread::yield_now();
        }
    }

    #[test]
    fn blocked_preparation_coalesces_bursts_and_discards_stale_errors() {
        let (started, active) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let (worker, _undrained) = SpriteWorker::spawn(move |time| {
            if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                started.send(()).unwrap();
                gate.recv().unwrap();
            }
            (
                Err(failed(format!("requested {time:?}"))),
                SpriteStats::default(),
            )
        })
        .unwrap();
        let worker = Arc::new(worker);
        request(&worker, Duration::ZERO);
        active.recv_timeout(Duration::from_secs(5)).unwrap();
        let controls = worker.clone();
        let (finished, requests) = mpsc::sync_channel(1);
        let caller = thread::spawn(move || {
            let mut last = 0;
            for i in 1..=1000 {
                last = request(&controls, Duration::from_millis(i));
            }
            finished.send(last).unwrap();
        });
        // Always release before asserting: even a regressed control lock must
        // let test cleanup join its blocked codec rather than hang.
        let accepted = requests.recv_timeout(Duration::from_secs(2));
        release.send(()).unwrap();
        caller.join().unwrap();
        let id = accepted.expect("controls waited for blocked preparation");
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            if let Some(result) = worker.try_take().unwrap() {
                break result;
            }
            assert!(Instant::now() < deadline, "latest preparation timed out");
            thread::yield_now();
        };
        assert_eq!(result.request_id(), id);
        assert_eq!(result.time(), Duration::from_secs(1));
        assert!(result.into_sample().unwrap_err().to_string().contains("1s"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        // No timer or repeated work while the consumer is paused/idle.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        Arc::try_unwrap(worker).ok().unwrap().shutdown().unwrap();
    }

    #[test]
    fn shutdown_waits_for_active_work_and_cancels_pending_work() {
        let (started, active) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let (worker, _events) = SpriteWorker::spawn(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            started.send(()).unwrap();
            gate.recv().unwrap();
            (Err(failed("active preparation")), SpriteStats::default())
        })
        .unwrap();
        request(&worker, Duration::ZERO);
        active.recv_timeout(Duration::from_secs(5)).unwrap();
        request(&worker, Duration::from_secs(1));
        let shared = worker.shared.clone();
        let cleanup = thread::spawn(move || worker.shutdown());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !shared.state.lock().unwrap().stopped {
            assert!(Instant::now() < deadline, "shutdown was not signaled");
            thread::yield_now();
        }
        assert!(!cleanup.is_finished());
        release.send(()).unwrap();
        cleanup.join().unwrap().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(shared.state.lock().unwrap().ready.is_none());
    }

    #[test]
    fn panicked_worker_is_reported_and_joined() {
        let (worker, _events) = SpriteWorker::spawn(|_| panic!("codec fault")).unwrap();
        request(&worker, Duration::ZERO);
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.try_take().is_ok() {
            assert!(Instant::now() < deadline, "worker panic was not reported");
            thread::yield_now();
        }
        assert!(
            worker
                .shutdown()
                .unwrap_err()
                .to_string()
                .contains("panicked")
        );
    }
}
