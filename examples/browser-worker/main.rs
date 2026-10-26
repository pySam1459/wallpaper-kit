//! Headless debug transport for browser verification; rendering stays in the library.

#[allow(dead_code)]
#[path = "../support/image_playback.rs"]
mod image_fixture;
#[allow(dead_code)]
#[path = "../support/texture.rs"]
mod texture_checks;

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::{Duration, Instant, SystemTime},
};
use wallpaper_kit::{
    DeviceMode, Frame, ImagePlayback, ImagePlaybackSample, ImageQuality, PlaybackSettings,
    Renderer, SceneSource, Size,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    command: Command,
}

#[derive(Deserialize)]
#[serde(tag = "name", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Inspect,
    Frame,
    Play,
    Pause,
    Seek { time_ms: u64 },
    Resize { width: u32, height: u32 },
    SetProperties { properties: BTreeMap<String, Value> },
    SetCalendarTime { unix_ms: u64 },
    Stop,
}

struct Viewer {
    source: SceneSource,
    calendar_time: Option<SystemTime>,
    playback: ImagePlayback,
    renderer: Renderer,
    events: Receiver<()>,
    size: Size,
    fixture: bool,
    last_sample: Option<ImagePlaybackSample>,
    last_frame: Option<Value>,
}

impl Viewer {
    fn open() -> Result<Self> {
        let mut scene = None;
        let mut engine = None;
        let mut output = None;
        let mut mode = DeviceMode::Hardware;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--scene" => scene = Some(PathBuf::from(args.next().ok_or("missing scene path")?)),
                "--engine-assets" => {
                    engine = Some(PathBuf::from(args.next().ok_or("missing engine path")?))
                }
                "--output" => {
                    output = Some(PathBuf::from(args.next().ok_or("missing output path")?))
                }
                "--software" => mode = DeviceMode::Software,
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }
        let output = output.ok_or("--output is required")?;
        fs::create_dir(&output)?;
        let fixture = scene.is_none();
        let source = if let Some(package) = scene {
            SceneSource {
                package,
                engine_assets: engine.ok_or("--scene requires --engine-assets")?,
            }
        } else {
            let package = output.join("fixture.pkg");
            fs::write(&package, image_fixture::package())?;
            let engine_assets = output.join("engine");
            fs::create_dir(&engine_assets)?;
            SceneSource {
                package,
                engine_assets,
            }
        };
        let playback = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                allow_unsupported_features: true,
                image_quality: if fixture {
                    ImageQuality::Nearest
                } else {
                    ImageQuality::Linear
                },
                ..PlaybackSettings::default()
            },
        )?;
        let size = Size::new(640, 360)?;
        let (renderer, events) = Renderer::new(size, mode)?;
        Ok(Self {
            source,
            calendar_time: None,
            playback,
            renderer,
            events,
            size,
            fixture,
            last_sample: None,
            last_frame: None,
        })
    }

    fn state(&self) -> Result<Value> {
        let pool = self.renderer.try_output_stats()?.map(|s| json!({
            "resident_textures": s.resident_textures, "resident_bytes": s.resident_bytes,
            "published_frames": s.published_frames, "texture_uploads": s.texture_uploads,
            "source_texture_bytes": s.source_texture_bytes, "effect_texture_bytes": s.effect_texture_bytes,
        }));
        Ok(json!({
            "ready": true, "fixture": self.fixture, "playing": self.playback.playing(),
            "time_ms": self.playback.time().as_secs_f64() * 1000.,
            "width": self.size.width(), "height": self.size.height(),
            "report": self.playback.report(), "frame": self.last_frame, "pool": pool,
        }))
    }

    fn frame(&mut self) -> Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let sample = loop {
            if let Some(sample) = self.playback.try_sample()? {
                break sample;
            }
            if Instant::now() >= deadline {
                return Err("scene sample timed out".into());
            }
            std::thread::sleep(Duration::from_millis(4));
        };
        let id = self.renderer.render_images(sample.composition.clone())?;
        let frame = loop {
            if let Some(frame) = self.renderer.try_frame()?
                && frame.id() == id
            {
                break frame;
            }
            if Instant::now() >= deadline {
                return Err("GPU frame timed out".into());
            }
            match self.events.recv_timeout(Duration::from_millis(10)) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => (),
                Err(RecvTimeoutError::Disconnected) => return Err("renderer disconnected".into()),
            }
        };
        let pixels = frame.readback()?;
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, self.size.width(), self.size.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(&pixels.rgba)?;
        }
        self.last_frame = Some(frame_info(&frame, &sample, &pixels.rgba));
        self.last_sample = Some(sample);
        if !self.playback.playing() {
            self.playback
                .freeze(self.last_sample.as_ref().expect("captured sample"));
            self.renderer.trim_memory()?;
        }
        Ok(png)
    }

    fn command(&mut self, command: &Command) -> Result<Vec<u8>> {
        match command {
            Command::Frame => return self.frame(),
            Command::Inspect | Command::Stop => (),
            Command::Play => self.playback.play(),
            Command::Pause => {
                if let Some(sample) = &self.last_sample {
                    self.playback.freeze(sample);
                } else {
                    self.playback.pause();
                }
                self.renderer.trim_memory()?;
            }
            Command::Seek { time_ms } => {
                if *time_ms > 86_400_000 {
                    return Err("seek must be within one day".into());
                }
                self.playback.seek(Duration::from_millis(*time_ms));
            }
            Command::Resize { width, height } => {
                let size = Size::new(*width, *height)?;
                self.renderer.resize(size)?;
                self.size = size;
            }
            Command::SetProperties { properties } => {
                let mut merged = self.playback.report().user_properties.clone();
                merged.extend(properties.clone());
                let mut replacement = ImagePlayback::open_scene_images_with_properties(
                    &self.source,
                    self.playback.report().settings,
                    &merged,
                )?;
                replacement.seek(self.playback.time());
                if let Some(time) = self.calendar_time {
                    replacement.set_calendar_time(time);
                }
                if self.playback.playing() {
                    replacement.play();
                }
                let previous = std::mem::replace(&mut self.playback, replacement);
                self.last_sample = None;
                self.last_frame = None;
                previous.shutdown()?;
            }
            Command::SetCalendarTime { unix_ms } => {
                if *unix_ms > 4_102_444_800_000 {
                    return Err("calendar time must be within 1970..=2100".into());
                }
                let time = SystemTime::UNIX_EPOCH + Duration::from_millis(*unix_ms);
                self.calendar_time = Some(time);
                self.playback.set_calendar_time(time);
                self.playback.seek(self.playback.time());
                self.last_sample = None;
                self.last_frame = None;
            }
        }
        Ok(Vec::new())
    }
}

fn frame_info(frame: &Frame, sample: &ImagePlaybackSample, rgba: &[u8]) -> Value {
    let adapter = frame.adapter_info();
    json!({
        "sequence": frame.sequence(), "time_ms": sample.time.as_secs_f64() * 1000.,
        "width": frame.size().width(), "height": frame.size().height(),
        "sha256": format!("{:x}", Sha256::digest(rgba)),
        "adapter": {"name": adapter.name, "software": adapter.software},
        "text_values": sample.text_values,
    })
}

fn write_reply(value: &Value, png: &[u8]) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.write_all(png)?;
    stdout.flush()?;
    Ok(())
}

fn run() -> Result<()> {
    let mut viewer = Viewer::open()?;
    write_reply(
        &json!({"event":"ready", "ok":true, "state":viewer.state()?, "bytes":0}),
        &[],
    )?;
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        let request: Request = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(error) => {
                write_reply(
                    &json!({"id":id,"ok":false,"error":error.to_string(),"bytes":0}),
                    &[],
                )?;
                continue;
            }
        };
        match viewer.command(&request.command) {
            Ok(png) => write_reply(
                &json!({"id":request.id,"ok":true,"state":viewer.state()?,"bytes":png.len()}),
                &png,
            )?,
            Err(error) => write_reply(
                &json!({"id":request.id,"ok":false,"error":error.to_string(),"bytes":0}),
                &[],
            )?,
        }
        if matches!(request.command, Command::Stop) {
            break;
        }
    }
    viewer.playback.shutdown()?;
    viewer.renderer.shutdown()?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        let _ = write_reply(
            &json!({"event":"failed","ok":false,"error":error.to_string(),"bytes":0}),
            &[],
        );
        eprintln!("{error}");
        std::process::exit(1);
    }
}
