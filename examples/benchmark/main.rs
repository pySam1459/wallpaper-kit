//! Readback-free performance consumer. OS telemetry belongs to the controller.
mod metrics;
#[allow(dead_code)]
#[path = "../support/mod.rs"]
mod support;
#[allow(dead_code)]
#[path = "../support/texture.rs"]
mod texture_checks;

use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, mpsc::Receiver},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use support::Result;
use wallpaper_kit::{
    DeviceMode, Frame, ImagePlayback, ImageQuality, PlaybackSettings, Renderer, SceneSource, Size,
};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Source {
    Blank,
    Static,
    Scene {
        path: PathBuf,
        engine_assets: PathBuf,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    source: Source,
    width: u32,
    height: u32,
    warmup_seconds: u32,
    measure_seconds: u32,
    idle_seconds: u32,
    target_fps: u32,
    software: bool,
    #[serde(default)]
    profile_gpu: bool,
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64()
        * 1000.
}

fn event(phase: &str, kind: &str) -> Result<f64> {
    let now = unix_ms();
    println!("{}", json!({"event":kind,"phase":phase,"unix_ms":now}));
    io::stdout().flush()?;
    Ok(now)
}

fn idle(name: &str, seconds: u32) -> Result<Value> {
    let start = event(name, "begin")?;
    std::thread::sleep(Duration::from_secs(u64::from(seconds)));
    Ok(json!({"name":name,"start_unix_ms":start,"end_unix_ms":event(name,"end")?}))
}

fn wait(renderer: &Renderer, events: &Receiver<()>, id: u64) -> Result<Arc<Frame>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(frame) = renderer.try_frame()?
            && frame.id() == id
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err("GPU completion exceeded 30 seconds".into());
        }
        events.recv_timeout(Duration::from_millis(10)).ok();
    }
}

fn static_source(output: &Path) -> Result<SceneSource> {
    let root = output.join("static-input");
    fs::create_dir(&root)?;
    fs::create_dir(root.join("engine"))?;
    let scene = serde_json::to_vec(
        &json!({"version":5,"general":{"orthogonalprojection":{"width":2,"height":2}},
        "objects":[{"id":1,"image":"models/image.json","origin":"1 1 0","size":"2 2"}]}),
    )?;
    let model = serde_json::to_vec(&json!({"autosize":true,"material":"materials/image.json"}))?;
    let material = serde_json::to_vec(
        &json!({"passes":[{"shader":"genericimage2","textures":["image"],"combos":{},
        "blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled"}]}),
    )?;
    let image = texture_checks::tex(
        0,
        [2, 2],
        &[
            200, 50, 30, 255, 30, 180, 70, 255, 20, 60, 220, 255, 220, 190, 50, 255,
        ],
        false,
        false,
    );
    let package = root.join("scene.pkg");
    fs::write(
        &package,
        texture_checks::package(&[
            ("scene.json", &scene),
            ("models/image.json", &model),
            ("materials/image.json", &material),
            ("materials/image.tex", &image),
        ]),
    )?;
    Ok(SceneSource {
        package,
        engine_assets: root.join("engine"),
    })
}

#[derive(Clone, Copy)]
struct Sample {
    slot: u64,
    requested_ms: f64,
    ready_ms: f64,
    sample_wait_ms: f64,
    submit_to_ready_ms: f64,
    source_age_ms: f64,
}

struct Consumer {
    renderer: Renderer,
    events: Receiver<()>,
    playback: Option<ImagePlayback>,
    size: Size,
}

struct Draw {
    frame: Arc<Frame>,
    sample_wait_ms: f64,
    submit_to_ready_ms: f64,
    source_age_ms: f64,
    indices: Option<Vec<usize>>,
}

impl Consumer {
    fn draw(&self) -> Result<Draw> {
        let started = Instant::now();
        let (id, source_age, indices) = if let Some(playback) = &self.playback {
            let sample = loop {
                if let Some(sample) = playback.try_sample()? {
                    break sample;
                }
                if started.elapsed() > Duration::from_secs(30) {
                    return Err("source readiness exceeded 30 seconds".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            let age = playback.time().saturating_sub(sample.time).as_secs_f64() * 1000.;
            let id = self.renderer.render_images(sample.composition)?;
            (id, age, Some(sample.frame_indices))
        } else {
            (self.renderer.resize(self.size)?, 0., None)
        };
        let sampled = Instant::now();
        let frame = wait(&self.renderer, &self.events, id)?;
        Ok(Draw {
            frame,
            sample_wait_ms: (sampled - started).as_secs_f64() * 1000.,
            submit_to_ready_ms: sampled.elapsed().as_secs_f64() * 1000.,
            source_age_ms: source_age,
            indices,
        })
    }

    fn warmup(&mut self, seconds: u32) -> Result<()> {
        if let Some(playback) = &mut self.playback {
            playback.pause();
            playback.seek(Duration::ZERO);
            playback.play();
        }
        let until = Instant::now() + Duration::from_secs(u64::from(seconds));
        while Instant::now() < until {
            drop(self.draw()?);
        }
        Ok(())
    }

    fn measure(
        &mut self,
        output: &Path,
        name: &str,
        config: &Config,
        paced: bool,
    ) -> Result<Value> {
        self.warmup(config.warmup_seconds)?;
        let before = self
            .renderer
            .try_output_stats()?
            .ok_or("pool unavailable")?;
        let mut samples = Vec::with_capacity(120_000);
        let mut previous_indices = None;
        let (mut source_selections, mut changed_selections) = (0, 0);
        let start_unix = event(name, "begin")?;
        let start = Instant::now();
        let duration = Duration::from_secs(u64::from(config.measure_seconds));
        let period = 1. / f64::from(config.target_fps);
        let mut slot = 0_u64;
        while start.elapsed() < duration {
            if paced {
                let due = Duration::from_secs_f64(slot as f64 * period);
                if let Some(remaining) = due.checked_sub(start.elapsed()) {
                    std::thread::sleep(remaining);
                }
                slot = slot.max((start.elapsed().as_secs_f64() / period).floor() as u64);
                if start.elapsed() >= duration {
                    break;
                }
            }
            let requested = start.elapsed().as_secs_f64() * 1000.;
            let Draw {
                frame,
                sample_wait_ms,
                submit_to_ready_ms,
                source_age_ms,
                indices,
            } = self.draw()?;
            drop(frame);
            if let Some(indices) = indices.filter(|v| !v.is_empty()) {
                source_selections += 1;
                if previous_indices.as_ref() != Some(&indices) {
                    changed_selections += 1;
                }
                previous_indices = Some(indices);
            }
            if samples.len() == samples.capacity() {
                return Err("120,000-frame telemetry bound reached; shorten the interval".into());
            }
            samples.push(Sample {
                slot,
                requested_ms: requested,
                ready_ms: start.elapsed().as_secs_f64() * 1000.,
                sample_wait_ms,
                submit_to_ready_ms,
                source_age_ms,
            });
            slot += 1;
        }
        let elapsed = start.elapsed().as_secs_f64();
        let end_unix = event(name, "end")?;
        if samples.len() < 2 {
            return Err("fewer than two completed benchmark frames".into());
        }
        let after = self
            .renderer
            .try_output_stats()?
            .ok_or("pool unavailable")?;
        if after.resident_textures > 3 {
            return Err("output pool exceeded three textures".into());
        }
        let mut csv =
            io::BufWriter::new(fs::File::create(output.join(format!("{name}-frames.csv")))?);
        writeln!(
            csv,
            "slot,requested_ms,ready_ms,sample_wait_ms,submit_to_ready_ms,source_age_ms"
        )?;
        for s in &samples {
            writeln!(
                csv,
                "{},{:.6},{:.6},{:.6},{:.6},{:.6}",
                s.slot,
                s.requested_ms,
                s.ready_ms,
                s.sample_wait_ms,
                s.submit_to_ready_ms,
                s.source_age_ms
            )?;
        }
        let gaps: Vec<_> = samples
            .windows(2)
            .map(|s| s[1].ready_ms - s[0].ready_ms)
            .collect();
        Ok(
            json!({"name":name,"start_unix_ms":start_unix,"end_unix_ms":end_unix,"elapsed_seconds":elapsed,
            "target_fps":paced.then_some(config.target_fps),"completed_frames":samples.len(),"completed_fps":samples.len() as f64/elapsed,
            "skipped_slots":if paced {(u64::from(config.measure_seconds)*u64::from(config.target_fps)).saturating_sub(samples.len() as u64)}else{0},
            "deadline_misses":if paced {samples.iter().filter(|s|s.ready_ms>(s.slot+1) as f64*period*1000.).count()}else{0},
            "ready_gap_ms":metrics::distribution(&gaps),
            "sample_to_ready_ms":metrics::distribution(&samples.iter().map(|s|s.ready_ms-s.requested_ms).collect::<Vec<_>>()),
            "sample_wait_ms":metrics::distribution(&samples.iter().map(|s|s.sample_wait_ms).collect::<Vec<_>>()),
            "submit_to_ready_ms":metrics::distribution(&samples.iter().map(|s|s.submit_to_ready_ms).collect::<Vec<_>>()),
            "source_age_ms":metrics::distribution(&samples.iter().map(|s|s.source_age_ms).collect::<Vec<_>>()),
            "source_index_observations":source_selections,"changed_source_index_selections":changed_selections,
            "telemetry_buffer_capacity_bytes":samples.capacity()*std::mem::size_of::<Sample>(),
            "pool_before":support::pool_info(before),"pool_after":support::pool_info(after)}),
        )
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let config_path = PathBuf::from(args.next().ok_or("missing config path")?);
    let output = PathBuf::from(args.next().ok_or("missing output path")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let config: Config = serde_json::from_slice(&fs::read(config_path)?)?;
    if !(1..=60).contains(&config.warmup_seconds)
        || !(1..=120).contains(&config.measure_seconds)
        || !(1..=60).contains(&config.idle_seconds)
        || !(1..=240).contains(&config.target_fps)
    {
        return Err("benchmark interval or frame-rate bounds exceeded".into());
    }
    let size = Size::new(config.width, config.height)?;
    fs::create_dir(&output)?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    if line.trim() != "start" {
        return Err("controller must assign the process job before start".into());
    }
    let mut phases = vec![idle("disabled", config.idle_seconds)?];
    let startup = Instant::now();
    let create = if config.profile_gpu {
        Renderer::new_profiled
    } else {
        Renderer::new
    };
    let (renderer, events) = create(
        size,
        if config.software {
            DeviceMode::Software
        } else {
            DeviceMode::Hardware
        },
    )?;
    let initial = wait(&renderer, &events, 1)?;
    let device_ms = startup.elapsed().as_secs_f64() * 1000.;
    drop(initial);
    let settings = PlaybackSettings {
        allow_unsupported_features: true,
        image_quality: ImageQuality::Linear,
        ..Default::default()
    };
    let loading = Instant::now();
    let source = match &config.source {
        Source::Blank => None,
        Source::Static => Some(static_source(&output)?),
        Source::Scene {
            path,
            engine_assets,
        } => Some(SceneSource {
            package: path.clone(),
            engine_assets: engine_assets.clone(),
        }),
    };
    let playback = source
        .as_ref()
        .map(|source| {
            if source
                .package
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pkg"))
            {
                ImagePlayback::open_scene_images_with_settings(source, settings)
            } else {
                ImagePlayback::open_video(&source.package, size, settings)
            }
        })
        .transpose()?;
    let load_ms = loading.elapsed().as_secs_f64() * 1000.;
    let mut consumer = Consumer {
        renderer,
        events,
        playback,
        size,
    };
    if let Some(p) = &mut consumer.playback {
        p.set_calendar_time(UNIX_EPOCH + Duration::from_secs(1_600_000_000));
    }
    let first = consumer.draw()?.frame;
    let first_content_ms = startup.elapsed().as_secs_f64() * 1000.;
    let info = support::frame_info(&first);
    let pixels = first.readback()?;
    if consumer.playback.is_some() && first.image_layer_count().is_none_or(|n| n == 0) {
        return Err("empty scene benchmark".into());
    }
    support::write_png(&output.join("initial.png"), &pixels)?;
    drop(pixels);
    drop(first);
    consumer.warmup(config.warmup_seconds)?;
    if let Some(p) = &mut consumer.playback {
        p.pause();
    }
    phases.push(idle("paused", config.idle_seconds)?);
    phases.push(consumer.measure(&output, "paced", &config, true)?);
    phases.push(consumer.measure(&output, "uncapped", &config, false)?);
    if let Some(p) = &mut consumer.playback {
        p.pause();
    }
    let frozen = consumer
        .renderer
        .try_frame()?
        .ok_or("missing retained frame")?
        .id();
    phases.push(idle("paused_after", config.idle_seconds)?);
    if consumer
        .renderer
        .try_frame()?
        .ok_or("missing paused frame")?
        .id()
        != frozen
    {
        return Err("paused renderer produced unsolicited frames".into());
    }
    let report = consumer
        .playback
        .as_ref()
        .map(|p| serde_json::to_value(p.report()))
        .transpose()?;
    let shutdown = Instant::now();
    consumer.renderer.shutdown()?;
    if let Some(p) = consumer.playback {
        p.shutdown()?;
    }
    let shutdown_ms = shutdown.elapsed().as_secs_f64() * 1000.;
    phases.push(idle("released", config.idle_seconds)?);
    support::write_json(
        &output.join("report.json"),
        &json!({"schema_version":1,"status":"passed","frame":info,
        "device_start_ms":device_ms,"asset_open_ms":load_ms,"first_content_ms":first_content_ms,"shutdown_ms":shutdown_ms,
        "playback":report,"phases":phases,"capture_during_measurement":false,
        "limits":["One request in flight through the public API; completed native render throughput, not displayed FPS or pure GPU duration.",
            "Initial readback/PNG and warmed setup excluded from timed phases; no window, browser, transfer or host UI.",
            "Static and blank active phases deliberately force renders. Actual static consumers should remain paused.",
            "Per-frame telemetry is bounded to 120,000 entries and written after the timed interval."]}),
    )?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
