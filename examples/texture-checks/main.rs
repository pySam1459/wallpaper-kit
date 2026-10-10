//! Public consumer of bounded TEX decoding and diagnostic GPU page drawing.

#[path = "../support/texture.rs"]
mod fixture;
#[path = "../support/mod.rs"]
mod support;

use scene_renderer::{
    Frame, Renderer, SceneSource, Size, TextureDecoder, TextureFormat, TextureRegion,
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, mpsc::Receiver},
    time::{Duration, Instant},
};
use support::Result;

fn wait_frame(renderer: &Renderer, events: &Receiver<()>, id: u64) -> Result<Arc<Frame>> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(frame) = renderer.try_frame()?
            && frame.id() == id
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err("texture frame timed out".into());
        }
        let _ = events.recv_timeout(Duration::from_millis(10));
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut common = Vec::new();
    let mut package = None;
    let mut engine = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scene" => {
                package = Some(PathBuf::from(args.next().ok_or("--scene needs a package")?))
            }
            "--engine-assets" => {
                engine = Some(PathBuf::from(
                    args.next().ok_or("--engine-assets needs a directory")?,
                ))
            }
            _ => common.push(arg),
        }
    }
    if package.is_some() != engine.is_some() {
        return Err("--scene and --engine-assets must be supplied together".into());
    }
    let (output, mode) = support::options(common.into_iter())?;
    let generated = output.join("generated");
    fs::create_dir(&generated)?;
    fs::create_dir(generated.join("engine"))?;
    let cases = fixture::cases();
    let entries: Vec<_> = cases
        .iter()
        .map(|case| (case.name, case.tex.as_slice()))
        .collect();
    fs::write(generated.join("scene.pkg"), fixture::package(&entries))?;
    let source = SceneSource {
        package: generated.join("scene.pkg"),
        engine_assets: generated.join("engine"),
    };
    let decoder = TextureDecoder::open(&source)?;
    let (renderer, events) = Renderer::new(Size::new(320, 180)?, mode)?;
    if support::uniform_rgba(&wait_frame(&renderer, &events, 1)?.readback()?)
        != Some([16, 24, 32, 255])
    {
        return Err("initial blank frame differs".into());
    }
    let mut results = Vec::new();
    let mut region_results = Vec::new();
    for case in &cases {
        let page = Arc::new(decoder.decode_page(case.name, 0)?);
        let id = renderer.render_texture_page(page.clone())?;
        let first = wait_frame(&renderer, &events, id)?;
        let pixels = first.readback()?;
        fixture::check(&pixels, case.extent, &case.rgba)?;
        support::write_png(&output.join(format!("{}.png", case.name)), &pixels)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let uploads = loop {
            if let Some(stats) = renderer.try_output_stats()?
                && stats.published_frames == first.sequence()
            {
                break stats.texture_uploads;
            }
            if Instant::now() >= deadline {
                return Err("texture counters timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        for _ in 0..12 {
            let id = renderer.render_texture_page(page.clone())?;
            drop(wait_frame(&renderer, &events, id)?);
        }
        let id = renderer.resize(Size::new(317, 193)?)?;
        let resized = wait_frame(&renderer, &events, id)?;
        fixture::check(&resized.readback()?, case.extent, &case.rgba)?;
        fixture::check(&first.readback()?, case.extent, &case.rgba)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let stats = loop {
            if let Some(stats) = renderer.try_output_stats()?
                && stats.published_frames == resized.sequence()
            {
                break stats;
            }
            if Instant::now() >= deadline {
                return Err("texture counters timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        if stats.texture_uploads != uploads
            || stats.source_texture_bytes != page.data().len() as u64
            || stats.resident_textures > 3
        {
            return Err("source upload reuse or output bounds failed".into());
        }
        results.push(json!({"name":case.name,"checks":"passed","frame":support::frame_info(&resized),"resources":support::pool_info(stats)}));
        drop(resized);
        drop(first);
        let frames = decoder.sprite_frames(case.name)?;
        for (index, metadata) in frames.iter().enumerate() {
            let origin = metadata.origin();
            let extent = metadata.extent();
            let rgba = fixture::crop(&case.rgba, case.extent, origin, extent);
            let region = Arc::new(TextureRegion::new(page.clone(), origin, extent)?);
            let id = renderer.render_texture_region(region)?;
            let first = wait_frame(&renderer, &events, id)?;
            fixture::check(&first.readback()?, extent, &rgba)?;
            support::write_png(
                &output.join(format!("{}-crop-{index}.png", case.name)),
                &first.readback()?,
            )?;
            let id = renderer.resize(Size::new(113, 79)?)?;
            let resized = wait_frame(&renderer, &events, id)?;
            fixture::check(&resized.readback()?, extent, &rgba)?;
            fixture::check(&first.readback()?, extent, &rgba)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            let stats = loop {
                if let Some(stats) = renderer.try_output_stats()?
                    && stats.published_frames == resized.sequence()
                {
                    break stats;
                }
                if Instant::now() >= deadline {
                    return Err("crop counters timed out".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            if stats.texture_uploads != uploads
                || stats.source_texture_bytes != page.data().len() as u64
                || stats.resident_textures > 3
            {
                return Err("crop uploaded source again or exceeded output bounds".into());
            }
            region_results.push(json!({"name":case.name,"frame_index":index,"origin":origin,"extent":extent,"checks":"passed","frame":support::frame_info(&resized),"resources":support::pool_info(stats)}));
        }
        let id = renderer.resize(Size::new(320, 180)?)?;
        drop(wait_frame(&renderer, &events, id)?);
    }
    let mut target = Vec::new();
    let mut target_frames = Vec::new();
    let mut target_identity = None;
    if let (Some(package), Some(engine_assets)) = (package, engine) {
        let source = SceneSource {
            package,
            engine_assets,
        };
        let opened = Instant::now();
        let decoder = TextureDecoder::open(&source)?;
        target_identity = Some(
            json!({"package":source.package,"engine_assets":source.engine_assets,"package_sha256":decoder.package_sha256(),"index_and_hash_ms":opened.elapsed().as_secs_f64()*1000.}),
        );
        let id = renderer.resize(Size::new(960, 540)?)?;
        drop(wait_frame(&renderer, &events, id)?);
        for (name, asset, page_index) in [
            ("background", "materials/Bg.tex", 0),
            ("clouds", "materials/Clouds.tex", 0),
            ("foreground", "materials/FG.tex", 0),
            (
                "cloud-mask",
                "materials/masks/cloudmotion_mask_ce1abe94.tex",
                0,
            ),
            (
                "bokeh",
                "materials/workshop/2820123291/particle/Boke_hex_tex.tex",
                0,
            ),
            ("birds-first", "materials/Oiseaux optimized_00000.tex", 0),
            ("birds-last", "materials/Oiseaux optimized_00000.tex", 26),
            ("character-first", "materials/Resize Yuta_00000.tex", 0),
            ("character-last", "materials/Resize Yuta_00000.tex", 29),
        ] {
            let started = Instant::now();
            let page = Arc::new(decoder.decode_page(asset, page_index)?);
            let decode_ms = started.elapsed().as_secs_f64() * 1000.;
            let id = renderer.render_texture_page(page.clone())?;
            let frame = wait_frame(&renderer, &events, id)?;
            let pixels = frame.readback()?;
            if page.format() == TextureFormat::Rgba8 {
                fixture::check(&pixels, page.extent(), page.data())?;
            }
            support::write_png(&output.join(format!("target-{name}.png")), &pixels)?;
            target.push(json!({"asset":asset,"page":page_index,"extent":page.extent(),"format":format!("{:?}", page.format()),"bytes":page.data().len(),"decode_ms":decode_ms,"frame":support::frame_info(&frame),"comparison":if page.format() == TextureFormat::Rgba8 {"point sampling checked against decoded page"} else {"capture only; generated format oracle tested separately"}}));
        }
        for (name, asset, boundary) in [
            ("birds", "materials/Oiseaux optimized_00000.tex", 18),
            ("character", "materials/Resize Yuta_00000.tex", 16),
        ] {
            let frames = decoder.sprite_frames(asset)?;
            if frames.len() != 480 {
                return Err("target sprite frame count differs".into());
            }
            let mut loaded = None;
            for index in [0, boundary - 1, boundary, 239, 479] {
                let metadata = frames[index];
                if loaded
                    .as_ref()
                    .is_none_or(|(page_index, _)| *page_index != metadata.page_index())
                {
                    // Diagnostic captures decode serially; this is not a streaming cache.
                    loaded = Some((
                        metadata.page_index(),
                        Arc::new(decoder.decode_page(asset, metadata.page_index())?),
                    ));
                }
                let page = &loaded.as_ref().expect("selected page loaded").1;
                let region = Arc::new(TextureRegion::new(
                    page.clone(),
                    metadata.origin(),
                    metadata.extent(),
                )?);
                let id = renderer.render_texture_region(region)?;
                let frame = wait_frame(&renderer, &events, id)?;
                let pixels = frame.readback()?;
                if page.format() == TextureFormat::Rgba8 {
                    let rgba = fixture::crop(
                        page.data(),
                        page.extent(),
                        metadata.origin(),
                        metadata.extent(),
                    );
                    fixture::check(&pixels, metadata.extent(), &rgba)?;
                }
                support::write_png(
                    &output.join(format!("target-{name}-frame-{index:03}.png")),
                    &pixels,
                )?;
                target_frames.push(json!({"asset":asset,"frame_index":index,"page_index":metadata.page_index(),"origin":metadata.origin(),"extent":metadata.extent(),"duration_seconds":metadata.duration().as_secs_f64(),"frame":support::frame_info(&frame),"comparison":if page.format()==TextureFormat::Rgba8 {"GPU crop checked against CPU rows"} else {"capture only; unaligned BC3 crops checked with generated fixtures"}}));
            }
        }
    }
    renderer.shutdown()?;
    support::write_json(
        &output.join("report.json"),
        &json!({
            "schema_version":2,"status":"passed","consumer":"texture-checks", "generated_cases":results, "generated_regions":region_results,"target_identity":target_identity,"target_pages":target,"target_frames":target_frames,
            "limits":["Diagnostic page/crop draws, not placed scene layers or sprite playback.","Point sampling and straight-alpha code-value composition; scene conventions unverified.","Decode/upload/capture observations are not steady rendering or host latency measurements."]
        }),
    )?;
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>TEX page checks</title><style>body{background:#101820;color:white;font:16px system-ui}img{max-width:480px;display:block}</style><h1>Diagnostic TEX pages</h1>",
    );
    for entry in fs::read_dir(&output)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|value| value == "png") {
            let name = entry.file_name().to_string_lossy().into_owned();
            html.push_str(&format!("<h2>{name}</h2><img src=\"{name}\">"));
        }
    }
    fs::write(output.join("index.html"), html)?;
    println!(
        "{}",
        json!({"status":"passed","output":output,"generated_cases":cases.len()})
    );
    Ok(())
}
