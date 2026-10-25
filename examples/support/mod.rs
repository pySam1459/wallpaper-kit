use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use wallpaper_kit::{DeviceMode, Frame, OutputStats, Pixels};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub fn options(mut args: impl Iterator<Item = String>) -> Result<(PathBuf, DeviceMode)> {
    let mut output = None;
    let mut mode = DeviceMode::Hardware;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().ok_or("--output needs a directory")?,
                ))
            }
            "--software" => mode = DeviceMode::Software,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let output = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "artifacts/manual-{}-{}",
            std::process::id(),
            timestamp()
        ))
    });
    fs::create_dir_all(&output)?;
    if fs::read_dir(&output)?.next().is_some() {
        return Err("evidence directory must be empty".into());
    }
    Ok((output, mode))
}

pub fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock precedes epoch")
        .as_millis()
}

pub fn frame_info(frame: &Frame) -> Value {
    let adapter = frame.adapter_info();
    json!({
        "frame_id": frame.id(), "sequence": frame.sequence(),
        "width": frame.size().width(), "height": frame.size().height(),
        "backend": "direct3d11", "format": "rgba8-unorm", "alpha": "opaque",
        "content": if frame.fixture_time().is_some() { "motion-fixture" } else if frame.image_layer_count().is_some() { "image-composition" } else if frame.texture_format().is_some() { "texture-page" } else { "blank" },
        "image_layer_count":frame.image_layer_count(),
        "source_format": frame.texture_format().map(|format| format!("{format:?}")),
        "source_region": frame.texture_region(),
        "fixture_time_ms": frame.fixture_time().map(|time| time.as_millis()),
        "adapter": {"name": adapter.name, "vendor_id": adapter.vendor_id, "device_id": adapter.device_id,
            "luid": adapter.luid, "software": adapter.software},
    })
}

pub fn pool_info(stats: OutputStats) -> Value {
    json!({"profile":stats.profile.map(|p| json!({"submitted_frames":p.submitted_frames,"gpu_frames":p.gpu_frames,
        "invalid_gpu_frames":p.invalid_gpu_frames,"gpu_draw_ns":p.gpu_draw_ns,
        "worker_prepare_ns":p.worker_prepare_ns,"worker_submit_ns":p.worker_submit_ns})),
        "resident_textures":stats.resident_textures,"resident_bytes":stats.resident_bytes,
        "texture_allocations":stats.texture_allocations,"submitted_frames":stats.submitted_frames,
        "source_texture_bytes":stats.source_texture_bytes,"texture_uploads":stats.texture_uploads,
        "effect_texture_bytes":stats.effect_texture_bytes,
        "published_frames":stats.published_frames,"waiting_for_frame_release":stats.waiting_for_frame_release})
}

pub fn write_png(path: &Path, pixels: &Pixels) -> Result<()> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(file),
        pixels.size.width(),
        pixels.size.height(),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels.rgba)?;
    Ok(())
}

pub fn uniform_rgba(pixels: &Pixels) -> Option<[u8; 4]> {
    let first: [u8; 4] = pixels.rgba.get(..4)?.try_into().ok()?;
    pixels
        .rgba
        .chunks_exact(4)
        .all(|pixel| pixel == first)
        .then_some(first)
}

pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
