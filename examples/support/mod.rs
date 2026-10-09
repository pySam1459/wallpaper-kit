use scene_renderer::{DeviceMode, Frame, Pixels};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub fn options() -> Result<(PathBuf, DeviceMode)> {
    let mut args = std::env::args().skip(1);
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
        "frame_id": frame.id(), "width": frame.size().width(), "height": frame.size().height(),
        "backend": "direct3d11", "format": "rgba8-unorm", "alpha": "opaque", "content": "blank",
        "adapter": {"name": adapter.name, "vendor_id": adapter.vendor_id, "device_id": adapter.device_id,
            "luid": adapter.luid, "software": adapter.software},
    })
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
