//! Host scheduling and observations; time and composition belong to the library.

use crate::support::Result;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use wallpaper_kit::{Frame, ImagePlayback, ImagePlaybackSample};

struct Selection {
    id: u64,
    time: Duration,
    indices: Vec<usize>,
}

pub struct Images {
    playback: ImagePlayback,
    pending: bool,
    next_tick: Option<Instant>,
    requested: Option<Selection>,
    latest: Option<Selection>,
    origin: Instant,
    observations: Vec<Value>,
    omitted: u64,
    pub fixture: bool,
}
impl Images {
    pub fn new(playback: ImagePlayback, fixture: bool) -> Self {
        Self {
            playback,
            pending: true,
            next_tick: Some(Instant::now()),
            requested: None,
            latest: None,
            origin: Instant::now(),
            observations: Vec::new(),
            omitted: 0,
            fixture,
        }
    }
    pub fn play(&mut self) {
        self.playback.play();
        self.next_tick = Some(Instant::now());
    }
    pub fn playing(&self) -> bool {
        self.playback.playing()
    }
    pub fn pause(&mut self) {
        self.playback.pause();
        if !self.pending {
            self.next_tick = None;
        }
    }
    pub fn seek(&mut self, time: Duration) {
        self.playback.seek(time);
        self.pending = true;
        self.next_tick = Some(Instant::now());
    }
    pub fn tick(&mut self) -> Result<Option<ImagePlaybackSample>> {
        if self
            .next_tick
            .is_none_or(|deadline| Instant::now() < deadline)
        {
            return Ok(None);
        }
        let sample = self.playback.try_sample()?;
        self.pending = sample.is_none();
        self.next_tick = if self.pending {
            Some(Instant::now() + Duration::from_millis(10))
        } else if self.playback.playing() {
            Some(Instant::now() + Duration::from_nanos(1_000_000_000 / 30))
        } else {
            None
        };
        Ok(sample)
    }
    pub fn requested(&mut self, id: u64, sample: &ImagePlaybackSample) {
        self.requested = Some(Selection {
            id,
            time: sample.time,
            indices: sample.frame_indices.clone(),
        });
    }
    pub fn resized(&mut self, id: u64) {
        if let Some(request) = &mut self.requested {
            request.id = id;
        }
    }
    pub fn presented(&mut self, frame: &Frame) {
        if let Some(request) = &self.requested
            && request.id == frame.id()
        {
            self.latest = Some(Selection {
                id: request.id,
                time: request.time,
                indices: request.indices.clone(),
            });
            if self.observations.len() < 2048 {
                self.observations.push(json!({"time_ns":request.time.as_nanos().to_string(),"frame_indices":request.indices,"frame_id":frame.id(),"sequence":frame.sequence(),"present_call_return_ms":self.origin.elapsed().as_secs_f64()*1000.}));
            } else {
                self.omitted += 1;
            }
        }
    }
    pub fn presented_at(&self, time: Duration) -> bool {
        !self.pending
            && self
                .latest
                .as_ref()
                .is_some_and(|sample| sample.time == time)
    }
    pub fn capture_time_ms(&self) -> Result<u64> {
        Ok(u64::try_from(
            self.latest
                .as_ref()
                .ok_or("no presented image sample")?
                .time
                .as_millis(),
        )?)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.next_tick
    }
    pub fn inspect(&self) -> Value {
        json!({"playing":self.playback.playing(),"clock_ms":self.playback.time().as_secs_f64()*1000.,"pending_sample":self.pending,"report":self.playback.report(),"presented_sample":self.latest.as_ref().map(|sample|json!({"time_ns":sample.time.as_nanos().to_string(),"frame_indices":sample.indices,"frame_id":sample.id}))})
    }
    pub fn report(&self) -> Value {
        json!({"observations":self.observations,"omitted":self.omitted,"limits":["Presentation-call return times, not GPU duration or displayed-frame timing.","Selected image playback only; omissions are recorded in image-playback.json."]})
    }
    pub fn shutdown(self) -> Result<()> {
        Ok(self.playback.shutdown()?)
    }
}
