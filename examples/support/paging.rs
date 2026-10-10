//! Bounded diagnostic waits, never a host UI polling policy.

use scene_renderer::{SpritePager, SpritePagingStats, SpriteSample};
use serde_json::{Value, json};
use std::{
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn sample(pager: &SpritePager, events: &Receiver<()>, time: Duration) -> Result<SpriteSample> {
    let started = Instant::now();
    loop {
        if let Some(sample) = pager.try_sample_at(time)? {
            return Ok(sample);
        }
        if started.elapsed() > Duration::from_secs(15) {
            return Err("sprite batch timed out".into());
        }
        let _ = events.recv_timeout(Duration::from_millis(2));
    }
}

pub fn settled(
    pager: &SpritePager,
    events: &Receiver<()>,
    batches: usize,
) -> Result<SpritePagingStats> {
    let started = Instant::now();
    loop {
        if let Some(stats) = pager.try_stats()?
            && !stats.preparing
            && stats.cached_batches == batches
        {
            return Ok(stats);
        }
        if started.elapsed() > Duration::from_secs(15) {
            return Err("sprite prefetch did not settle".into());
        }
        let _ = events.recv_timeout(Duration::from_millis(2));
    }
}

pub fn stats(value: SpritePagingStats) -> Value {
    json!({"prepared_batches":value.prepared_batches,"discarded_batches":value.discarded_batches,"page_decodes":value.page_decodes,"frame_extracts":value.frame_extracts,"cached_batches":value.cached_batches,"cached_frame_bytes":value.cached_frame_bytes,"reserved_work_bytes":value.reserved_work_bytes,"peak_admitted_bytes":value.peak_admitted_bytes,"preparing":value.preparing,"unavailable_polls":value.unavailable_polls})
}
