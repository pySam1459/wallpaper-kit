use super::{InspectionError, SceneSource, SpriteSample, SpriteStream, TextureDecoder};
use std::{
    ops::Range,
    sync::{Arc, Condvar, Mutex, TryLockError, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

const MAX_WORKING_BYTES: u64 = 768 * 1024 * 1024;
const TARGET_BATCH_BYTES: u64 = 64 * 1024 * 1024;

/// Per-pager payload admission and preparation counters, not total process memory.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpritePagingStats {
    /// Successful complete batches, including reloads after eviction.
    pub prepared_batches: u64,
    /// Preparations discarded after a seek made their group unnecessary.
    pub discarded_batches: u64,
    /// Successful atlas decodes, including canceled preparations.
    pub page_decodes: u64,
    /// Extracted frame copies, including canceled preparations.
    pub frame_extracts: u64,
    /// Ready batches, at most two contiguous windows within authored page runs.
    pub cached_batches: usize,
    /// Ready extracted frame payloads, at most 128 MiB.
    pub cached_frame_bytes: u64,
    /// Retained atlas bytes when idle, or conservative decode/extraction reservation.
    pub reserved_work_bytes: u64,
    /// Largest sum of ready payloads and admitted work, at most 768 MiB.
    pub peak_admitted_bytes: u64,
    /// Asset initialization, queued demand or one batch preparation is active.
    pub preparing: bool,
    /// Polls with unavailable frames, excluding metadata contention.
    pub unavailable_polls: u64,
}

struct Timeline {
    ends: Vec<u128>,
    groups: Vec<Group>,
}
struct Group {
    frames: Range<usize>,
    bytes: u64,
    reservation: u64,
}
impl Timeline {
    fn new(stream: &SpriteStream) -> Result<Self, InspectionError> {
        let mut ends = Vec::with_capacity(stream.frames().len());
        let mut total = 0;
        let mut groups = Vec::new();
        let mut start = 0;
        for (index, frame) in stream.frames().iter().enumerate() {
            total += frame.duration().as_nanos();
            ends.push(total);
            if index > 0 && frame.page_index() != stream.frames()[index - 1].page_index() {
                groups.push(start..index);
                start = index;
            }
        }
        groups.push(start..stream.frames().len());
        let mut windows = Vec::new();
        for run in groups {
            // Preserve whole-run validation before decoding, even though ready
            // storage now retains only short windows from that run.
            stream.batch_admission(run.clone())?;
            let first_window = windows.len();
            let mut start = run.start;
            let mut bytes = 0;
            for index in run.clone() {
                let size = stream.batch_admission(index..index + 1)?.0;
                if index > start && bytes + size > TARGET_BATCH_BYTES {
                    windows.push(start..index);
                    start = index;
                    bytes = 0;
                }
                bytes += size;
            }
            windows.push(start..run.end);
            // A tiny tail shortens the lead for decoding the next page. Move
            // frames from its predecessor without exceeding the byte bound.
            if windows.len() > first_window + 1 {
                let last = windows.len() - 1;
                let mut last_bytes = stream.batch_admission(windows[last].clone())?.0;
                while windows[last - 1].len() > 1 && last_bytes < TARGET_BATCH_BYTES / 2 {
                    let candidate = windows[last].start - 1..windows[last].end;
                    let size = stream
                        .batch_admission(candidate.start..candidate.start + 1)?
                        .0;
                    if last_bytes + size > TARGET_BATCH_BYTES {
                        break;
                    }
                    windows[last - 1].end -= 1;
                    windows[last] = candidate;
                    last_bytes += size;
                }
            }
        }
        let groups = windows
            .into_iter()
            .map(|frames| {
                let (bytes, reservation) = stream.batch_admission(frames.clone())?;
                Ok(Group {
                    frames,
                    bytes,
                    reservation,
                })
            })
            .collect::<Result<_, InspectionError>>()?;
        Ok(Self { ends, groups })
    }
    fn selection(&self, time: Duration) -> (usize, usize) {
        let local = time.as_nanos() % self.ends.last().expect("checked nonempty frames");
        let frame = self.ends.partition_point(|end| *end <= local);
        (
            self.groups
                .partition_point(|group| group.frames.end <= frame),
            frame,
        )
    }
    fn next(&self, group: usize) -> usize {
        (group + 1) % self.groups.len()
    }
    fn start(&self, frame: usize) -> Duration {
        // Checked TEX durations sum to less than four million seconds.
        Duration::from_nanos(if frame == 0 {
            0
        } else {
            self.ends[frame - 1] as u64
        })
    }
}
struct Batch {
    group: usize,
    first: usize,
    frames: Vec<SpriteSample>,
    bytes: u64,
}
struct State {
    time: Duration,
    timeline: Option<Arc<Timeline>>,
    batches: Vec<Batch>,
    failure: Option<InspectionError>,
    failed: bool,
    stats: SpritePagingStats,
    stopped: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

/// Background preparation of current and next bounded windows of authored frames.
///
/// Each batch owns at most 64 MiB of extracted frames. It never crosses
/// an authored page boundary. One atlas stays decoded between windows on that
/// page, avoiding repeated codec work. Playback samples ready batches while
/// the asset worker prepares ahead. At most two batches plus one atlas and
/// preparation workspace are admitted under a 768 MiB cap. Whole page runs
/// still require <=256 MiB of extracted payload before decoding. Caller-retained samples, separate
/// pagers, metadata, allocator overhead and GPU memory are outside that cap.
/// No clock, timer, scene replacement or full-scene memory coordinator exists.
/// Drop and shutdown join: perform them outside latency-critical UI work.
pub struct SpritePager {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl SpritePager {
    /// Starts background opening/indexing and prepares time zero plus its next
    /// window. Performs no file I/O on the caller. Notifications have capacity
    /// one and are coalesced; dropping/ignoring them cannot block preparation.
    /// Asset/admission errors arrive through [`Self::try_sample_at`].
    pub fn new(
        source: SceneSource,
        asset: String,
    ) -> Result<(Self, mpsc::Receiver<()>), InspectionError> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                time: Duration::ZERO,
                timeline: None,
                batches: Vec::new(),
                failure: None,
                failed: false,
                stats: SpritePagingStats {
                    preparing: true,
                    ..SpritePagingStats::default()
                },
                stopped: false,
            }),
            changed: Condvar::new(),
        });
        let (notify, events) = mpsc::sync_channel(1);
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("scene-sprite-prefetch".into())
            .spawn(move || {
                if let Err(error) = prepare(&worker_shared, &notify, source, asset)
                    && let Ok(mut state) = worker_shared.state.lock()
                    && !state.stopped
                {
                    state.failure = Some(error);
                    state.failed = true;
                    state.stats.preparing = false;
                    state.stats.reserved_work_bytes = 0;
                    drop(state);
                    let _ = notify.try_send(()); // A queued wakeup already represents failure.
                }
            })
            .map_err(|error| InspectionError::io("sprite pager", error))?;
        Ok((
            Self {
                shared,
                worker: Some(worker),
            },
            events,
        ))
    }
    /// Select by cumulative durations and modulo looping, and update demand.
    /// Returns a shared ready frame without I/O, copying pixels or waiting for
    /// metadata/codecs. `None` means unavailable or busy metadata: keep the last
    /// completed GPU frame and retry. A seek replaces demand; obsolete batch
    /// work may finish but cannot supply the wrong frame. Repeated selections
    /// of a resident frame share its Arc. Stop advancing time to pause; bounded
    /// outstanding preparation settles, then the asset thread sleeps.
    /// Asset errors are terminal: the first poll takes the contextual failure;
    /// later polls report that the worker stopped. Recreate to retry.
    pub fn try_sample_at(&self, time: Duration) -> Result<Option<SpriteSample>, InspectionError> {
        let mut state = match self.shared.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(error)) => return Err(failed(error.to_string())),
        };
        if let Some(error) = state.failure.take() {
            return Err(error);
        }
        if state.failed || self.worker.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(failed("worker stopped"));
        }
        let old = state.time;
        state.time = time;
        if let Some(timeline) = state.timeline.clone() {
            let (group, frame) = timeline.selection(time);
            if group != timeline.selection(old).0 {
                state.stats.preparing = [group, timeline.next(group)]
                    .into_iter()
                    .any(|needed| !state.batches.iter().any(|batch| batch.group == needed));
                self.shared.changed.notify_one();
            }
            if let Some(batch) = state.batches.iter().find(|batch| batch.group == group) {
                return Ok(Some(batch.frames[frame - batch.first].clone()));
            }
        }
        state.stats.unavailable_polls += 1;
        Ok(None)
    }
    /// Snapshot payload admission and preparation without waiting for metadata.
    /// Returns `None` on contention. Unavailable polls are not displayed misses;
    /// the consumer must measure requests, publications and live deadlines.
    pub fn try_stats(&self) -> Result<Option<SpritePagingStats>, InspectionError> {
        match self.shared.state.try_lock() {
            Ok(state) => Ok(Some(state.stats)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(error)) => Err(failed(error.to_string())),
        }
    }
    /// Cancel future work and join active preparation. Codecs may finish first.
    /// Releases assets; previously returned extracted samples remain valid.
    /// Call outside latency-critical UI work. Reports a worker panic.
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
            self.shared.changed.notify_one();
        }
    }
}
impl Drop for SpritePager {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        } // Explicit shutdown reports panics.
    }
}
fn failed(detail: impl Into<String>) -> InspectionError {
    InspectionError::invalid("sprite pager", detail)
}
fn needed(state: &State, timeline: &Timeline, group: usize) -> bool {
    let current = timeline.selection(state.time).0;
    !state.stopped && (group == current || group == timeline.next(current))
}
fn prepare(
    shared: &Shared,
    notify: &mpsc::SyncSender<()>,
    source: SceneSource,
    asset: String,
) -> Result<(), InspectionError> {
    let mut stream = TextureDecoder::open(&source)?.sprite_stream(&asset)?;
    let timeline = Arc::new(Timeline::new(&stream)?);
    {
        let mut state = shared
            .state
            .lock()
            .map_err(|error| failed(error.to_string()))?;
        state.timeline = Some(timeline.clone());
    }
    loop {
        let (group, obsolete) = {
            let mut state = shared
                .state
                .lock()
                .map_err(|error| failed(error.to_string()))?;
            loop {
                if state.stopped {
                    return Ok(());
                }
                let current = timeline.selection(state.time).0;
                let next = timeline.next(current);
                let target = [current, next]
                    .into_iter()
                    .find(|group| !state.batches.iter().any(|batch| batch.group == *group));
                if let Some(group) = target {
                    let mut obsolete = Vec::new();
                    let mut index = 0;
                    while index < state.batches.len() {
                        if !needed(&state, &timeline, state.batches[index].group) {
                            obsolete.push(state.batches.swap_remove(index));
                        } else {
                            index += 1;
                        }
                    }
                    let reservation = timeline.groups[group]
                        .reservation
                        .max(stream.stats().cached_page_bytes + timeline.groups[group].bytes);
                    state.stats.cached_batches = state.batches.len();
                    state.stats.cached_frame_bytes =
                        state.batches.iter().map(|batch| batch.bytes).sum();
                    let admitted = state.stats.cached_frame_bytes + reservation;
                    if admitted > MAX_WORKING_BYTES {
                        return Err(failed("sprite preparation exceeds 768 MiB admission"));
                    }
                    state.stats.reserved_work_bytes = reservation;
                    state.stats.peak_admitted_bytes = state.stats.peak_admitted_bytes.max(admitted);
                    state.stats.preparing = true;
                    break (group, obsolete);
                }
                state.stats.preparing = false;
                state.stats.reserved_work_bytes = stream.stats().cached_page_bytes;
                state = shared
                    .changed
                    .wait(state)
                    .map_err(|error| failed(error.to_string()))?;
            }
        };
        drop(obsolete); // Evicted buffers die before decode, outside metadata.
        let range = timeline.groups[group].frames.clone();
        let bytes = timeline.groups[group].bytes;
        let mut frames = Vec::with_capacity(range.len());
        let mut failure = None;
        for frame in range.clone() {
            let is_needed = {
                let state = shared
                    .state
                    .lock()
                    .map_err(|error| failed(error.to_string()))?;
                needed(&state, &timeline, group)
            };
            if !is_needed {
                break;
            }
            match stream.sample(timeline.start(frame)) {
                Ok(sample) => frames.push(sample),
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        let next = timeline.next(group);
        if next != group
            && stream.frames()[timeline.groups[next].frames.start].page_index()
                == stream.frames()[range.start].page_index()
        {
            stream.clear_frame(); // The next window needs this decoded atlas.
        } else {
            stream.clear_pixels();
        }
        let work = stream.stats();
        let batch = Batch {
            group,
            first: range.start,
            frames,
            bytes,
        };
        let mut state = shared
            .state
            .lock()
            .map_err(|error| failed(error.to_string()))?;
        state.stats.page_decodes = work.page_decodes;
        state.stats.frame_extracts = work.frame_extracts;
        state.stats.preparing = false;
        state.stats.reserved_work_bytes = work.cached_page_bytes;
        let outcome = completion(&mut state, &timeline, &batch, range.len(), failure);
        if outcome == Completion::Ready {
            state.batches.push(batch);
            state.stats.prepared_batches += 1;
            state.stats.cached_batches = state.batches.len();
            state.stats.cached_frame_bytes = state.batches.iter().map(|batch| batch.bytes).sum();
        }
        if outcome != Completion::Failed {
            let current = timeline.selection(state.time).0;
            state.stats.preparing = [current, timeline.next(current)]
                .into_iter()
                .any(|group| !state.batches.iter().any(|batch| batch.group == group));
        }
        drop(state);
        // Release canceled partial payloads outside metadata. A ready batch was moved.
        if outcome != Completion::Discarded {
            let _ = notify.try_send(()); // Full means a wakeup is already queued.
        }
        if outcome == Completion::Failed {
            return Ok(());
        }
    }
}

#[derive(PartialEq)]
enum Completion {
    Ready,
    Discarded,
    Failed,
}

// Demand relevance and terminal failure publication share the same metadata lock.
fn completion(
    state: &mut State,
    timeline: &Timeline,
    batch: &Batch,
    expected_frames: usize,
    failure: Option<InspectionError>,
) -> Completion {
    if !needed(state, timeline, batch.group) {
        state.stats.discarded_batches += 1;
        Completion::Discarded
    } else if let Some(error) = failure {
        state.failure = Some(error);
        state.failed = true;
        Completion::Failed
    } else if batch.frames.len() == expected_frames {
        Completion::Ready
    } else {
        state.stats.discarded_batches += 1;
        Completion::Discarded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_failure_is_terminal_only_if_its_group_is_still_needed() {
        let timeline = Timeline {
            ends: vec![1, 2, 3],
            groups: (0..3)
                .map(|index| Group {
                    frames: index..index + 1,
                    bytes: 0,
                    reservation: 0,
                })
                .collect(),
        };
        let batch = Batch {
            group: 0,
            first: 0,
            frames: Vec::new(),
            bytes: 0,
        };
        let mut state = State {
            time: Duration::from_nanos(1), // Seek to group one; zero is obsolete.
            timeline: None,
            batches: Vec::new(),
            failure: None,
            failed: false,
            stats: SpritePagingStats::default(),
            stopped: false,
        };
        assert!(
            completion(
                &mut state,
                &timeline,
                &batch,
                1,
                Some(failed("obsolete codec"))
            ) == Completion::Discarded
        );
        assert!(!state.failed && state.failure.is_none());
        assert_eq!(state.stats.discarded_batches, 1);
        state.time = Duration::ZERO;
        assert!(
            completion(
                &mut state,
                &timeline,
                &batch,
                1,
                Some(failed("needed codec"))
            ) == Completion::Failed
        );
        assert!(state.failed);
        assert!(
            state
                .failure
                .take()
                .expect("contextual error")
                .to_string()
                .contains("needed codec")
        );
        assert!(state.failed); // Consuming context cannot make a failed worker appear healthy.
        state.stopped = true;
        assert!(
            completion(
                &mut state,
                &timeline,
                &batch,
                1,
                Some(failed("stopped codec"))
            ) == Completion::Discarded
        );
        assert!(state.failure.is_none());
    }
}
