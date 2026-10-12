//! Fixed-time GPU playback probe using the library's public consumer API.

use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::BufWriter,
    path::PathBuf,
    time::{Duration, Instant},
};
use wallpaper_kit::{DeviceMode, ImagePlayback, PlaybackSettings, Renderer, SceneSource, Size};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() {
    if let Err(error) = run() {
        println!("{}", json!({"supported":false,"error":error.to_string()}));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let package = PathBuf::from(args.next().ok_or("missing package")?);
    let engine_assets = PathBuf::from(args.next().ok_or("missing engine assets")?);
    let output = PathBuf::from(args.next().ok_or("missing output directory")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    fs::create_dir_all(&output)?;
    let source = SceneSource {
        package,
        engine_assets,
    };
    let settings = PlaybackSettings {
        allow_unsupported_features: true,
        ..Default::default()
    };
    let mut playback = if source
        .package
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pkg"))
    {
        ImagePlayback::open_scene_images_with_settings(&source, settings)?
    } else {
        ImagePlayback::open_video(&source.package, Size::new(640, 360)?, settings)?
    };
    playback.set_calendar_time(std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000));
    let (renderer, events) = Renderer::new(Size::new(640, 360)?, DeviceMode::Software)?;
    let mut captures = Vec::new();
    for time_ms in [0, 1500, 4000, 0] {
        playback.seek(Duration::from_millis(time_ms));
        let deadline = Instant::now() + Duration::from_secs(20);
        let sample = loop {
            if let Some(sample) = playback.try_sample()? {
                break sample;
            }
            if Instant::now() >= deadline {
                return Err("asset preparation timed out".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let background = sample
            .composition
            .background_color()
            .map(|v| (v * 255.).round() as u8);
        let id = renderer.render_images(sample.composition)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let frame = loop {
            if let Some(frame) = renderer.try_frame()?
                && frame.id() == id
            {
                break frame;
            }
            if Instant::now() >= deadline {
                return Err("GPU completion timed out".into());
            }
            events.recv_timeout(Duration::from_millis(10)).ok();
        };
        if frame.image_layer_count().is_none_or(|count| count == 0) {
            return Err("empty image composition".into());
        }
        let pixels = frame.readback()?;
        if pixels.rgba.chunks_exact(4).all(|pixel| {
            pixel
                .iter()
                .zip(background)
                .all(|(a, b)| a.abs_diff(b) <= 1)
        }) {
            return Err("only the composition background was drawn".into());
        }
        let hash = format!("{:x}", Sha256::digest(&pixels.rgba));
        if time_ms == 0 && !captures.is_empty() && captures[0] != hash {
            return Err("fixed-time rewind changed pixels".into());
        }
        if captures.is_empty() {
            let mut encoder = png::Encoder::new(
                BufWriter::new(fs::File::create(output.join("first.png"))?),
                pixels.size.width(),
                pixels.size.height(),
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(&pixels.rgba)?;
        }
        captures.push(hash);
    }
    playback.play();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut live = Vec::new();
    while live.len() < 3 {
        std::thread::sleep(Duration::from_millis(33));
        if let Some(sample) = playback.try_sample()? {
            if live
                .last()
                .is_some_and(|previous| *previous >= sample.time.as_nanos())
            {
                return Err("live sample clock did not advance".into());
            }
            live.push(sample.time.as_nanos());
        }
        if Instant::now() >= deadline {
            return Err("live asset sampling timed out".into());
        }
    }
    let report = json!({"supported":true,"playback":playback.report(),"pixel_sha256":captures,"resolution":[640,360],"backend":"D3D11 WARP","times_ms":[0,1500,4000,0],"live_sample_times_ns":live});
    renderer.shutdown()?;
    playback.shutdown()?;
    println!("{report}");
    Ok(())
}
