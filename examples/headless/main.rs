//! Headless consumer that verifies real blank GPU frames and their lifetime.

#[path = "../support/mod.rs"]
mod support;

use scene_renderer::{Frame, OutputStats, Renderer, Size};
use serde_json::json;
use std::sync::{
    Arc,
    mpsc::{Receiver, RecvTimeoutError},
};
use std::time::{Duration, Instant};
use support::Result;

fn wait_frame(renderer: &Renderer, events: &Receiver<()>, id: u64) -> Result<Arc<Frame>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(frame) = renderer.try_frame()?
            && frame.id() == id
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err("frame completion timed out".into());
        }
        match events.recv_timeout(Duration::from_millis(10)) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => (),
            Err(RecvTimeoutError::Disconnected) => {
                return Err("renderer worker disconnected".into());
            }
        }
    }
}

fn check_pixels(frame: &Frame) -> Result<scene_renderer::Pixels> {
    let pixels = frame.readback()?;
    let expected = [16, 24, 32, 255];
    if pixels.rgba.len() != frame.size().width() as usize * frame.size().height() as usize * 4
        || support::uniform_rgba(&pixels) != Some(expected)
    {
        return Err("GPU capture differs from the specified blank RGBA output".into());
    }
    Ok(pixels)
}

fn wait_stats(renderer: &Renderer, ready: impl Fn(OutputStats) -> bool) -> Result<OutputStats> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(stats) = renderer.try_output_stats()?
            && ready(stats)
        {
            return Ok(stats);
        }
        if Instant::now() >= deadline {
            return Err("pool observation timed out".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn main() -> Result<()> {
    let (output, mode) = support::options(std::env::args().skip(1))?;
    let started = Instant::now();
    let (renderer, events) = Renderer::new(Size::new(640, 360)?, mode)?;
    let first = wait_frame(&renderer, &events, 1)?;
    let cold_start_ms = started.elapsed().as_secs_f64() * 1000.0;
    support::write_png(&output.join("initial.png"), &check_pixels(&first)?)?;
    while events.try_recv().is_ok() {}
    if !matches!(
        events.recv_timeout(Duration::from_millis(250)),
        Err(RecvTimeoutError::Timeout)
    ) {
        return Err("idle renderer emitted unsolicited work".into());
    }
    if !Arc::ptr_eq(
        &first,
        &renderer.try_frame()?.ok_or("idle frame disappeared")?,
    ) {
        return Err("idle renderer replaced its output".into());
    }
    // Observe allocation reuse through the same public API on the selected
    // adapter. Keep one old lease while requesting 32 same-size blank frames.
    let mut warm_allocations = 0;
    for index in 0..32 {
        let id = renderer.resize(first.size())?;
        let frame = wait_frame(&renderer, &events, id)?;
        let stats = wait_stats(&renderer, |stats| {
            stats.published_frames == frame.sequence()
        })?;
        if stats.resident_textures > 3 || stats.resident_bytes > 192 * 1024 * 1024 {
            return Err("output pool exceeded its resource bound".into());
        }
        if index == 10 {
            warm_allocations = stats.texture_allocations;
        }
        if index > 10 && stats.texture_allocations != warm_allocations {
            return Err("same-size frames kept allocating output textures".into());
        }
    }
    let steady = wait_stats(&renderer, |_| true)?;
    let size = Size::new(317, 193)?;
    let id = renderer.resize(size)?;
    let resized = wait_frame(&renderer, &events, id)?;
    if resized.size() != size {
        return Err("resized output has incorrect dimensions".into());
    }
    support::write_png(&output.join("resized.png"), &check_pixels(&resized)?)?;
    let resized_info = support::frame_info(&resized);
    check_pixels(&first)?;
    if Size::new(0, 10).is_ok() || Size::new(16_384, 16_384).is_ok() {
        return Err("invalid dimensions were accepted".into());
    }
    let third_id = renderer.resize(size)?;
    let third = wait_frame(&renderer, &events, third_id)?;
    let pending = renderer.resize(size)?;
    let saturated = wait_stats(&renderer, |stats| stats.waiting_for_frame_release)?;
    if saturated.resident_textures != 3
        || renderer
            .try_frame()?
            .is_some_and(|frame| frame.id() != third_id)
    {
        return Err("pool saturation failed to preserve latest output".into());
    }
    drop(resized);
    let resumed = wait_frame(&renderer, &events, pending)?;
    let after_release = wait_stats(&renderer, |stats| {
        stats.published_frames == resumed.sequence()
    })?;
    let shutdown_start = Instant::now();
    renderer.shutdown()?;
    let shutdown_ms = shutdown_start.elapsed().as_secs_f64() * 1000.0;
    check_pixels(&first)?;
    check_pixels(&third)?;
    check_pixels(&resumed)?;
    let report = json!({
        "schema_version": 1, "status": "passed", "consumer": "headless", "pid": std::process::id(),
        "initial": support::frame_info(&first), "resized": resized_info,
        "checks": ["gpu-pixels", "idle-no-redraw", "resize", "retained-frame-after-resize",
            "invalid-size", "shutdown", "retained-frame-after-shutdown", "bounded-output-reuse",
            "pool-saturation", "resume-after-release"],
        "output_pool":{"steady":support::pool_info(steady),"saturated":support::pool_info(saturated),
            "after_release":support::pool_info(after_release),"steady_requests":32},
        "cold_start_ms": cold_start_ms, "shutdown_ms": shutdown_ms,
        "capture_readback": true, "limits": ["blank output only", "no scene or host latency measurements"],
    });
    support::write_json(&output.join("report.json"), &report)?;
    println!("{report}");
    Ok(())
}
