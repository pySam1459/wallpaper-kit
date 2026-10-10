//! Bounded diagnostic waiting; production hosts poll from their own event loop.

use scene_renderer::{SpriteResult, SpriteWorker};
use serde_json::{Value, json};
use std::{
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn request(worker: &SpriteWorker, time: Duration) -> Result<u64> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(id) = worker.try_request_at(time)? {
            return Ok(id);
        }
        if Instant::now() >= deadline {
            return Err("sprite request stayed busy".into());
        }
        std::thread::yield_now();
    }
}

pub fn wait(
    worker: &SpriteWorker,
    events: &Receiver<()>,
    id: u64,
) -> Result<(SpriteResult, Value)> {
    let start = Instant::now();
    let mut polls = 0;
    let mut max_poll_ms = 0_f64;
    loop {
        let poll_start = Instant::now();
        let result = worker.try_take()?;
        max_poll_ms = max_poll_ms.max(poll_start.elapsed().as_secs_f64() * 1000.);
        polls += 1;
        if let Some(result) = result {
            if result.request_id() != id {
                return Err("superseded sprite result published".into());
            }
            let stats = result.stats();
            return Ok((
                result,
                json!({"request_id":id,"wait_ms":start.elapsed().as_secs_f64()*1000.,"polls":polls,"max_result_poll_ms":max_poll_ms,"page_decodes":stats.page_decodes,"frame_extracts":stats.frame_extracts,"cached_page_bytes":stats.cached_page_bytes,"cached_frame_bytes":stats.cached_frame_bytes}),
            ));
        }
        if start.elapsed() >= Duration::from_secs(15) {
            return Err("sprite preparation timed out".into());
        }
        let _ = events.recv_timeout(Duration::from_millis(2));
    }
}
