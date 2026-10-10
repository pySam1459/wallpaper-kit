//! Public consumer of bounded TEX decoding and diagnostic GPU page drawing.

#[path = "../support/texture.rs"]
mod texture_checks;
use texture_checks as fixture;
#[path = "../support/layers.rs"]
mod layers_checks;
#[path = "../support/paging.rs"]
mod paging;
#[path = "../support/sprite.rs"]
mod sprite_checks;
#[path = "../support/mod.rs"]
mod support;

use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, mpsc::Receiver},
    time::{Duration, Instant},
};
use support::Result;
use wallpaper_kit::{
    Frame, OutputStats, Renderer, SceneSource, Size, SpritePager, SpriteWorker, TextureDecoder,
    TextureFormat, TextureRegion,
};

fn wait_frame(renderer: &Renderer, events: &Receiver<()>, id: u64) -> Result<Arc<Frame>> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(frame) = renderer.try_frame()?
            && frame.id() == id
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            let latest = renderer
                .try_frame()?
                .map(|frame| (frame.id(), frame.sequence()));
            return Err(format!(
                "texture frame timed out: requested {id}, latest {latest:?}, pool {:?}",
                renderer.try_output_stats()?
            )
            .into());
        }
        let _ = events.recv_timeout(Duration::from_millis(10));
    }
}

fn wait_stats(renderer: &Renderer, sequence: u64) -> Result<OutputStats> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(stats) = renderer.try_output_stats()?
            && stats.published_frames == sequence
        {
            return Ok(stats);
        }
        if Instant::now() >= deadline {
            return Err("composition counters timed out".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

// Diagnostic wall-clock consumer: waits for GPU readiness, never reads pixels
// during timing. This measures this consumer, not a host UI or display cadence.
fn live_pager(
    renderer: &Renderer,
    events: &Receiver<()>,
    pager: &SpritePager,
    pager_events: &Receiver<()>,
    duration: Duration,
) -> Result<serde_json::Value> {
    let started = Instant::now();
    let tick = Duration::from_nanos(1_000_000_000 / 60);
    let finish = duration * 2;
    let mut deadline = Duration::ZERO;
    let mut polls = 0;
    let mut unavailable = 0;
    let mut max_poll_ms = 0_f64;
    let mut max_gpu_ready_ms = 0_f64;
    let mut max_tick_gap_ms = 0_f64;
    let mut last_tick = Duration::ZERO;
    let mut previous = None;
    let mut rendered = 0;
    let mut skipped_frames = 0;
    let mut first_sequence = None;
    let mut last_sequence = None;
    let mut latest = None;
    while started.elapsed() < finish {
        let time = started.elapsed();
        if time < deadline {
            std::thread::sleep(deadline - time);
            continue;
        }
        max_tick_gap_ms = max_tick_gap_ms.max((time - last_tick).as_secs_f64() * 1000.);
        last_tick = time;
        deadline = time + tick;
        let poll = Instant::now();
        let sample = pager.try_sample_at(time)?;
        max_poll_ms = max_poll_ms.max(poll.elapsed().as_secs_f64() * 1000.);
        polls += 1;
        let Some(sample) = sample else {
            unavailable += 1;
            continue; // Retain the previous completed GPU frame.
        };
        let index = sample.frame_index();
        if previous == Some(index) {
            continue;
        }
        if let Some(old) = previous {
            // This route only runs the two inspected 480-frame target assets.
            let advance = (index + 480 - old) % 480;
            skipped_frames += advance.saturating_sub(1);
        }
        previous = Some(index);
        let ready = Instant::now();
        let id = renderer.render_texture_region(sample.region().clone())?;
        let frame = wait_frame(renderer, events, id)?;
        max_gpu_ready_ms = max_gpu_ready_ms.max(ready.elapsed().as_secs_f64() * 1000.);
        first_sequence.get_or_insert(frame.sequence());
        last_sequence = Some(frame.sequence());
        latest = Some(frame);
        rendered += 1;
    }
    if rendered == 0 {
        return Err("live pager produced no GPU frames".into());
    }
    // Pausing holds the exact final demand; any outstanding prefetch may settle.
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.;
    let paused = paging::settled(pager, pager_events, 2)?;
    let idle_started = Instant::now();
    std::thread::sleep(Duration::from_millis(100));
    let idle = paging::settled(pager, pager_events, 2)?;
    if paused.page_decodes != idle.page_decodes || paused.frame_extracts != idle.frame_extracts {
        return Err("paused pager continued periodic work".into());
    }
    let resources = wait_stats(
        renderer,
        latest.as_ref().expect("rendered frame").sequence(),
    )?;
    Ok(
        json!({"loops":2,"authored_duration_ns":duration.as_nanos(),"requested_tick_hz":60,"elapsed_ms":elapsed_ms,"polls":polls,"unavailable_polls":unavailable,"rendered_frames":rendered,"skipped_authored_frames":skipped_frames,"max_sample_poll_ms":max_poll_ms,"max_gpu_ready_wait_ms":max_gpu_ready_ms,"max_tick_gap_ms":max_tick_gap_ms,"first_sequence":first_sequence,"last_sequence":last_sequence,"pause_observed_ms":idle_started.elapsed().as_secs_f64()*1000.,"pause_counters_unchanged":true,"resources":support::pool_info(resources)}),
    )
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
    let mut extracted_results = Vec::new();
    let mut worker_results = Vec::new();
    let mut paged_results = Vec::new();
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
        let mut stream = decoder.sprite_stream(case.name)?;
        let first_end = stream.frames()[0].duration();
        let total = stream.duration();
        for (checkpoint, time, expected_index) in [
            ("start", Duration::ZERO, 0),
            ("before-boundary", first_end - Duration::from_nanos(1), 0),
            ("boundary", first_end, 1),
            ("last", total - Duration::from_nanos(1), 1),
            ("loop", total, 0),
            ("second-loop", total * 2 + first_end, 1),
        ] {
            let sample = stream.sample(time)?;
            if sample.frame_index() != expected_index {
                return Err("sprite time selection differs".into());
            }
            let authored = sample.authored_frame();
            let expected = fixture::crop(
                &case.rgba,
                case.extent,
                authored.origin(),
                authored.extent(),
            );
            let id = renderer.render_texture_region(sample.region().clone())?;
            let frame = wait_frame(&renderer, &events, id)?;
            let pixels = frame.readback()?;
            fixture::check(&pixels, authored.extent(), &expected)?;
            support::write_png(
                &output.join(format!("extracted-{}-{checkpoint}.png", case.name)),
                &pixels,
            )?;
            let before = wait_stats(&renderer, frame.sequence())?;
            let repeated = stream.sample(time)?;
            if !Arc::ptr_eq(sample.region(), repeated.region()) {
                return Err("current extracted frame was copied again".into());
            }
            let id = renderer.render_texture_region(repeated.region().clone())?;
            let repeated = wait_frame(&renderer, &events, id)?;
            let stats = wait_stats(&renderer, repeated.sequence())?;
            if stats.texture_uploads != before.texture_uploads
                || stats.source_texture_bytes != sample.region().page().data().len() as u64
            {
                return Err("extracted source upload reuse differs".into());
            }
            let cache = stream.stats();
            extracted_results.push(json!({"name":case.name,"checkpoint":checkpoint,"time_ns":time.as_nanos(),"frame_index":sample.frame_index(),"source_origin":authored.origin(),"source_extent":authored.extent(),"extracted_extent":sample.region().page().extent(),"residual_crop":sample.region().origin(),"page_decodes":cache.page_decodes,"frame_extracts":cache.frame_extracts,"cached_page_bytes":cache.cached_page_bytes,"cached_frame_bytes":cache.cached_frame_bytes,"resources":support::pool_info(stats),"checks":"passed"}));
        }
        let (worker, worker_events) = SpriteWorker::new(source.clone(), case.name.into())?;
        let request_id = sprite_checks::request(&worker, first_end)?;
        // The independent GPU worker can produce a frame while assets prepare.
        let progress = renderer.render_fixture_at(Duration::ZERO)?;
        drop(wait_frame(&renderer, &events, progress)?);
        let (result, metrics) = sprite_checks::wait(&worker, &worker_events, request_id)?;
        let sample = result.into_sample()?;
        if sample.frame_index() != 1 {
            return Err("worker selected wrong sprite time".into());
        }
        let authored = sample.authored_frame();
        let expected = fixture::crop(
            &case.rgba,
            case.extent,
            authored.origin(),
            authored.extent(),
        );
        let id = renderer.render_texture_region(sample.region().clone())?;
        let frame = wait_frame(&renderer, &events, id)?;
        let pixels = frame.readback()?;
        fixture::check(&pixels, authored.extent(), &expected)?;
        support::write_png(&output.join(format!("worker-{}.png", case.name)), &pixels)?;
        let before = wait_stats(&renderer, frame.sequence())?;
        let request_id = sprite_checks::request(&worker, first_end)?;
        let (repeated, _) = sprite_checks::wait(&worker, &worker_events, request_id)?;
        let repeated = repeated.into_sample()?;
        if !Arc::ptr_eq(sample.region(), repeated.region()) {
            return Err("worker copied repeated sample".into());
        }
        let id = renderer.render_texture_region(repeated.region().clone())?;
        let repeated = wait_frame(&renderer, &events, id)?;
        let stats = wait_stats(&renderer, repeated.sequence())?;
        if stats.texture_uploads != before.texture_uploads {
            return Err("worker sample uploaded again".into());
        }
        worker.shutdown()?;
        worker_results.push(json!({"name":case.name,"preparation":metrics,"resources":support::pool_info(stats),"checks":"passed"}));
        drop(frame);
        drop(repeated);
        let (pager, pager_events) = SpritePager::new(source.clone(), case.name.into())?;
        let initial = paging::sample(&pager, &pager_events, Duration::ZERO)?;
        let total: Duration = decoder
            .sprite_frames(case.name)?
            .iter()
            .map(|frame| frame.duration())
            .sum();
        for (label, time, expected_index) in [("boundary", first_end, 1), ("loop", total, 0)] {
            let sample = paging::sample(&pager, &pager_events, time)?;
            if sample.frame_index() != expected_index
                || (expected_index == 0 && !Arc::ptr_eq(initial.region(), sample.region()))
            {
                return Err("prepared batch selection or reuse differs".into());
            }
            let authored = sample.authored_frame();
            let expected = fixture::crop(
                &case.rgba,
                case.extent,
                authored.origin(),
                authored.extent(),
            );
            let id = renderer.render_texture_region(sample.region().clone())?;
            let frame = wait_frame(&renderer, &events, id)?;
            let pixels = frame.readback()?;
            fixture::check(&pixels, authored.extent(), &expected)?;
            support::write_png(
                &output.join(format!("paged-{}-{label}.png", case.name)),
                &pixels,
            )?;
        }
        let cache = paging::settled(&pager, &pager_events, 1)?;
        if cache.page_decodes != 1
            || cache.frame_extracts != 2
            || cache.peak_admitted_bytes > 768 * 1024 * 1024
        {
            return Err("generated batch bounds or decode reuse differ".into());
        }
        pager.shutdown()?;
        paged_results
            .push(json!({"name":case.name,"cache":paging::stats(cache),"checks":"passed"}));
        let id = renderer.resize(Size::new(320, 180)?)?;
        drop(wait_frame(&renderer, &events, id)?);
    }
    let layers = layers_checks::Fixture::new(&decoder)?;
    let id = renderer.render_images(layers.composition.clone())?;
    let composed = wait_frame(&renderer, &events, id)?;
    layers.check(&composed.readback()?)?;
    if composed.image_layer_count() != Some(6) {
        return Err("composition metadata differs".into());
    }
    support::write_png(&output.join("composition.png"), &composed.readback()?)?;
    let uploads = wait_stats(&renderer, composed.sequence())?.texture_uploads;
    for _ in 0..12 {
        let id = renderer.render_images(layers.composition.clone())?;
        drop(wait_frame(&renderer, &events, id)?);
    }
    let id = renderer.resize(Size::new(317, 193)?)?;
    let resized = wait_frame(&renderer, &events, id)?;
    layers.check(&resized.readback()?)?;
    layers.check(&composed.readback()?)?;
    let stats = wait_stats(&renderer, resized.sequence())?;
    if stats.texture_uploads != uploads
        || stats.source_texture_bytes != 86
        || stats.effect_texture_bytes != 288
        || stats.resident_textures > 3
    {
        return Err("composition source reuse/bounds differ".into());
    }
    let composition_result = json!({"checks":"passed","frame":support::frame_info(&resized),"resources":support::pool_info(stats)});
    drop(resized);
    drop(composed);
    let mut target_compositions = Vec::new();
    let mut target = Vec::new();
    let mut target_frames = Vec::new();
    let mut target_extracted = Vec::new();
    let mut target_worker = Vec::new();
    let mut target_paged = Vec::new();
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
        let mut references = std::collections::BTreeMap::new();
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
                references.insert((name, index), pixels.rgba);
            }
        }
        for (name, asset, boundary) in [
            ("birds", "materials/Oiseaux optimized_00000.tex", 18),
            ("character", "materials/Resize Yuta_00000.tex", 16),
        ] {
            let mut stream = decoder.sprite_stream(asset)?;
            for (label, index, time) in [
                ("first", 0, Duration::ZERO),
                (
                    "before-page",
                    boundary - 1,
                    stream
                        .frames()
                        .iter()
                        .take(boundary - 1)
                        .map(|frame| frame.duration())
                        .sum(),
                ),
                (
                    "next-page",
                    boundary,
                    stream
                        .frames()
                        .iter()
                        .take(boundary)
                        .map(|frame| frame.duration())
                        .sum(),
                ),
                (
                    "middle",
                    239,
                    stream
                        .frames()
                        .iter()
                        .take(239)
                        .map(|frame| frame.duration())
                        .sum(),
                ),
                (
                    "last",
                    479,
                    stream
                        .frames()
                        .iter()
                        .take(479)
                        .map(|frame| frame.duration())
                        .sum(),
                ),
                ("loop", 0, stream.duration()),
            ] {
                let started = Instant::now();
                let sample = stream.sample(time)?;
                let sample_ms = started.elapsed().as_secs_f64() * 1000.;
                if sample.frame_index() != index {
                    return Err("target time selection differs".into());
                }
                let id = renderer.render_texture_region(sample.region().clone())?;
                let frame = wait_frame(&renderer, &events, id)?;
                let pixels = frame.readback()?;
                if pixels.rgba != references[&(name, index)] {
                    return Err("extracted target pixels differ from whole-atlas GPU crop".into());
                }
                support::write_png(
                    &output.join(format!("target-extracted-{name}-{label}.png")),
                    &pixels,
                )?;
                let stats = wait_stats(&renderer, frame.sequence())?;
                let cache = stream.stats();
                if stats.source_texture_bytes != cache.cached_frame_bytes {
                    return Err("target extraction uploaded full atlas".into());
                }
                target_extracted.push(json!({"asset":asset,"time_ns":time.as_nanos(),"frame_index":index,"page_index":sample.authored_frame().page_index(),"sample_ms":sample_ms,"extracted_extent":sample.region().page().extent(),"residual_crop":sample.region().origin(),"page_decodes":cache.page_decodes,"frame_extracts":cache.frame_extracts,"cached_page_bytes":cache.cached_page_bytes,"cached_frame_bytes":cache.cached_frame_bytes,"resources":support::pool_info(stats),"comparison":"Exact GPU pixels against earlier whole-atlas crop; no Wallpaper Engine reference."}));
            }
            let time = stream
                .frames()
                .iter()
                .take(boundary)
                .map(|frame| frame.duration())
                .sum();
            drop(stream);
            let (worker, worker_events) = SpriteWorker::new(source.clone(), asset.into())?;
            let request_id = sprite_checks::request(&worker, time)?;
            let progress = renderer.render_fixture_at(Duration::ZERO)?;
            drop(wait_frame(&renderer, &events, progress)?);
            let (result, metrics) = sprite_checks::wait(&worker, &worker_events, request_id)?;
            let sample = result.into_sample()?;
            if sample.frame_index() != boundary {
                return Err("target worker selected wrong page-boundary time".into());
            }
            let id = renderer.render_texture_region(sample.region().clone())?;
            let frame = wait_frame(&renderer, &events, id)?;
            let pixels = frame.readback()?;
            if pixels.rgba != references[&(name, boundary)] {
                return Err("target worker pixels differ from whole-atlas crop".into());
            }
            support::write_png(&output.join(format!("target-worker-{name}.png")), &pixels)?;
            let stats = wait_stats(&renderer, frame.sequence())?;
            if stats.source_texture_bytes != sample.region().page().data().len() as u64 {
                return Err("target worker uploaded full atlas".into());
            }
            worker.shutdown()?;
            target_worker.push(json!({"asset":asset,"frame_index":boundary,"preparation":metrics,"resources":support::pool_info(stats),"comparison":"Exact GPU pixels against whole-atlas crop; no Wallpaper Engine reference."}));
            drop(frame);
            let metadata = decoder.sprite_frames(asset)?;
            let duration: Duration = metadata.iter().map(|frame| frame.duration()).sum();
            let (pager, pager_events) = SpritePager::new(source.clone(), asset.into())?;
            let started = Instant::now();
            drop(paging::sample(&pager, &pager_events, Duration::ZERO)?);
            let initial = paging::settled(&pager, &pager_events, 2)?;
            if initial.page_decodes != 2 {
                return Err("initial current/next prefetch differs".into());
            }
            let startup_ms = started.elapsed().as_secs_f64() * 1000.;
            let mut captures = Vec::new();
            for (label, index, time) in [
                ("first", 0, Duration::ZERO),
                (
                    "before-page",
                    boundary - 1,
                    metadata
                        .iter()
                        .take(boundary - 1)
                        .map(|frame| frame.duration())
                        .sum(),
                ),
                ("next-page", boundary, time),
                (
                    "middle",
                    239,
                    metadata
                        .iter()
                        .take(239)
                        .map(|frame| frame.duration())
                        .sum(),
                ),
                (
                    "last",
                    479,
                    metadata
                        .iter()
                        .take(479)
                        .map(|frame| frame.duration())
                        .sum(),
                ),
                ("loop", 0, duration),
            ] {
                let sample = paging::sample(&pager, &pager_events, time)?;
                if sample.frame_index() != index {
                    return Err("target batch selected wrong time".into());
                }
                let id = renderer.render_texture_region(sample.region().clone())?;
                let frame = wait_frame(&renderer, &events, id)?;
                let pixels = frame.readback()?;
                if pixels.rgba != references[&(name, index)] {
                    return Err("target batch differs from whole-atlas crop".into());
                }
                support::write_png(
                    &output.join(format!("target-paged-{name}-{label}.png")),
                    &pixels,
                )?;
                captures.push(json!({"label":label,"frame_index":index,"time_ns":time.as_nanos()}));
            }
            // Explicitly reestablish initial demand before starting the live clock.
            drop(paging::sample(&pager, &pager_events, Duration::ZERO)?);
            let before_live = paging::settled(&pager, &pager_events, 2)?;
            let live = live_pager(&renderer, &events, &pager, &pager_events, duration)?;
            let final_cache = paging::settled(&pager, &pager_events, 2)?;
            if final_cache.cached_frame_bytes > 512 * 1024 * 1024
                || final_cache.peak_admitted_bytes > 768 * 1024 * 1024
            {
                return Err("target prefetch exceeded admission".into());
            }
            pager.shutdown()?;
            target_paged.push(json!({"asset":asset,"startup_ms":startup_ms,"initial_cache":paging::stats(initial),"captures":captures,"before_live":paging::stats(before_live),"live":live,"final_cache":paging::stats(final_cache),"comparison":"Controlled captures exactly match whole-atlas GPU crops. Live headless readiness is separate, without readback or native presentation."}));
        }
        for (name, objects, frame_index) in [
            ("images", vec![28, 32, 77, 86], 0),
            ("birds", vec![72, 89], 239),
            ("wires", vec![22], 0),
        ] {
            let started = Instant::now();
            let snapshot = decoder.image_snapshot(&objects, frame_index)?;
            let prepare_ms = started.elapsed().as_secs_f64() * 1000.;
            let id = renderer.render_images(snapshot.composition)?;
            let frame = wait_frame(&renderer, &events, id)?;
            support::write_png(
                &output.join(format!("target-composition-{name}.png")),
                &frame.readback()?,
            )?;
            let stats = wait_stats(&renderer, frame.sequence())?;
            target_compositions.push(json!({"name":name,"sprite_frame_index":frame_index,"object_ids":snapshot.object_ids,"omissions":snapshot.omissions,"prepare_ms":prepare_ms,"resources":support::pool_info(stats),"frame":support::frame_info(&frame),"comparison":"Diagnostic snapshot capture; Wallpaper Engine fidelity unverified."}));
        }
        let error = decoder
            .image_snapshot(&[28, 72], 0)
            .err()
            .ok_or("expected source-budget rejection")?;
        if !error
            .to_string()
            .contains("remaining composition source budget before decode")
        {
            return Err(error.into());
        }
    }
    renderer.shutdown()?;
    support::write_json(
        &output.join("report.json"),
        &json!({
            "schema_version":6,"status":"passed","consumer":"texture-checks", "generated_cases":results, "generated_regions":region_results,"generated_extracted":extracted_results,"generated_worker":worker_results,"generated_paged":paged_results,"generated_composition":composition_result,"target_identity":target_identity,"target_pages":target,"target_frames":target_frames,"target_extracted":target_extracted,"target_worker":target_worker,"target_paged":target_paged,"target_compositions":target_compositions,
            "limits":["Diagnostic fixed-time sprite sampling, pages/crops and selected composition with partial image effects; no complete scene playback.","Point placement sampling and straight-alpha SDR composition; scene fidelity unverified.","Decode/upload/capture observations are not steady rendering or host latency measurements."]
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
