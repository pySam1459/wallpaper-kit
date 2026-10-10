use serde_json::{Value, json};
use std::time::{Duration, Instant};
use wallpaper_kit::Frame;

// Consumer-owned diagnostic clock. There is no timer or unbounded history in
// the library; live playback requests explicit times through the public API.
pub struct Fixture {
    active: bool,
    base: Duration,
    started: Option<Instant>,
    next_tick: Instant,
    origin: Instant,
    request: Option<(u64, f64)>,
    samples: Vec<Value>,
    omitted: u64,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            active: false,
            base: Duration::ZERO,
            started: None,
            next_tick: Instant::now(),
            origin: Instant::now(),
            request: None,
            samples: Vec::new(),
            omitted: 0,
        }
    }

    pub fn step(&mut self, time: Duration) {
        self.active = true;
        self.base = time;
        self.started = None;
    }

    pub fn play(&mut self) {
        self.active = true;
        if self.started.is_none() {
            self.started = Some(Instant::now());
            self.next_tick = Instant::now();
        }
    }

    pub fn pause(&mut self) {
        if let Some(started) = self.started.take() {
            self.base += started.elapsed();
        }
    }

    pub fn running(&self) -> bool {
        self.started.is_some()
    }

    pub fn tick(&mut self) -> Option<Duration> {
        let started = self.started?;
        if Instant::now() < self.next_tick {
            return None;
        }
        self.next_tick = Instant::now() + Duration::from_secs_f64(1.0 / 30.0);
        Some(self.base + started.elapsed())
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.started.map(|_| self.next_tick)
    }

    pub fn requested(&mut self, id: u64) {
        self.request = Some((id, self.origin.elapsed().as_secs_f64() * 1000.0));
    }

    pub fn presented(&mut self, frame: &Frame) {
        let Some(time) = frame.fixture_time() else {
            return;
        };
        if self.samples.len() == 2_048 {
            self.omitted += 1;
            return;
        }
        self.samples.push(
            json!({"frame_id":frame.id(),"playback_ms":time.as_secs_f64()*1000.0,
            "requested_ms":self.request.filter(|(id,_)|*id==frame.id()).map(|(_,t)|t),
            "present_call_return_ms":self.origin.elapsed().as_secs_f64()*1000.0,
            "mode":if self.running(){"live"}else{"controlled"}}),
        );
    }

    pub fn report(&self) -> Value {
        json!({"schema_version":1,"active":self.active,"running":self.running(),
            "samples":self.samples,"omitted_samples":self.omitted,"sample_cap":2048,
            "limits":["Present return is not display time.","Controlled captures include readback overhead.",
                "Native live samples exclude explicit captures; no GPU durations or host latency measured."]})
    }
}
