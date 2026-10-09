//! Deterministic GPU motion captures and separate readback-free live telemetry.

#[path = "../support/motion.rs"]
mod motion_checks;
#[path = "../support/mod.rs"]
mod support;
mod timing;

use scene_renderer::{Frame, Pixels, Renderer, Size};
use serde_json::json;
use std::{
    sync::{Arc, mpsc::Receiver},
    time::{Duration, Instant},
};
use support::Result;

fn wait_frame(renderer: &Renderer, events: &Receiver<()>, id: u64) -> Result<(Arc<Frame>, f64)> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut max_call_ms = 0.0_f64;
    loop {
        let call_start = Instant::now();
        let frame = renderer.try_frame()?;
        max_call_ms = max_call_ms.max(call_start.elapsed().as_secs_f64() * 1000.0);
        if let Some(frame) = frame
            && frame.id() == id
        {
            return Ok((frame, max_call_ms));
        }
        if Instant::now() >= deadline {
            return Err("motion frame completion timed out".into());
        }
        let _ = events.recv_timeout(Duration::from_millis(1));
    }
}

fn main() -> Result<()> {
    let (output, mode) = support::options()?;
    let size = Size::new(320, 180)?;
    let (renderer, events) = Renderer::new(size, mode)?;
    let (initial, _) = wait_frame(&renderer, &events, 1)?;
    if support::uniform_rgba(&initial.readback()?) != Some([16, 24, 32, 255]) {
        return Err("blank startup differs".into());
    }
    let times = [0, 49, 50, 500, 1_000, 1_950, 1_999, 2_000, 2_050, 4_000];
    let mut sheet = Pixels {
        size: Size::new(1_600, 360)?,
        rgba: vec![0; 1_600 * 360 * 4],
    };
    let mut checkpoints = Vec::new();
    let mut frozen = None;
    let mut mistimed = None;
    let mut end_of_loop = None;
    let mut verifier_failures = Vec::new();
    for (index, time) in times.into_iter().enumerate() {
        let id = renderer.render_fixture_at(Duration::from_millis(time))?;
        let (frame, _) = wait_frame(&renderer, &events, id)?;
        if frame.fixture_time() != Some(Duration::from_millis(time)) {
            return Err("fixture frame reports the wrong playback time".into());
        }
        let pixels = frame.readback()?;
        let name = format!("frame-{time:04}.png");
        support::write_png(&output.join(&name), &pixels)?;
        let check = motion_checks::check_pixels(&pixels, time);
        checkpoints.push(
            json!({"time_ms":time,"capture":name,"frame":support::frame_info(&frame),
            "status":if check.is_ok(){"passed"}else{"failed"},"error":check.as_ref().err()}),
        );
        support::write_json(&output.join("checkpoints.json"), &json!(checkpoints))?;
        check?;
        for row in 0..180 {
            let dst = (((index / 5) * 180 + row) * 1_600 + (index % 5) * 320) * 4;
            sheet.rgba[dst..dst + 320 * 4]
                .copy_from_slice(&pixels.rgba[row * 320 * 4..(row + 1) * 320 * 4]);
        }
        if time == 0 {
            frozen = Some(pixels.rgba.clone());
        }
        if time == 500 {
            mistimed = Some(pixels.rgba.clone());
        }
        if time == 1_999 {
            end_of_loop = Some(pixels.rgba.clone());
        }
    }
    for (name, rgba, source_ms, expected_ms) in [
        (
            "frozen-motion",
            frozen.ok_or("missing freeze capture")?,
            0,
            500,
        ),
        (
            "wrong-speed",
            mistimed.ok_or("missing timing capture")?,
            500,
            1_000,
        ),
        (
            "broken-loop",
            end_of_loop.ok_or("missing loop capture")?,
            1_999,
            2_000,
        ),
    ] {
        let pixels = Pixels { size, rgba };
        let error = motion_checks::check_pixels(&pixels, expected_ms)
            .err()
            .ok_or("motion verifier accepted injected fault")?;
        verifier_failures.push(json!({"fault":name,"detected":true,"diagnostic":error,
            "source_capture":format!("frame-{source_ms:04}.png"),"expected_time_ms":expected_ms}));
    }
    if !timing::stall_self_test() {
        return Err("timing verifier accepted injected stall".into());
    }
    verifier_failures
        .push(json!({"fault":"120-ms-stall","detected":true,"kind":"synthetic-timeline"}));
    support::write_json(
        &output.join("verifier-self-tests.json"),
        &json!(verifier_failures),
    )?;
    support::write_png(&output.join("contact-sheet.png"), &sheet)?;
    let cards: String = times.iter().map(|t|format!("<figure><figcaption>{t} ms</figcaption><img src=\"frame-{t:04}.png\" width=320 height=180></figure>")).collect();
    std::fs::write(
        output.join("index.html"),
        format!(
            "<!doctype html><meta charset=utf-8><title>Motion fixture evidence</title><style>body{{background:#101820;color:white;font:16px sans-serif}}main{{display:flex;flex-wrap:wrap}}figure{{margin:8px}}</style><h1>Controlled playback checkpoints</h1><p>Two-second loop, 50 ms steps. Live timing is reported separately.</p><main>{cards}</main>"
        ),
    )?;

    // Resize preserves controlled content, including its playback time.
    let resized_id = renderer.resize(Size::new(317, 193)?)?;
    let (resized, _) = wait_frame(&renderer, &events, resized_id)?;
    if resized.fixture_time() != Some(Duration::from_millis(4_000)) {
        return Err("resize lost fixture playback time".into());
    }
    let resized_pixels = resized.readback()?;
    support::write_png(&output.join("resized.png"), &resized_pixels)?;
    motion_checks::check_pixels(&resized_pixels, 4_000)?;
    let restored_id = renderer.resize(size)?;
    let _ = wait_frame(&renderer, &events, restored_id)?;

    // Warmup and deterministic captures are excluded from this live run.
    // Absolute slots prevent an expensive frame from slowing the playback clock.
    let period = Duration::from_secs_f64(1.0 / 30.0);
    let slots = 125_u32;
    let start = Instant::now();
    let mut samples = Vec::with_capacity(slots as usize);
    for slot in 0..slots {
        let due = start + period * slot;
        if let Some(delay) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(delay);
        }
        if Instant::now() >= due + period {
            continue;
        }
        let playback = start.elapsed();
        let id = renderer.render_fixture_at(playback)?;
        let requested_ms = playback.as_secs_f64() * 1000.0;
        let (_, acquisition_ms) = wait_frame(&renderer, &events, id)?;
        samples.push(timing::Sample {
            slot,
            frame_id: id,
            playback_ms: requested_ms,
            requested_ms,
            ready_ms: start.elapsed().as_secs_f64() * 1000.0,
            acquisition_ms,
        });
    }
    if samples.is_empty() {
        return Err("no live frames observed".into());
    }
    let metrics = timing::summary(&samples, period.as_secs_f64() * 1000.0, slots);
    support::write_json(
        &output.join("timeline.json"),
        &json!(samples.iter().map(timing::Sample::json).collect::<Vec<_>>()),
    )?;
    renderer.shutdown()?;
    let report = json!({"schema_version":1,"status":"passed","consumer":"motion-checks",
        "adapter":support::frame_info(&initial)["adapter"],"deterministic_checkpoints":checkpoints.len(),
        "resized_fixture_pixels":"passed", "correctness":"passed",
        "verifier_self_tests":verifier_failures,"live_timing":metrics,"capture_during_live_timing":false,
        "limits":["Diagnostic fixture; no Workshop or video support.",
            "Ready times are consumer observations, not GPU durations or display times.",
            "Timing is diagnostic, not a production performance acceptance score.",
            "Immutable texture allocation per request; no bounded animation frame pool.",
            "No host input latency or memory plateau measurement."]});
    support::write_json(&output.join("report.json"), &report)?;
    println!("{report}");
    Ok(())
}
