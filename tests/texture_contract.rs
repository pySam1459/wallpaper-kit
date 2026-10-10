//! Public TEX decoding contract with generated payloads.

#[path = "../examples/support/texture.rs"]
mod fixture;

use scene_renderer::{DeviceMode, Frame, Renderer, Size};
use scene_renderer::{SceneSource, TextureDecoder, TextureFormat, TextureRegion};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{
    sync::{Arc, mpsc::Receiver},
    time::{Duration, Instant},
};

static RUN: AtomicU64 = AtomicU64::new(0);

struct Package {
    root: PathBuf,
}

impl Package {
    fn new(texture: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "texture-contract-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("engine")).unwrap();
        fs::write(
            root.join("scene.pkg"),
            fixture::package(&[("image.tex", texture)]),
        )
        .unwrap();
        Self { root }
    }
    fn source(&self) -> SceneSource {
        SceneSource {
            package: self.root.join("scene.pkg"),
            engine_assets: self.root.join("engine"),
        }
    }
}

impl Drop for Package {
    fn drop(&mut self) {
        assert_eq!(
            self.root.canonicalize().unwrap(),
            self.root
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .join(self.root.file_name().unwrap())
        );
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn decodes_raw_lz4_and_png_without_color_or_block_conversion() {
    for (format, extent, data, expected) in [
        (0, [2, 2], fixture::rgba(), TextureFormat::Rgba8),
        (9, [3, 1], vec![0, 128, 255], TextureFormat::R8),
        (4, [8, 8], fixture::bc3(), TextureFormat::Bc3),
    ] {
        for compressed in [false, true] {
            let package = Package::new(&fixture::tex(format, extent, &data, compressed, false));
            let page = TextureDecoder::open(&package.source())
                .unwrap()
                .decode_page("image.tex", 0)
                .unwrap();
            assert_eq!(page.extent(), extent);
            assert_eq!(page.format(), expected);
            assert_eq!(page.data(), data);
        }
    }
    for compressed in [false, true] {
        let rgba = fixture::rgba();
        let png = fixture::png([2, 2], png::ColorType::Rgba, &rgba);
        let package = Package::new(&fixture::tex(0, [2, 2], &png, compressed, true));
        assert_eq!(
            TextureDecoder::open(&package.source())
                .unwrap()
                .decode_page("image.tex", 0)
                .unwrap()
                .data(),
            rgba
        );
    }
}

#[test]
fn expands_png_rgb_and_gray_alpha_to_rgba() {
    for (color, bytes, expected) in [
        (
            png::ColorType::Rgb,
            vec![1, 2, 3, 4, 5, 6],
            vec![1, 2, 3, 255, 4, 5, 6, 255],
        ),
        (
            png::ColorType::GrayscaleAlpha,
            vec![3, 7, 5, 9],
            vec![3, 3, 3, 7, 5, 5, 5, 9],
        ),
    ] {
        let png = fixture::png([2, 1], color, &bytes);
        let package = Package::new(&fixture::tex(0, [2, 1], &png, false, true));
        assert_eq!(
            TextureDecoder::open(&package.source())
                .unwrap()
                .decode_page("image.tex", 0)
                .unwrap()
                .data(),
            expected
        );
    }
}

#[test]
fn rejects_corrupt_payloads_and_png_extent_mismatch() {
    let png = fixture::png([2, 1], png::ColorType::Rgb, &[1; 6]);
    for texture in [
        fixture::tex(0, [2, 2], &[0; 16], false, true),
        fixture::tex(0, [2, 2], &png, false, true),
        fixture::tex(0, [2, 2], &[0; 4], true, false),
    ] {
        let package = Package::new(&texture);
        let error = TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .unwrap_err();
        assert!(error.to_string().starts_with("image.tex:"));
    }
    // Invalid LZ4 back-reference offset zero, while metadata remains valid.
    let mut texture = fixture::tex(0, [2, 2], &[0; 16], true, false);
    texture.truncate(91);
    texture[87..91].copy_from_slice(&3_u32.to_le_bytes());
    texture.extend_from_slice(&[0, 0, 0]);
    let package = Package::new(&texture);
    assert!(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .is_err()
    );
}

#[test]
fn rejects_missing_assets_pages_and_allocation_bombs() {
    let texture = fixture::tex(0, [2, 2], &fixture::rgba(), false, false);
    let package = Package::new(&texture);
    assert!(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("missing.tex", 0)
            .is_err()
    );
    assert!(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("../image.tex", 0)
            .is_err()
    );
    assert!(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("image.tex", 1)
            .is_err()
    );
    let png = fixture::png([1, 1], png::ColorType::Rgba, &[0; 4]);
    let oversized = Package::new(&fixture::tex(0, [16_384, 16_384], &png, false, true));
    assert!(
        TextureDecoder::open(&oversized.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
    let mut texture = fixture::tex(0, [2, 2], &png, true, true);
    texture[83..87].copy_from_slice(&u32::MAX.to_le_bytes());
    let oversized = Package::new(&texture);
    assert!(
        TextureDecoder::open(&oversized.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .is_err()
    );
}

#[test]
fn decodes_only_the_selected_base_mip_and_checks_unselected_ranges() {
    let png = fixture::png([2, 2], png::ColorType::Rgba, &fixture::rgba());
    let first = fixture::tex(0, [2, 2], b"invalid PNG", false, true);
    let second = fixture::tex(0, [2, 2], &png, false, true);
    let mut texture = first;
    texture[55..59].copy_from_slice(&2_u32.to_le_bytes());
    texture.extend_from_slice(&second[67..]);
    let package = Package::new(&texture);
    assert!(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .is_err()
    );
    assert_eq!(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("image.tex", 1)
            .unwrap()
            .data(),
        fixture::rgba()
    );

    let mut texture = second;
    texture[67..71].copy_from_slice(&2_u32.to_le_bytes());
    let mip = fixture::tex(0, [1, 1], b"invalid PNG", false, true);
    texture.extend_from_slice(&mip[71..]);
    let package = Package::new(&texture);
    assert_eq!(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .unwrap()
            .data(),
        fixture::rgba()
    );
    texture.pop();
    let truncated = Package::new(&texture);
    assert!(
        TextureDecoder::open(&truncated.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .is_err()
    );
}

fn wait_frame(renderer: &Renderer, events: &Receiver<()>, id: u64) -> Arc<Frame> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(frame) = renderer.try_frame().unwrap()
            && frame.id() == id
        {
            return frame;
        }
        assert!(Instant::now() < deadline, "texture frame timed out");
        let _ = events.recv_timeout(Duration::from_millis(10));
    }
}

fn wait_stats(renderer: &Renderer, frame: &Frame) -> scene_renderer::OutputStats {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(stats) = renderer.try_output_stats().unwrap()
            && stats.published_frames == frame.sequence()
        {
            return stats;
        }
        assert!(Instant::now() < deadline, "crop counters timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn gpu_samples_all_storage_formats_and_preserves_old_frames() {
    let (renderer, events) =
        Renderer::new(Size::new(32, 24).unwrap(), DeviceMode::Software).unwrap();
    for case in fixture::cases() {
        let package = Package::new(&case.tex);
        let page = Arc::new(
            TextureDecoder::open(&package.source())
                .unwrap()
                .decode_page("image.tex", 0)
                .unwrap(),
        );
        let id = renderer.render_texture_page(page.clone()).unwrap();
        let first = wait_frame(&renderer, &events, id);
        assert_eq!(first.texture_format(), Some(page.format()));
        assert!(first.fixture_time().is_none());
        fixture::check(&first.readback().unwrap(), case.extent, &case.rgba)
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        for _ in 0..12 {
            let id = renderer.render_texture_page(page.clone()).unwrap();
            let frame = wait_frame(&renderer, &events, id);
            fixture::check(&frame.readback().unwrap(), case.extent, &case.rgba).unwrap();
        }
        let id = renderer.resize(Size::new(31, 23).unwrap()).unwrap();
        let resized = wait_frame(&renderer, &events, id);
        fixture::check(&resized.readback().unwrap(), case.extent, &case.rgba).unwrap();
        fixture::check(&first.readback().unwrap(), case.extent, &case.rgba).unwrap();
        drop(resized);
        drop(first);
        let id = renderer.resize(Size::new(32, 24).unwrap()).unwrap();
        drop(wait_frame(&renderer, &events, id));
    }
    renderer.shutdown().unwrap();
}

#[test]
fn texture_oracle_detects_orientation_alpha_and_channel_faults() {
    let case = fixture::cases().remove(0);
    let mut rgba = case.rgba.clone();
    // Direct expected composition for the 2x2 corner fixture.
    rgba[4..8].copy_from_slice(&[8, 140, 16, 255]);
    rgba[8..12].copy_from_slice(&[16, 24, 32, 255]);
    let mut pixels = scene_renderer::Pixels {
        size: Size::new(2, 2).unwrap(),
        rgba,
    };
    fixture::check(&pixels, case.extent, &case.rgba).unwrap();
    let original = pixels.rgba.clone();
    pixels.rgba.rotate_left(8);
    assert!(fixture::check(&pixels, case.extent, &case.rgba).is_err());
    pixels.rgba = original;
    pixels.rgba[3] = 254;
    assert!(fixture::check(&pixels, case.extent, &case.rgba).is_err());
    pixels.rgba[3] = 255;
    pixels.rgba.swap(0, 2);
    assert!(fixture::check(&pixels, case.extent, &case.rgba).is_err());
    pixels.rgba.swap(0, 2);
    pixels.rgba[8..12].copy_from_slice(&[0, 0, 255, 255]);
    assert!(fixture::check(&pixels, case.extent, &case.rgba).is_err());
}

#[test]
fn sprite_metadata_retains_order_page_rectangles_and_variable_durations() {
    let first = fixture::tex(0, [2, 2], &fixture::rgba(), false, false);
    let mut texture = first.clone();
    texture[55..59].copy_from_slice(&2_u32.to_le_bytes());
    texture.extend_from_slice(&first[67..]);
    let texture = fixture::sprite(
        texture,
        &[
            (0, 0.06, [1., 0., 1., 0., 0., 2.]),
            (1, 0.12, [0., 1., 2., 0., 0., 1.]),
        ],
    );
    let package = Package::new(&texture);
    let decoder = TextureDecoder::open(&package.source()).unwrap();
    let frames = decoder.sprite_frames("image.tex").unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].page_index(), 0);
    assert_eq!(frames[0].origin(), [1, 0]);
    assert_eq!(frames[0].extent(), [1, 2]);
    assert!((frames[0].duration().as_secs_f64() - 0.06).abs() < 1e-8);
    assert_eq!(frames[1].page_index(), 1);
    assert_eq!(frames[1].origin(), [0, 1]);
    assert_eq!(frames[1].extent(), [2, 1]);
    assert!((frames[1].duration().as_secs_f64() - 0.12).abs() < 1e-8);
    assert_eq!(
        decoder.decode_page("image.tex", 1).unwrap().data(),
        fixture::rgba()
    );
}

#[test]
fn rejects_unsupported_sprite_rectangles_and_invalid_regions() {
    let base = fixture::tex(0, [2, 2], &fixture::rgba(), false, false);
    let static_package = Package::new(&base);
    let decoder = TextureDecoder::open(&static_package.source()).unwrap();
    assert!(
        decoder
            .sprite_frames("image.tex")
            .unwrap_err()
            .to_string()
            .contains("no sprite frames")
    );
    let page = Arc::new(decoder.decode_page("image.tex", 0).unwrap());
    for (origin, extent) in [
        ([0, 0], [0, 1]),
        ([2, 0], [1, 1]),
        ([0, 1], [2, 2]),
        ([u32::MAX, 0], [2, 1]),
    ] {
        assert!(matches!(
            TextureRegion::new(page.clone(), origin, extent),
            Err(scene_renderer::Error::InvalidTextureRegion)
        ));
    }
    for rectangle in [
        [0.5, 0., 1., 0., 0., 1.], // fractional
        [1., 0., -1., 0., 0., 1.], // mirrored
        [0., 1., 0., -1., 1., 0.], // rotated
        [0., 0., 1., 0., 0., 0.],  // degenerate
        [0., 0., 2., 0., 0., 3.],  // outside
        [f32::NAN, 0., 1., 0., 0., 1.],
    ] {
        let package = Package::new(&fixture::sprite(base.clone(), &[(0, 0.06, rectangle)]));
        let error = TextureDecoder::open(&package.source())
            .unwrap()
            .sprite_frames("image.tex")
            .unwrap_err();
        assert!(error.to_string().starts_with("image.tex:"));
    }
    for (page_index, duration) in [
        (1, 0.06),
        (0, 0.),
        (0, f32::INFINITY),
        (0, f32::from_bits(1)),
    ] {
        let package = Package::new(&fixture::sprite(
            base.clone(),
            &[(page_index, duration, [0., 0., 2., 0., 0., 2.])],
        ));
        assert!(
            TextureDecoder::open(&package.source())
                .unwrap()
                .sprite_frames("image.tex")
                .is_err()
        );
    }
}

#[test]
fn gpu_crops_all_formats_and_reuses_source_across_region_changes() {
    let (renderer, events) =
        Renderer::new(Size::new(31, 23).unwrap(), DeviceMode::Software).unwrap();
    for case in fixture::cases() {
        let package = Package::new(&case.tex);
        let decoder = TextureDecoder::open(&package.source()).unwrap();
        let page = Arc::new(decoder.decode_page("image.tex", 0).unwrap());
        let id = renderer.render_texture_page(page.clone()).unwrap();
        let frame = wait_frame(&renderer, &events, id);
        let uploads = wait_stats(&renderer, &frame).texture_uploads;
        drop(frame);
        // BC3 cases deliberately begin inside a block and cross into its neighbours.
        for (origin, extent) in [
            ([1, 0], [case.extent[0] - 1, case.extent[1]]),
            ([0, case.extent[1] - 1], [case.extent[0], 1]),
        ] {
            let rgba = fixture::crop(&case.rgba, case.extent, origin, extent);
            let region = Arc::new(TextureRegion::new(page.clone(), origin, extent).unwrap());
            let id = renderer.render_texture_region(region.clone()).unwrap();
            let first = wait_frame(&renderer, &events, id);
            assert_eq!(first.texture_region(), Some((origin, extent)));
            fixture::check(&first.readback().unwrap(), extent, &rgba).unwrap();
            let id = renderer.resize(Size::new(17, 13).unwrap()).unwrap();
            let resized = wait_frame(&renderer, &events, id);
            fixture::check(&resized.readback().unwrap(), extent, &rgba).unwrap();
            fixture::check(&first.readback().unwrap(), extent, &rgba).unwrap();
            let stats = wait_stats(&renderer, &resized);
            assert_eq!(stats.texture_uploads, uploads);
            assert_eq!(stats.source_texture_bytes, page.data().len() as u64);
            assert!(stats.resident_textures <= 3);
            drop(first);
            drop(resized);
            let id = renderer.resize(Size::new(31, 23).unwrap()).unwrap();
            drop(wait_frame(&renderer, &events, id));
        }
    }
    renderer.shutdown().unwrap();
}

#[test]
fn sprite_stream_uses_variable_durations_and_reloads_only_page_transitions() {
    let first = fixture::tex(0, [2, 2], &fixture::rgba(), false, false);
    let mut texture = first.clone();
    texture[55..59].copy_from_slice(&2_u32.to_le_bytes());
    texture.extend_from_slice(&first[67..]);
    let texture = fixture::sprite(
        texture,
        &[
            (0, 0.125, [1., 0., 1., 0., 0., 2.]),
            (0, 0.25, [0., 1., 2., 0., 0., 1.]),
            (1, 0.125, [0., 0., 2., 0., 0., 2.]),
        ],
    );
    let package = Package::new(&texture);
    let mut stream = package_decoder_stream(&package);
    assert_eq!(stream.duration(), Duration::from_millis(500));
    assert_eq!(stream.stats().page_decodes, 0);
    for (nanos, index) in [
        (0, 0),
        (124_999_999, 0),
        (125_000_000, 1),
        (374_999_999, 1),
        (375_000_000, 2),
        (499_999_999, 2),
        (500_000_000, 0),
        (1_125_000_000, 1),
    ] {
        assert_eq!(stream.frame_index_at(Duration::from_nanos(nanos)), index);
    }
    assert_eq!(stream.frame_index_at(Duration::MAX), 2);
    let first = stream.sample(Duration::ZERO).unwrap();
    let again = stream.sample(Duration::from_millis(124)).unwrap();
    assert!(Arc::ptr_eq(first.region(), again.region()));
    assert_eq!(stream.stats().frame_extracts, 1);
    stream.sample(Duration::from_millis(125)).unwrap();
    assert_eq!(stream.stats().page_decodes, 1);
    stream.sample(Duration::from_millis(375)).unwrap();
    assert_eq!(stream.stats().page_decodes, 2);
    stream.sample(Duration::from_millis(500)).unwrap();
    assert_eq!(stream.stats().page_decodes, 3);
    assert_eq!(
        first.region().page().data(),
        fixture::crop(&fixture::rgba(), [2, 2], [1, 0], [1, 2])
    );
    assert_eq!(stream.stats().cached_page_bytes, 16);
    assert_eq!(stream.stats().cached_frame_bytes, 8);
}

fn package_decoder_stream(package: &Package) -> scene_renderer::SpriteStream {
    // The stream must remain usable after its decoder is dropped.
    TextureDecoder::open(&package.source())
        .unwrap()
        .sprite_stream("image.tex")
        .unwrap()
}

#[test]
fn extracted_sprite_gpu_pixels_match_whole_atlas_crops_in_all_storage_formats() {
    let (renderer, events) =
        Renderer::new(Size::new(31, 23).unwrap(), DeviceMode::Software).unwrap();
    let mut cases = fixture::cases();
    cases.push(fixture::Case {
        name: "bc3-partial-edge",
        extent: [7, 5],
        tex: fixture::sprite(
            fixture::tex(4, [7, 5], &fixture::bc3(), false, false),
            &[(0, 0.125, [5., 3., 2., 0., 0., 2.])],
        ),
        rgba: fixture::crop(
            &fixture::cases()
                .into_iter()
                .find(|case| case.name == "bc3-raw")
                .unwrap()
                .rgba,
            [8, 8],
            [0, 0],
            [7, 5],
        ),
    });
    for case in cases {
        let package = Package::new(&case.tex);
        let mut stream = package_decoder_stream(&package);
        let frames = stream.frames().to_vec();
        let mut time = Duration::ZERO;
        for (index, authored) in frames.iter().enumerate() {
            let sample = stream.sample(time).unwrap();
            assert_eq!(sample.frame_index(), index);
            let expected = fixture::crop(
                &case.rgba,
                case.extent,
                authored.origin(),
                authored.extent(),
            );
            let id = renderer
                .render_texture_region(sample.region().clone())
                .unwrap();
            let frame = wait_frame(&renderer, &events, id);
            fixture::check(&frame.readback().unwrap(), authored.extent(), &expected).unwrap();
            let uploads = wait_stats(&renderer, &frame).texture_uploads;
            let sample2 = stream.sample(time + authored.duration() / 2).unwrap();
            assert!(Arc::ptr_eq(sample.region(), sample2.region()));
            let id = renderer
                .render_texture_region(sample2.region().clone())
                .unwrap();
            let repeated = wait_frame(&renderer, &events, id);
            assert_eq!(wait_stats(&renderer, &repeated).texture_uploads, uploads);
            drop(repeated);
            let id = renderer.resize(Size::new(17, 13).unwrap()).unwrap();
            let resized = wait_frame(&renderer, &events, id);
            fixture::check(&resized.readback().unwrap(), authored.extent(), &expected).unwrap();
            fixture::check(&frame.readback().unwrap(), authored.extent(), &expected).unwrap();
            assert_eq!(
                wait_stats(&renderer, &resized).source_texture_bytes,
                sample.region().page().data().len() as u64
            );
            time += authored.duration();
        }
        assert_eq!(stream.sample(time).unwrap().frame_index(), 0);
        assert_eq!(stream.stats().page_decodes, 1);
    }
    renderer.shutdown().unwrap();
}

#[test]
fn sprite_extraction_rejects_large_frames_before_decoding_and_keeps_asset_locked() {
    let texture = fixture::sprite(
        fixture::tex(0, [8192, 8192], b"invalid PNG", false, true),
        &[(0, 0.125, [0., 0., 8192., 0., 0., 8192.])],
    );
    let package = Package::new(&texture);
    let mut stream = package_decoder_stream(&package);
    let error = stream.sample(Duration::ZERO).unwrap_err().to_string();
    assert!(
        error.contains("extracted sprite frame exceeds 64 MiB before atlas decode"),
        "{error}"
    );
    assert_eq!(stream.stats().page_decodes, 0);
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(package.root.join("scene.pkg"))
            .is_err()
    );
    drop(stream);
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(package.root.join("scene.pkg"))
            .is_ok()
    );
}

#[test]
fn crop_oracle_uses_expected_rows_and_detects_wrong_region() {
    let rgba = fixture::rgba();
    assert_eq!(
        fixture::crop(&rgba, [2, 2], [1, 0], [1, 2]),
        [[0, 255, 0, 128], [240, 160, 80, 255]].concat()
    );
    let (renderer, events) = Renderer::new(Size::new(7, 5).unwrap(), DeviceMode::Software).unwrap();
    let package = Package::new(&fixture::tex(0, [2, 2], &rgba, false, false));
    let page = Arc::new(
        TextureDecoder::open(&package.source())
            .unwrap()
            .decode_page("image.tex", 0)
            .unwrap(),
    );
    let region = Arc::new(TextureRegion::new(page, [0, 0], [1, 2]).unwrap());
    let id = renderer.render_texture_region(region).unwrap();
    assert!(
        fixture::check(
            &wait_frame(&renderer, &events, id).readback().unwrap(),
            [1, 2],
            &fixture::crop(&rgba, [2, 2], [1, 0], [1, 2])
        )
        .is_err()
    );
    renderer.shutdown().unwrap();
}
