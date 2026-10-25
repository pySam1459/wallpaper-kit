//! Public-API sprite residency/work profile; no rendering or pixel readback.
use serde::Deserialize;
use serde_json::json;
use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use wallpaper_kit::{SceneSource, SpritePager, TextureDecoder};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    package: PathBuf,
    engine_assets: PathBuf,
    assets: Vec<String>,
    seconds: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let config: Config = serde_json::from_slice(&fs::read(args.next().ok_or("missing config")?)?)?;
    let output = PathBuf::from(args.next().ok_or("missing output")?);
    if args.next().is_some()
        || config.assets.is_empty()
        || config.assets.len() > 32
        || !(1..=120).contains(&config.seconds)
        || output.exists()
    {
        return Err("invalid profile configuration or existing output".into());
    }
    fs::create_dir(&output)?;
    let source = SceneSource {
        package: config.package,
        engine_assets: config.engine_assets,
    };
    let decoder = TextureDecoder::open(&source)?;
    let mut profiles = Vec::new();
    for asset in config.assets {
        let mut stream = decoder.sprite_stream(&asset)?;
        let started = Instant::now();
        drop(stream.sample(Duration::ZERO)?);
        let decode_ms = started.elapsed().as_secs_f64() * 1000.;
        let frames = stream.frames();
        let frame = frames[0];
        let stats = stream.stats();
        let metadata = json!({"asset":asset,"frames":frames.len(),"duration_ms":stream.duration().as_secs_f64()*1000.,
            "first_page_frames":frames.iter().take_while(|f|f.page_index()==frame.page_index()).count(),
            "frame_extent":frame.extent(),"decoded_atlas_bytes":stats.cached_page_bytes,
            "first_frame_bytes":stats.cached_frame_bytes,"first_decode_extract_ms":decode_ms});
        drop(stream);
        let (pager, _events) = SpritePager::new(source.clone(), asset)?;
        profiles.push((pager, metadata, Vec::new()));
    }
    let started = Instant::now();
    let mut slot = 0;
    while started.elapsed() < Duration::from_secs(u64::from(config.seconds)) {
        let time = started.elapsed();
        for (pager, _, observations) in &mut profiles {
            let ready = pager.try_sample_at(time)?;
            if let Some(stats) = pager.try_stats()? {
                observations.push(json!({"time_ms":time.as_secs_f64()*1000.,"ready":ready.is_some(),
                    "cached_frame_bytes":stats.cached_frame_bytes,"reserved_work_bytes":stats.reserved_work_bytes,
                    "peak_admitted_bytes":stats.peak_admitted_bytes,"page_decodes":stats.page_decodes,
                    "frame_extracts":stats.frame_extracts,"prepared_batches":stats.prepared_batches,
                    "unavailable_polls":stats.unavailable_polls,"preparing":stats.preparing}));
            }
        }
        slot += 1;
        if let Some(wait) =
            Duration::from_secs_f64(f64::from(slot) / 60.).checked_sub(started.elapsed())
        {
            std::thread::sleep(wait);
        }
    }
    let mut assets = Vec::new();
    for (pager, metadata, observations) in profiles {
        pager.shutdown()?;
        assets.push(json!({"metadata":metadata,"samples":observations}));
    }
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&json!({"package_sha256":decoder.package_sha256(),
        "seconds":config.seconds,"assets":assets,"limits":"CPU-only pager work/residency; initialization and JSON observations perturb timing."}))?,
    )?;
    println!("{}", output.display());
    Ok(())
}
