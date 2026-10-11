//! Generated public composition/snapshot contracts, with real WARP draws.

#[path = "../examples/support/layers.rs"]
mod layers_checks;
#[path = "../examples/support/image_playback.rs"]
mod playback_checks;
#[path = "../examples/support/texture.rs"]
mod texture_checks;

use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::Receiver,
    },
    time::{Duration, Instant},
};
use wallpaper_kit::{
    CloudMotion, CloudMotionSettings, DeviceMode, Frame, GaussianBlur, ImageComposition,
    ImageLayer, ImagePlayback, ImagePlaybackSample, ImageQuality, OutputStats, PlaybackSettings,
    PreciseBlur, Renderer, SceneSource, Size, TextureDecoder, TextureRegion,
};

static RUN: AtomicU64 = AtomicU64::new(0);
struct Package {
    root: PathBuf,
}
impl Package {
    fn new(entries: &[(&str, &[u8])]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "image-contract-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("engine")).unwrap();
        fs::write(root.join("scene.pkg"), texture_checks::package(entries)).unwrap();
        Self { root }
    }
    fn decoder(&self) -> TextureDecoder {
        TextureDecoder::open(&SceneSource {
            package: self.root.join("scene.pkg"),
            engine_assets: self.root.join("engine"),
        })
        .unwrap()
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
fn wait(renderer: &Renderer, events: &Receiver<()>, id: u64) -> Arc<Frame> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(frame) = renderer.try_frame().unwrap()
            && frame.id() == id
        {
            return frame;
        }
        assert!(Instant::now() < deadline, "composition frame timed out");
        let _ = events.recv_timeout(Duration::from_millis(10));
    }
}

fn stats(renderer: &Renderer, sequence: u64) -> OutputStats {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(stats) = renderer.try_output_stats().unwrap()
            && stats.published_frames == sequence
        {
            return stats;
        }
        assert!(Instant::now() < deadline, "composition stats timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn consumer_policy_skips_particle_assets_and_disabled_image_effects() {
    // An invalid particle definition must never be opened by either policy.
    let package = particle_package(json!({"maxcount":5000}), None, json!(true));
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    assert!(ImagePlayback::open_scene_images(&source).is_err());
    for settings in [
        PlaybackSettings {
            visual_effects: false,
            ..Default::default()
        },
        PlaybackSettings {
            reduce_flashing: true,
            ..Default::default()
        },
    ] {
        let playback = ImagePlayback::open_scene_images_with_settings(&source, settings).unwrap();
        assert_eq!(playback.report().settings, settings);
        assert!(playback.report().particle_objects.is_empty());
        assert_eq!(playback_sample(&playback).composition.layers().len(), 1);
        assert!(
            playback
                .report()
                .omissions
                .iter()
                .any(|line| line.contains("consumer"))
        );
        playback.shutdown().unwrap();
    }
    let tex = texture_checks::tex(0, [1, 1], &[255; 4], false, false);
    let mut image = object(1);
    image["effects"] = json!([{"file":"effects/cloudmotion/effect.json"}]);
    let package = snapshot_package(json!([image]), model(), material(false), &tex);
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    assert!(ImagePlayback::open_scene_images(&source).is_err());
    let settings = PlaybackSettings {
        visual_effects: false,
        image_quality: ImageQuality::Linear,
        ..Default::default()
    };
    let playback = ImagePlayback::open_with_settings(&source, &[1], settings).unwrap();
    let sample = playback_sample(&playback);
    assert!(sample.composition.layers()[0].cloud_motion().is_none());
    assert_eq!(
        sample.composition.layers()[0].image_quality(),
        ImageQuality::Linear
    );
    assert!(playback.report().cloud_motions.is_empty());
    playback.shutdown().unwrap();
}

#[test]
fn disabled_animation_holds_sprite_time_but_allows_explicit_stills() {
    let package = Package::new(&[]);
    fs::write(package.root.join("scene.pkg"), playback_checks::package()).unwrap();
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    let settings = PlaybackSettings {
        animation: false,
        ..Default::default()
    };
    let mut playback = ImagePlayback::open_scene_images_with_settings(&source, settings).unwrap();
    playback.play();
    assert!(!playback.playing());
    let initial = playback_sample(&playback);
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(playback.time(), Duration::ZERO);
    assert_eq!(initial.frame_indices, [0, 0]);
    playback.seek(Duration::from_millis(250));
    assert_eq!(playback_sample(&playback).frame_indices, [1, 1]);
    playback.play();
    assert!(!playback.playing());
    assert_eq!(playback.time(), Duration::from_millis(250));
    playback.shutdown().unwrap();
}

#[test]
fn freeze_retains_the_presented_sample_despite_newer_work_and_resumes_from_it() {
    let package = Package::new(&[]);
    fs::write(package.root.join("scene.pkg"), playback_checks::package()).unwrap();
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
    playback.seek(Duration::from_millis(125));
    let presented_sample = playback_sample(&playback);
    let (renderer, events) =
        Renderer::new(Size::new(320, 180).unwrap(), DeviceMode::Software).unwrap();
    let id = renderer
        .render_images(presented_sample.composition.clone())
        .unwrap();
    let held_frame = wait(&renderer, &events, id);
    let pixels = held_frame.readback().unwrap().rgba;
    playback.seek(Duration::from_millis(250));
    let newer = playback_sample(&playback);
    let id = renderer.render_images(newer.composition).unwrap();
    playback.freeze(&presented_sample);
    drop(wait(&renderer, &events, id)); // Consumer discards this newer completion.
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(playback.time(), presented_sample.time);
    assert!(!playback.playing());
    let frozen = playback_sample(&playback);
    assert!(Arc::ptr_eq(
        &frozen.composition,
        &presented_sample.composition
    ));
    assert_eq!(held_frame.readback().unwrap().rgba, pixels);
    let resumed_at = Instant::now();
    playback.play();
    assert!(playback.playing());
    assert!(playback.time() >= presented_sample.time);
    assert!(playback.time() - presented_sample.time <= resumed_at.elapsed());
    playback.pause();
    assert!(!Arc::ptr_eq(
        &playback_sample(&playback).composition,
        &frozen.composition
    ));
    playback.shutdown().unwrap();
    renderer.shutdown().unwrap();
    assert_eq!(held_frame.readback().unwrap().rgba, pixels);
}

#[test]
fn linear_quality_scales_without_reading_neighboring_atlas_texels() {
    let tex = texture_checks::tex(
        0,
        [4, 1],
        &[
            255, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255, 0, 255, 0, 255,
        ],
        false,
        false,
    );
    let package = Package::new(&[("image.tex", &tex)]);
    let page = Arc::new(package.decoder().decode_page("image.tex", 0).unwrap());
    let region = Arc::new(TextureRegion::new(page, [1, 0], [2, 1]).unwrap());
    let (renderer, events) = Renderer::new(Size::new(4, 1).unwrap(), DeviceMode::Software).unwrap();
    for (quality, expected) in [
        (ImageQuality::Nearest, [0, 0, 255, 255]),
        (ImageQuality::Linear, [0, 64, 191, 255]),
    ] {
        let layer = ImageLayer::new(region.clone(), [2., 0.5], [4., 1.], 0., 1., 1.)
            .unwrap()
            .with_image_quality(quality);
        let composition =
            Arc::new(ImageComposition::new(Size::new(4, 1).unwrap(), vec![layer]).unwrap());
        let id = renderer.render_images(composition).unwrap();
        let pixels = wait(&renderer, &events, id).readback().unwrap();
        for (pixel, expected) in pixels.rgba.chunks_exact(4).zip(expected) {
            assert_eq!(pixel, [expected, expected, expected, 255]);
        }
    }
    renderer.shutdown().unwrap();
}

#[test]
fn authored_particles_survive_seek_and_image_layer_order() {
    let scene = serde_json::to_vec(&json!({"version":5,"general":{"orthogonalprojection":{"width":64,"height":64}},"objects":[
        {"id":1,"image":"models/base.json","origin":"32 32 0","size":"64 64"},
        {"id":2,"particle":"particles/stars.json","origin":"32 32 0","visible":{"user":"stars","value":true}},
        {"id":3,"image":"models/base.json","origin":"32 32 0","size":"8 8"}
    ]})).unwrap();
    let stars = br#"{"maxcount":100,"material":"materials/star.json","emitter":[{"name":"boxrandom","rate":20,"distancemax":"30 30 0"}],"initializer":[{"name":"lifetimerandom","min":4,"max":4},{"name":"sizerandom","min":2,"max":2}],"operator":[{"name":"alphafade","fadeintime":0.1,"fadeouttime":0.9}],"renderer":[{"name":"sprite"}]}"#;
    let tex = texture_checks::tex(0, [1, 1], &[255, 255, 255, 255], false, false);
    let package = Package::new(&[
        ("scene.json", &scene), ("particles/stars.json", stars),
        ("models/base.json", br#"{"material":"materials/base.json"}"#),
        ("materials/base.json", br#"{"passes":[{"shader":"genericimage4","blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","combos":{},"textures":["white"]}]}"#),
        ("materials/star.json", br#"{"passes":[{"shader":"genericparticle","blending":"additive","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","textures":["white"]}]}"#),
        ("materials/white.tex", &tex),
    ]);
    let mut playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    playback.seek(Duration::from_secs(3));
    let first = playback.try_sample().unwrap().unwrap();
    // The particle born at exactly three seconds has zero fade-in alpha.
    assert_eq!(first.composition.layers().len(), 61);
    assert_eq!(
        first.composition.layers().first().unwrap().size(),
        [64., 64.]
    );
    assert_eq!(first.composition.layers().last().unwrap().size(), [8., 8.]);
    let positions: Vec<_> = first
        .composition
        .layers()
        .iter()
        .map(|layer| layer.center())
        .collect();
    playback.seek(Duration::from_secs(30));
    assert!(
        playback
            .try_sample()
            .unwrap()
            .unwrap()
            .composition
            .layers()
            .len()
            <= 102
    );
    playback.seek(Duration::from_secs(3));
    let repeated = playback.try_sample().unwrap().unwrap();
    assert_eq!(
        positions,
        repeated
            .composition
            .layers()
            .iter()
            .map(|layer| layer.center())
            .collect::<Vec<_>>()
    );
    playback.shutdown().unwrap();
}

fn particle_package(root: Value, child: Option<Value>, visible: Value) -> Package {
    let scene=serde_json::to_vec(&json!({"version":1,"general":{"orthogonalprojection":{"width":64,"height":64}},"objects":[
        {"id":1,"image":"models/base.json","origin":"32 32 0","size":"64 64","color":"1 1 1","alignment":"center","perspective":false},
        {"id":2,"particle":"particles/root.json","origin":"32 32 0","visible":visible}
    ]})).unwrap();
    let root = serde_json::to_vec(&root).unwrap();
    let child = serde_json::to_vec(&child).unwrap();
    let dark = texture_checks::tex(0, [1, 1], &[32, 32, 32, 255], false, false);
    let white = texture_checks::tex(0, [1, 1], &[255, 255, 255, 255], false, false);
    let mut package_bytes=texture_checks::package(&[
        ("scene.json",&scene),("particles/root.json",&root),("particles/child.json",&child),
        ("models/base.json",br#"{"material":"materials/base.json"}"#),
        ("materials/base.json",br#"{"passes":[{"shader":"genericimage2","blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","combos":{"VERSION":2},"textures":["dark"]}]}"#),
        ("materials/star.json",br#"{"passes":[{"shader":"genericparticle","blending":"additive","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","textures":["white"]}]}"#),
        ("materials/dark.tex",&dark),("materials/white.tex",&white)
    ]);
    package_bytes[4..12].copy_from_slice(b"PKGV0016");
    let package = Package::new(&[]);
    fs::write(package.root.join("scene.pkg"), package_bytes).unwrap();
    package
}

fn burst() -> Value {
    json!({"maxcount":4,"material":"materials/star.json","emitter":[{"name":"sphererandom","instantaneous":2}],"initializer":[
        {"name":"lifetimerandom","min":2,"max":2},{"name":"sizerandom","min":8,"max":8},
        {"name":"colorrandom","min":"255 64 0","max":"255 64 0"},{"name":"alpharandom","min":0.5,"max":0.5}
    ],"renderer":[{"name":"sprite"}]})
}

#[test]
fn particle_sprite_size_matches_the_engine_reference() {
    // Generated engine probe: size 64 spans 64 output pixels at 2x canvas scale.
    let package = particle_package(burst(), None, json!(true));
    let playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    let sample = playback.try_sample().unwrap().unwrap();
    assert_eq!(sample.composition.layers()[1].size(), [4., 4.]);
    playback.shutdown().unwrap();
}

#[test]
fn particle_trail_scales_the_sprite_by_speed_even_without_movement() {
    // Engine probes: length 2, speed 2 stretches a size-64 sprite to 128x32;
    // missing length gives 0.05, with no minimum stretch at low speed.
    for (renderer, speed, expected) in [
        (
            json!({"name":"spritetrail","length":2,"minlength":0,"maxlength":200}),
            2,
            [4., 16.],
        ),
        (json!({"name":"spritetrail"}), 2, [4., 0.4]),
        (json!({"name":"spritetrail"}), 1000, [4., 40.]),
        (
            json!({"name":"spritetrail","length":1,"minlength":1,"maxlength":1}),
            2,
            [4., 4.],
        ),
    ] {
        let mut root = burst();
        root["initializer"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"velocityrandom","min":format!("{speed} 0 0"),"max":format!("{speed} 0 0")}));
        root["renderer"] = json!([renderer]);
        let package = particle_package(root, None, json!(true));
        let mut playback = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .unwrap();
        playback.seek(Duration::from_secs(1));
        let sample = playback.try_sample().unwrap().unwrap();
        let layer = &sample.composition.layers()[1];
        assert_eq!(layer.center(), [32., 32.]);
        for (actual, expected) in layer.size().into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.001,
                "{} != {expected}",
                actual
            );
        }
        assert_eq!(layer.angle_degrees(), 270.);
        playback.shutdown().unwrap();
    }
}

#[test]
fn particle_sprite_sequence_uses_lifetime_and_blends_frames() {
    check_particle_sequence(false);
}

#[test]
fn particle_frame_blending_retains_distinct_pages_and_mirrored_uvs() {
    check_particle_sequence(true);
}

fn check_particle_sequence(separate_pages: bool) {
    let mut root = burst();
    root["emitter"][0]["instantaneous"] = json!(1);
    root["initializer"] = json!([
        {"name":"lifetimerandom","min":12,"max":12},
        {"name":"sizerandom","min":16,"max":16}
    ]);
    root["sequencemultiplier"] = json!(1);
    let package = Package::new(&[]);
    // Install a generated 2-frame atlas in the package instead of white.
    let scene = serde_json::to_vec(
        &json!({"version":5,"general":{"orthogonalprojection":{"width":64,"height":64}},"objects":[
            {"id":1,"image":"models/base.json","origin":"32 32 0","size":"64 64"},
            {"id":2,"particle":"particles/root.json","origin":"32 32 0","scale":if separate_pages {"-1 -1 1"} else {"1 1 1"}}
        ]}),
    )
    .unwrap();
    let pixels: Vec<_> = (0..4)
        .flat_map(|_| {
            (0..8).flat_map(|x| {
                if x < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                }
            })
        })
        .collect();
    let atlas = if separate_pages {
        let mut red = texture_checks::tex(0, [4, 4], &[255, 0, 0, 255].repeat(16), false, false);
        let green = texture_checks::tex(0, [4, 4], &[0, 255, 0, 255].repeat(16), false, false);
        red[55..59].copy_from_slice(&2_u32.to_le_bytes());
        red.extend_from_slice(&green[67..]);
        texture_checks::sprite(
            red,
            &[
                (0, 0.25, [0., 0., 4., 0., 0., 4.]),
                (1, 0.25, [0., 0., 4., 0., 0., 4.]),
            ],
        )
    } else {
        texture_checks::sprite(
            texture_checks::tex(0, [8, 4], &pixels, false, false),
            &[
                (0, 0.25, [0., 0., 4., 0., 0., 4.]),
                (0, 0.25, [4., 0., 4., 0., 0., 4.]),
            ],
        )
    };
    let root = serde_json::to_vec(&root).unwrap();
    let black = texture_checks::tex(0, [1, 1], &[0, 0, 0, 255], false, false);
    let mut bytes = texture_checks::package(&[
        ("scene.json", &scene), ("particles/root.json", &root), ("materials/white.tex", &atlas),
        ("materials/black.tex", &black),
        ("models/base.json", br#"{"material":"materials/base.json"}"#),
        ("materials/base.json", br#"{"passes":[{"shader":"genericimage4","blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","combos":{},"textures":["black"]}]}"#),
        ("materials/star.json", br#"{"passes":[{"shader":"genericparticle","blending":"additive","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","textures":["white"]}]}"#),
    ]);
    bytes[4..12].copy_from_slice(b"PKGV0016");
    fs::write(package.root.join("scene.pkg"), bytes).unwrap();
    let mut playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    let (renderer, events) =
        Renderer::new(Size::new(64, 64).unwrap(), DeviceMode::Software).unwrap();
    for (time, expected) in [
        (0, [255, 0, 0]),
        (3000, [128, 128, 0]),
        (6000, [0, 255, 0]),
        (9000, [128, 128, 0]),
        (3000, [128, 128, 0]),
    ] {
        playback.seek(Duration::from_millis(time));
        let sample = playback.try_sample().unwrap().unwrap();
        let id = renderer.render_images(sample.composition).unwrap();
        let pixels = wait(&renderer, &events, id).readback().unwrap();
        // Sample near the sprite's left edge to avoid atlas-border interpolation.
        let pixel = &pixels.rgba[(32 * 64 + 29) * 4..(32 * 64 + 29) * 4 + 3];
        for (actual, expected) in pixel.iter().zip(expected) {
            assert!(
                (i32::from(*actual) - expected).abs() <= 2,
                "{time}: {pixel:?} != {expected}"
            );
        }
    }
    renderer.shutdown().unwrap();
    playback.shutdown().unwrap();
}

fn render_profile(
    general: Value,
    refract: Option<(f32, [u8; 4])>,
    alpha: f32,
    angle: f32,
    overlay: bool,
) -> Package {
    let mut objects = vec![
        json!({"id":1,"image":"models/base.json","origin":"32 32 0","size":"64 64"}),
        json!({"id":2,"particle":"particles/root.json","origin":"32 32 0","angles":format!("0 0 {angle}")}),
    ];
    if overlay {
        objects.push(json!({"id":3,"image":"models/base.json","origin":"32 32 0","size":"64 64","alpha":0.75}));
    }
    let mut general = general;
    general["orthogonalprojection"] = json!({"width":64,"height":64});
    let scene =
        serde_json::to_vec(&json!({"version":5,"general":general,"objects":objects})).unwrap();
    let mut root = burst();
    root["emitter"][0]["instantaneous"] = json!(1);
    root["initializer"][1] = json!({"name":"sizerandom","min":32,"max":32});
    root["initializer"][2] = json!({"name":"colorrandom","min":"255 255 255","max":"255 255 255"});
    root["initializer"][3] = json!({"name":"alpharandom","min":alpha,"max":alpha});
    let root = serde_json::to_vec(&root).unwrap();
    let mut pass = json!({"shader":"genericparticle","blending":"additive","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","textures":["white"],"constantshadervalues":{"ui_editor_properties_overbright":4}});
    if let Some((amount, _)) = refract {
        pass["blending"] = json!("translucent");
        pass["textures"] = json!(["white", "normal"]);
        pass["combos"] = json!({"REFRACT":1});
        pass["constantshadervalues"] = json!({"ui_editor_properties_refract_amount":amount});
    }
    let material = serde_json::to_vec(&json!({"passes":[pass]})).unwrap();
    let pixels: Vec<_> = (0..64)
        .flat_map(|y| {
            (0..64).flat_map(move |x| {
                if refract.is_some() {
                    [x * 4, y * 4, 0, 255]
                } else {
                    [0, 0, 0, 255]
                }
            })
        })
        .collect();
    let base = texture_checks::tex(0, [64, 64], &pixels, false, false);
    let normal = texture_checks::tex(
        0,
        [1, 1],
        &refract.map_or([128, 128, 255, 255], |(_, normal)| normal),
        false,
        false,
    );
    let white = texture_checks::tex(0, [1, 1], &[255; 4], false, false);
    Package::new(&[("scene.json",&scene),("models/base.json",br#"{"material":"materials/base.json"}"#),
        ("materials/base.json",br#"{"passes":[{"shader":"genericimage4","blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","combos":{},"textures":["base"]}]}"#),
        ("particles/root.json",&root),("materials/star.json",&material),("materials/base.tex",&base),("materials/white.tex",&white),("materials/normal.tex",&normal)])
}

#[test]
fn refraction_displaces_the_prior_scene_with_alpha_rotation_and_resize() {
    let (renderer, events) =
        Renderer::new(Size::new(64, 64).unwrap(), DeviceMode::Software).unwrap();
    for (amount, alpha, angle, normal, size) in [
        (0., 1., 0., [255, 128, 255, 255], [64, 64]),
        (0.125, 1., 0., [255, 128, 255, 255], [64, 64]),
        (0.125, 0.5, 0., [255, 128, 255, 255], [64, 64]),
        (0.125, 1., 90., [255, 128, 255, 255], [128, 96]),
        (0.125, 1., 0., [128, 255, 255, 255], [64, 64]),
    ] {
        let package = render_profile(json!({}), Some((amount, normal)), alpha, angle, false);
        let playback = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .unwrap();
        assert!(
            !playback
                .report()
                .omissions
                .iter()
                .any(|v| v.contains("material combination"))
        );
        let sample = playback.try_sample().unwrap().unwrap();
        let _resize = renderer
            .resize(Size::new(size[0], size[1]).unwrap())
            .unwrap();
        let frame = wait(
            &renderer,
            &events,
            renderer.render_images(sample.composition.clone()).unwrap(),
        );
        let capture = frame.readback().unwrap();
        let (sin, cos) = angle.to_radians().sin_cos();
        let nx = f32::from(normal[0]) / 255. * 2. - 1.;
        let ny = f32::from(normal[1]) / 255. * 2. - 1.;
        let offset = [cos * nx + sin * ny, -sin * nx + cos * ny];
        for y in size[1] / 2 - 4..size[1] / 2 + 4 {
            for x in size[0] / 2 - 4..size[0] / 2 + 4 {
                let source = [
                    (x as f32 + 0.5) * 64. / size[0] as f32 - 0.5,
                    (y as f32 + 0.5) * 64. / size[1] as f32 - 0.5,
                ];
                let original = [source[0].round() * 4., source[1].round() * 4.];
                // Refraction bilinearly samples the rasterized background, which
                // itself uses ordinary image sampling at this output resolution.
                if size == [64, 64] {
                    for channel in 0..2 {
                        let expected =
                            original[channel] + offset[channel] * amount * 64. * 4. * alpha;
                        let actual = capture.rgba[((y * size[0] + x) * 4) as usize + channel];
                        assert!(
                            (f32::from(actual) - expected).abs() <= 1.5,
                            "{angle} {alpha} {channel}: {actual} != {expected}"
                        );
                    }
                } else {
                    let pixel = &capture.rgba[((y * size[0] + x) * 4) as usize..][..4];
                    assert!(
                        pixel[1] < 112 && pixel[0] > 112,
                        "rotated normal must move upward: {pixel:?}"
                    );
                }
            }
        }
        assert_eq!(capture.rgba[0], 0);
        playback.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn hdr_retains_overbright_until_resolve_and_blooms_only_admitted_highlights() {
    let (renderer, events) =
        Renderer::new(Size::new(64, 64).unwrap(), DeviceMode::Software).unwrap();
    for (general, overlay, center, glow) in [
        (json!({"hdr":false}), true, 64, false),
        (json!({"hdr":true,"bloomhdrstrength":0}), true, 255, false),
        // Disabling bloom must retain HDR accumulation without a halo.
        (
            json!({"hdr":true,"bloom":false,"bloomhdrstrength":2}),
            true,
            255,
            false,
        ),
        (
            json!({"hdr":true,"bloom":false,"bloomhdrstrength":2}),
            false,
            255,
            false,
        ),
        (
            json!({"hdr":true,"bloomhdrstrength":1,"bloomhdrthreshold":1,"bloomhdrfeather":0,"bloomhdrscatter":0.7,"bloomhdriterations":3}),
            false,
            255,
            true,
        ),
        (
            json!({"hdr":true,"bloomhdrstrength":1,"bloomhdrthreshold":8,"bloomhdrfeather":0}),
            false,
            255,
            false,
        ),
    ] {
        let package = render_profile(general.clone(), None, 1., 0., overlay);
        let playback = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .unwrap();
        assert_eq!(playback.report().hdr_bloom, general["hdr"] == true);
        let sample = playback.try_sample().unwrap().unwrap();
        let frame = wait(
            &renderer,
            &events,
            renderer.render_images(sample.composition.clone()).unwrap(),
        );
        let capture = frame.readback().unwrap();
        assert!((i32::from(capture.rgba[(32 * 64 + 32) * 4]) - center).abs() <= 1);
        let halo = capture.rgba[(32 * 64 + 50) * 4];
        assert_eq!(halo > 0, glow, "threshold/strength halo {halo}");
        if general["bloom"] == false {
            assert_eq!(
                stats(&renderer, frame.sequence()).effect_texture_bytes,
                64 * 64 * 8,
                "disabled bloom retains only the float accumulation target"
            );
        }
        playback.shutdown().unwrap();
    }
    for general in [
        json!({"hdr":"true"}),
        json!({"hdr":true,"bloom":"false"}),
        json!({"hdr":true,"bloomhdriterations":1.5}),
        json!({"hdr":true,"bloomhdriterations":17}),
        json!({"hdr":true,"bloomhdrstrength":-1}),
    ] {
        let package = render_profile(general, None, 1., 0., false);
        assert!(
            ImagePlayback::open_scene_images(&SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine")
            })
            .is_err()
        );
    }
    renderer.shutdown().unwrap();
}

#[test]
fn death_event_spawns_tinted_additive_children_at_the_rocket_endpoint() {
    let mut root = burst();
    root["emitter"][0]["instantaneous"] = json!(1);
    root["initializer"][0] = json!({"name":"lifetimerandom","min":1,"max":1});
    root["initializer"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"velocityrandom","min":"8 0 0","max":"8 0 0"}));
    root["operator"] = json!([{"name":"movement","gravity":"0 0 0"}]);
    root["renderer"] = json!([{"name":"spritetrail","length":2,"maxlength":20}]);
    root["children"] = json!([{"type":"eventdeath","name":"particles/child.json"}]);
    let package = particle_package(
        root,
        Some(burst()),
        json!({"user":"fireworks","value":true}),
    );
    let mut playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    assert_eq!(playback.report().particle_objects, [2]);
    playback.seek(Duration::from_millis(500));
    let rocket = playback.try_sample().unwrap().unwrap();
    assert_eq!(rocket.composition.layers().len(), 2);
    assert_eq!(rocket.composition.layers()[1].center(), [36., 32.]);
    assert!((rocket.composition.layers()[1].size()[1] - 64.).abs() < 0.001);
    playback.seek(Duration::from_secs(2));
    let hit = playback.try_sample().unwrap().unwrap();
    assert_eq!(hit.composition.layers().len(), 3);
    for spark in &hit.composition.layers()[1..] {
        assert_eq!(spark.center(), [40., 32.]);
    }
    let (renderer, events) =
        Renderer::new(Size::new(64, 64).unwrap(), DeviceMode::Software).unwrap();
    let id = renderer.render_images(hit.composition.clone()).unwrap();
    let pixels = wait(&renderer, &events, id).readback().unwrap();
    let offset = (32 * 64 + 40) * 4;
    for (actual, expected) in pixels.rgba[offset..offset + 4]
        .iter()
        .zip([255, 96, 32, 255])
    {
        assert!((*actual as i32 - expected).abs() <= 1);
    }
    let first_stats = stats(&renderer, 1);
    assert_eq!(first_stats.texture_uploads, 2);
    playback.seek(Duration::from_secs(4));
    let empty = playback.try_sample().unwrap().unwrap();
    assert_eq!(empty.composition.layers().len(), 1);
    let id = renderer.render_images(empty.composition).unwrap();
    let idle = wait(&renderer, &events, id);
    assert_eq!(stats(&renderer, idle.sequence()).texture_uploads, 2);
    playback.seek(Duration::from_secs(2));
    let id = renderer
        .render_images(playback.try_sample().unwrap().unwrap().composition)
        .unwrap();
    let repeated = wait(&renderer, &events, id);
    assert_eq!(pixels.rgba, repeated.readback().unwrap().rgba);
    assert_eq!(stats(&renderer, repeated.sequence()).texture_uploads, 2);
    renderer.shutdown().unwrap();
    playback.shutdown().unwrap();
}

#[test]
fn particles_reject_cycles_invalid_counts_and_skip_hidden_assets() {
    let open = |package: &Package| {
        ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
    };
    let mut cycle = burst();
    cycle["children"] = json!([{"name":"particles/root.json","type":"eventdeath"}]);
    assert!(
        open(&particle_package(cycle, None, json!(true)))
            .err()
            .unwrap()
            .to_string()
            .contains("cyclic")
    );
    let mut count = burst();
    count["maxcount"] = json!(5000);
    assert!(open(&particle_package(count, None, json!(true))).is_err());
    let hidden = particle_package(
        json!({"malformed":true}),
        None,
        json!({"user":"stars","value":false}),
    );
    let playback = open(&hidden).unwrap();
    assert!(playback.report().particle_objects.is_empty());
    assert_eq!(
        playback
            .try_sample()
            .unwrap()
            .unwrap()
            .composition
            .layers()
            .len(),
        1
    );
    playback.shutdown().unwrap();
}

#[test]
fn prewarmed_rope_and_follow_child_share_the_current_particle_position() {
    let mut root = burst();
    root["starttime"] = json!(0.5);
    root["emitter"][0]["instantaneous"] = json!(1);
    root["initializer"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"velocityrandom","min":"8 0 0","max":"8 0 0"}));
    root["operator"] = json!([{"name":"movement","gravity":"0 0 0"}]);
    root["renderer"] = json!([{"name":"ropetrail","length":0.2,"segments":4}]);
    root["children"] = json!([{"type":"eventfollow","name":"particles/child.json"}]);
    let mut child = burst();
    child["emitter"][0]["instantaneous"] = json!(1);
    let package = particle_package(root, Some(child), json!(true));
    let playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    let sample = playback.try_sample().unwrap().unwrap();
    assert_eq!(sample.composition.layers().len(), 6);
    assert_eq!(sample.composition.layers()[5].center(), [36., 32.]);
    for strip in &sample.composition.layers()[1..5] {
        assert!((34.4..36.).contains(&strip.center()[0]));
        assert!((strip.size()[1] - 0.4).abs() < 0.001);
    }
    playback.shutdown().unwrap();
}

#[test]
fn placed_layers_are_ordered_reused_and_retained_across_resize() {
    let cases = texture_checks::cases();
    let entries: Vec<_> = cases
        .iter()
        .map(|case| (case.name, case.tex.as_slice()))
        .collect();
    let package = Package::new(&entries);
    let fixture = layers_checks::Fixture::new(&package.decoder()).unwrap();
    let (renderer, events) =
        Renderer::new(Size::new(320, 180).unwrap(), DeviceMode::Software).unwrap();
    let id = renderer.render_images(fixture.composition.clone()).unwrap();
    let first = wait(&renderer, &events, id);
    fixture.check(&first.readback().unwrap()).unwrap();
    assert_eq!(first.image_layer_count(), Some(6));
    let initial_stats = stats(&renderer, first.sequence());
    assert_eq!(initial_stats.effect_texture_bytes, 6 * 6 * 8);
    assert_eq!(
        (
            initial_stats.texture_uploads,
            initial_stats.source_texture_bytes
        ),
        (3, 86)
    );
    let reversed = fixture
        .composition
        .layers()
        .iter()
        .rev()
        .map(|layer| {
            ImageLayer::new(
                layer.region().clone(),
                layer.center(),
                layer.size(),
                layer.angle_degrees(),
                layer.opacity(),
                layer.brightness(),
            )
            .unwrap()
        })
        .collect();
    let id = renderer
        .render_images(Arc::new(
            ImageComposition::new(fixture.composition.canvas(), reversed).unwrap(),
        ))
        .unwrap();
    let reversed = wait(&renderer, &events, id);
    assert!(
        fixture.check(&reversed.readback().unwrap()).is_err(),
        "oracle must detect wrong ordering"
    );
    assert_eq!(stats(&renderer, reversed.sequence()).texture_uploads, 3);
    drop(reversed);
    let id = renderer.render_images(fixture.composition.clone()).unwrap();
    drop(wait(&renderer, &events, id));
    let id = renderer.resize(Size::new(317, 193).unwrap()).unwrap();
    fixture
        .check(&wait(&renderer, &events, id).readback().unwrap())
        .unwrap();
    let page = fixture.composition.layers()[0].region().page().clone();
    let id = renderer.render_texture_page(page).unwrap();
    let page_frame = wait(&renderer, &events, id);
    texture_checks::check(
        &page_frame.readback().unwrap(),
        [2, 2],
        &texture_checks::rgba(),
    )
    .unwrap();
    let page_stats = stats(&renderer, page_frame.sequence());
    assert_eq!(page_stats.effect_texture_bytes, 0);
    assert_eq!(
        (page_stats.texture_uploads, page_stats.source_texture_bytes),
        (3, 16)
    );
    renderer.shutdown().unwrap();
    fixture.check(&first.readback().unwrap()).unwrap();
}

#[test]
fn gaussian_blur_parameters_masks_and_axis_bypass_match_pixels() {
    let cases = texture_checks::cases();
    let entries: Vec<_> = cases
        .iter()
        .map(|case| (case.name, case.tex.as_slice()))
        .collect();
    let package = Package::new(&entries);
    let mut fixture = layers_checks::Fixture::new(&package.decoder()).unwrap();
    let mask = fixture.composition.layers()[1].region().clone();
    let (renderer, events) =
        Renderer::new(Size::new(64, 48).unwrap(), DeviceMode::Software).unwrap();
    let unblurred = fixture
        .composition
        .layers()
        .iter()
        .map(|layer| {
            ImageLayer::new(
                layer.region().clone(),
                layer.center(),
                layer.size(),
                layer.angle_degrees(),
                layer.opacity(),
                layer.brightness(),
            )
            .unwrap()
        })
        .collect();
    let id = renderer
        .render_images(Arc::new(
            ImageComposition::new(fixture.composition.canvas(), unblurred).unwrap(),
        ))
        .unwrap();
    assert!(
        fixture
            .check(&wait(&renderer, &events, id).readback().unwrap())
            .is_err(),
        "oracle must detect missing blur"
    );
    for sigma in [
        [0., 0.],
        [0., 1.1],
        [1.2, 0.],
        [2.2, 0.3],
        [8., 8.],
        [f32::MIN_POSITIVE / 2., 0.],
    ] {
        for masked in [false, true] {
            let layers = fixture
                .composition
                .layers()
                .iter()
                .map(|layer| {
                    ImageLayer::new(
                        layer.region().clone(),
                        layer.center(),
                        layer.size(),
                        layer.angle_degrees(),
                        layer.opacity(),
                        layer.brightness(),
                    )
                    .unwrap()
                    .with_blur(GaussianBlur::new(sigma, masked.then(|| mask.clone())).unwrap())
                })
                .collect();
            fixture.composition =
                Arc::new(ImageComposition::new(fixture.composition.canvas(), layers).unwrap());
            let id = renderer.render_images(fixture.composition.clone()).unwrap();
            let frame = wait(&renderer, &events, id);
            fixture.check(&frame.readback().unwrap()).unwrap();
            assert_eq!(
                stats(&renderer, frame.sequence()).effect_texture_bytes,
                if sigma == [0., 0.] { 0 } else { 6 * 6 * 8 }
            );
        }
    }
    renderer.shutdown().unwrap();
}

#[test]
fn cloud_motion_time_masks_crops_and_replacement_match_pixels() {
    let cases = texture_checks::cases();
    let entries: Vec<_> = cases
        .iter()
        .map(|case| (case.name, case.tex.as_slice()))
        .collect();
    let package = Package::new(&entries);
    let decoder = package.decoder();
    let mut fixture = layers_checks::Fixture::new(&decoder).unwrap();
    let noise = Arc::new(
        TextureRegion::new(
            Arc::new(decoder.decode_page("r8-raw", 0).unwrap()),
            [1, 0],
            [2, 1],
        )
        .unwrap(),
    );
    let masks = [
        None,
        Some(noise.clone()),
        Some(fixture.composition.layers()[0].region().clone()),
    ];
    let (renderer, events) =
        Renderer::new(Size::new(64, 48).unwrap(), DeviceMode::Software).unwrap();
    for (amount, direction, grain, speed) in [
        (0.2, 0.7, [2., 0.5], 0.02),
        (0.1, 0., [0.5, 0.1], 0.1),
        (0.1, std::f32::consts::FRAC_PI_2, [4., 1.], 0.01),
        (0., 0., [2., 0.5], 0.),
    ] {
        for mask in &masks {
            let motion = Arc::new(CloudMotion::new(
                CloudMotionSettings::new(amount, direction, grain, speed).unwrap(),
                noise.clone(),
                mask.clone(),
            ));
            let mut first_pixels = None;
            for time in [0, 1250, 12000, 50000, 50001] {
                let layers = fixture
                    .composition
                    .layers()
                    .iter()
                    .map(|layer| {
                        let layer = ImageLayer::new(
                            layer.region().clone(),
                            layer.center(),
                            layer.size(),
                            layer.angle_degrees(),
                            layer.opacity(),
                            layer.brightness(),
                        )
                        .unwrap()
                        .with_precise_blur(PreciseBlur::new([1., 1.]).unwrap())
                        .with_cloud_motion(motion.clone(), Duration::from_millis(time));
                        assert!(layer.precise_blur().is_none());
                        assert!(layer.blur().is_none());
                        assert_eq!(layer.cloud_motion().unwrap().1, Duration::from_millis(time));
                        layer
                    })
                    .collect();
                fixture.composition =
                    Arc::new(ImageComposition::new(fixture.composition.canvas(), layers).unwrap());
                let id = renderer.render_images(fixture.composition.clone()).unwrap();
                let frame = wait(&renderer, &events, id);
                let pixels = frame.readback().unwrap();
                fixture.check(&pixels).unwrap();
                assert_eq!(stats(&renderer, frame.sequence()).effect_texture_bytes, 0);
                if time == 0 {
                    first_pixels = Some(pixels);
                }
                if time == 12000 && amount == 0.2 && mask.is_none() {
                    assert!(
                        fixture.check(first_pixels.as_ref().unwrap()).is_err(),
                        "oracle must detect frozen cloud motion"
                    );
                }
            }
        }
    }
    let layer = ImageLayer::new(noise.clone(), [1., 1.], [2., 2.], 0., 1., 1.)
        .unwrap()
        .with_cloud_motion(
            Arc::new(CloudMotion::new(
                CloudMotionSettings::new(0.1, 0., [2., 0.5], 0.02).unwrap(),
                noise,
                None,
            )),
            Duration::ZERO,
        )
        .with_blur(GaussianBlur::new([0., 0.], None).unwrap());
    assert!(layer.cloud_motion().is_none());
    assert!(layer.blur().is_some());
    for amount in [f32::NAN, f32::INFINITY, -0.01, 0.201] {
        assert!(CloudMotionSettings::new(amount, 0., [2., 0.5], 0.02).is_err());
    }
    assert!(CloudMotionSettings::new(0.1, f32::NAN, [2., 0.5], 0.02).is_err());
    for grain in [
        [0.49, 0.5],
        [4.01, 0.5],
        [2., 0.09],
        [2., 1.01],
        [f32::INFINITY, 0.5],
    ] {
        assert!(CloudMotionSettings::new(0.1, 0., grain, 0.02).is_err());
    }
    for speed in [f32::NAN, -0.01, 0.101] {
        assert!(CloudMotionSettings::new(0.1, 0., [2., 0.5], speed).is_err());
    }
    renderer.shutdown().unwrap();
}

#[test]
fn blur_bounds_reject_nonfinite_parameters_and_oversized_scratch() {
    for sigma in [[f32::NAN, 1.], [1., f32::INFINITY], [-1., 0.], [0., 8.01]] {
        assert!(GaussianBlur::new(sigma, None).is_err());
    }
    let wide = texture_checks::tex(9, [8192, 1], &vec![128; 8192], false, false);
    let tall = texture_checks::tex(9, [1, 8192], &vec![128; 8192], false, false);
    let package = Package::new(&[("wide", &wide), ("tall", &tall)]);
    let decoder = package.decoder();
    let layers = ["wide", "tall"]
        .into_iter()
        .map(|name| {
            let page = Arc::new(decoder.decode_page(name, 0).unwrap());
            let extent = page.extent();
            let region = Arc::new(TextureRegion::new(page, [0, 0], extent).unwrap());
            ImageLayer::new(region, [4., 4.], [8., 8.], 0., 1., 1.)
                .unwrap()
                .with_blur(GaussianBlur::new([1., 1.], None).unwrap())
        })
        .collect();
    assert!(matches!(
        ImageComposition::new(Size::new(8, 8).unwrap(), layers),
        Err(wallpaper_kit::Error::InvalidBlur)
    ));
}

#[test]
fn checked_geometry_rejects_invalid_inputs_and_layer_counts() {
    let texture = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    let package = Package::new(&[("image", &texture)]);
    let region = Arc::new(
        TextureRegion::new(
            Arc::new(package.decoder().decode_page("image", 0).unwrap()),
            [0, 0],
            [2, 2],
        )
        .unwrap(),
    );
    for (center, size, angle, opacity, brightness) in [
        ([f32::NAN, 0.], [1., 1.], 0., 1., 1.),
        ([65537., 0.], [1., 1.], 0., 1., 1.),
        ([0., 0.], [0., 1.], 0., 1., 1.),
        ([0., 0.], [-1., 1.], 0., 1., 1.),
        ([0., 0.], [1., 1.], f32::INFINITY, 1., 1.),
        ([0., 0.], [1., 1.], 0., 1.1, 1.),
        ([0., 0.], [1., 1.], 0., 1., 17.),
    ] {
        assert!(ImageLayer::new(region.clone(), center, size, angle, opacity, brightness).is_err());
    }
    let layer = || ImageLayer::new(region.clone(), [1., 1.], [2., 2.], -90., 1., 1.).unwrap();
    assert_eq!(layer().angle_degrees(), 270.);
    let canvas = Size::new(8, 8).unwrap();
    assert!(ImageComposition::new(canvas, vec![]).is_err());
    assert!(ImageComposition::new(canvas, (0..33).map(|_| layer()).collect()).is_err());
    assert_eq!(
        ImageComposition::new(canvas, (0..32).map(|_| layer()).collect())
            .unwrap()
            .layers()
            .len(),
        32
    );
}

#[test]
fn precise_blur_scales_straight_alpha_crops_and_replacement_match_pixels() {
    let cases = texture_checks::cases();
    let entries: Vec<_> = cases
        .iter()
        .map(|case| (case.name, case.tex.as_slice()))
        .collect();
    let package = Package::new(&entries);
    let mut fixture = layers_checks::Fixture::new(&package.decoder()).unwrap();
    let (renderer, events) =
        Renderer::new(Size::new(64, 48).unwrap(), DeviceMode::Software).unwrap();
    for scale in [
        [0., 0.],
        [0.32, 0.32],
        [1., 1.],
        [2., 2.],
        [0., 1.],
        [2., 0.],
        [0.7, 1.3],
    ] {
        let layers = fixture
            .composition
            .layers()
            .iter()
            .map(|layer| {
                ImageLayer::new(
                    layer.region().clone(),
                    layer.center(),
                    layer.size(),
                    layer.angle_degrees(),
                    layer.opacity(),
                    layer.brightness(),
                )
                .unwrap()
                .with_blur(GaussianBlur::new([8., 8.], None).unwrap())
                .with_precise_blur(PreciseBlur::new(scale).unwrap())
            })
            .collect();
        fixture.composition =
            Arc::new(ImageComposition::new(fixture.composition.canvas(), layers).unwrap());
        assert!(
            fixture
                .composition
                .layers()
                .iter()
                .all(|l| l.blur().is_none() && l.precise_blur().unwrap().scale() == scale)
        );
        let id = renderer.render_images(fixture.composition.clone()).unwrap();
        let frame = wait(&renderer, &events, id);
        fixture.check(&frame.readback().unwrap()).unwrap();
        assert_eq!(
            stats(&renderer, frame.sequence()).effect_texture_bytes,
            if scale == [0.; 2] { 0 } else { 288 }
        );
    }
    for scale in [[f32::NAN, 1.], [1., f32::INFINITY], [-0.1, 1.], [2.01, 1.]] {
        assert!(PreciseBlur::new(scale).is_err());
    }
    let layer = &fixture.composition.layers()[0];
    let replaced = ImageLayer::new(
        layer.region().clone(),
        layer.center(),
        layer.size(),
        0.,
        1.,
        1.,
    )
    .unwrap()
    .with_precise_blur(PreciseBlur::new([1.; 2]).unwrap())
    .with_blur(GaussianBlur::new([1.; 2], None).unwrap());
    assert!(replaced.precise_blur().is_none());
    assert!(replaced.blur().is_some());
    renderer.shutdown().unwrap();
}

#[test]
fn unsupported_precise_overrides_fail_before_decode_and_disabled_effects_skip() {
    let corrupt = texture_checks::tex(0, [2, 2], b"not a PNG", false, true);
    for overrides in [
        json!({"combos":{"KERNEL":1}}),
        json!({"textures":[null,null,"mask"]}),
        json!({"constantshadervalues":{"scale":"NaN 1"}}),
    ] {
        let mut image = object(991);
        image["effects"] =
            json!([{"file":"effects/blurprecise/effect.json","passes":[overrides,{}]}]);
        let package = snapshot_package(json!([image]), model(), material(false), &corrupt);
        let error = match package.decoder().image_snapshot(&[991], 0) {
            Ok(_) => panic!("unsupported blur was accepted"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("effects/blurprecise/effect.json")
        );
        assert!(!error.to_string().contains("PNG"));
    }
    let mut image = object(991);
    image["effects"] = json!([{"file":"effects/blurprecise/effect.json","visible":false,"passes":[{"combos":{"KERNEL":9}}]}]);
    let texture = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    let package = snapshot_package(json!([image]), model(), material(false), &texture);
    let snapshot = package.decoder().image_snapshot(&[991], 0).unwrap();
    assert!(snapshot.composition.layers()[0].precise_blur().is_none());
    assert!(
        !snapshot
            .omissions
            .iter()
            .any(|line| line.contains("blurprecise"))
    );
}

#[test]
fn unsupported_cloud_profiles_fail_before_decode_and_disabled_effects_skip() {
    let corrupt = texture_checks::tex(0, [2, 2], b"not a PNG", false, true);
    for overrides in [
        json!({"combos":{"OTHER":1}}),
        json!({"textures":["replacement"]}),
        json!({"constantshadervalues":{"ui_editor_properties_amount":0.3}}),
    ] {
        let mut image = object(991);
        image["effects"] = json!([{"file":"effects/cloudmotion/effect.json","passes":[overrides]}]);
        let package = snapshot_package(json!([image]), model(), material(false), &corrupt);
        let error = package
            .decoder()
            .image_snapshot(&[991], 0)
            .err()
            .expect("unsupported cloud was accepted");
        assert!(
            error
                .to_string()
                .contains("effects/cloudmotion/effect.json")
        );
        assert!(!error.to_string().contains("PNG"));
    }
    let mut image = object(991);
    image["effects"] = json!([{"file":"effects/cloudmotion/effect.json"}]);
    let scene = serde_json::to_vec(&json!({"version":5,"general":{"orthogonalprojection":{"width":16,"height":12}},"objects":[image]})).unwrap();
    let image_model = serde_json::to_vec(&model()).unwrap();
    let image_material = serde_json::to_vec(&material(false)).unwrap();
    let cloud_material = serde_json::to_vec(&json!({"passes":[{"shader":"effects/cloudmotion","blending":"normal","depthtest":"disabled","depthwrite":"disabled","cullmode":"nocull"}]})).unwrap();
    for (pass, expected) in [
        (
            json!({"material":"materials/cloud.json","bind":[{"index":0,"name":"other"}]}),
            "effects/cloudmotion/effect.json",
        ),
        (
            json!({"material":"materials/cloud.json"}),
            "shaders/effects/cloudmotion.vert",
        ),
    ] {
        let definition = serde_json::to_vec(
            &json!({"version":2,"replacementkey":"cloudmotion","passes":[pass]}),
        )
        .unwrap();
        let package = Package::new(&[
            ("scene.json", &scene),
            ("models/image.json", &image_model),
            ("materials/image.json", &image_material),
            ("materials/image.tex", &corrupt),
            ("effects/cloudmotion/effect.json", &definition),
            ("materials/cloud.json", &cloud_material),
            (
                "shaders/effects/cloudmotion.vert",
                b"unverified shader identity",
            ),
        ]);
        let error = package
            .decoder()
            .image_snapshot(&[991], 0)
            .err()
            .expect("unsupported cloud was accepted");
        assert!(error.to_string().contains(expected));
        assert!(!error.to_string().contains("PNG"));
    }
    let mut image = object(991);
    image["effects"] = json!([{"file":"effects/cloudmotion/effect.json","visible":false,"passes":[{"combos":{"OTHER":1}}]}]);
    let texture = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    let package = snapshot_package(json!([image]), model(), material(false), &texture);
    let snapshot = package.decoder().image_snapshot(&[991], 0).unwrap();
    assert!(snapshot.composition.layers()[0].cloud_motion().is_none());
    assert!(
        !snapshot
            .omissions
            .iter()
            .any(|line| line.contains("cloudmotion"))
    );
}

fn object(id: u64) -> Value {
    json!({"id":id,"image":"models/image.json","origin":"4 5 0","size":"2 3","scale":"2 1 1","angles":"0 0 90","alpha":0.5,"brightness":2.})
}
fn material(sprite: bool) -> Value {
    json!({"passes":[{"shader":"genericimage4","textures":["image"],"combos":if sprite {json!({"SPRITESHEET":1})} else {json!({})},"blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled"}]})
}
fn snapshot_package(objects: Value, model: Value, material: Value, texture: &[u8]) -> Package {
    let scene=serde_json::to_vec(&json!({"version":5,"general":{"orthogonalprojection":{"width":16,"height":12}},"objects":objects})).unwrap();
    let model = serde_json::to_vec(&model).unwrap();
    let material = serde_json::to_vec(&material).unwrap();
    Package::new(&[
        ("scene.json", &scene),
        ("models/image.json", &model),
        ("materials/image.json", &material),
        ("materials/image.tex", texture),
    ])
}
fn model() -> Value {
    json!({"autosize":true,"material":"materials/image.json"})
}

#[test]
fn snapshot_preserves_authored_order_shared_pages_crops_and_named_omissions() {
    let texture = texture_checks::sprite(
        texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false),
        &[(0, 0.06, [1., 1., 1., 0., 0., 1.])],
    );
    let mut a = object(1);
    a["effects"] = json!([{"file":"effects/cloud.json"}]);
    a["parallaxDepth"] = json!("1 1");
    let package = snapshot_package(
        json!([a,object(2),{"id":3,"shape":"unsupported"}]),
        model(),
        material(true),
        &texture,
    );
    let snapshot = package.decoder().image_snapshot(&[2, 1], 0).unwrap();
    assert_eq!(snapshot.object_ids, [1, 2]);
    let layers = snapshot.composition.layers();
    assert!(Arc::ptr_eq(
        layers[0].region().page(),
        layers[1].region().page()
    ));
    assert_eq!(layers[0].region().origin(), [1, 1]);
    assert_eq!(layers[0].region().extent(), [1, 1]);
    assert_eq!(
        (
            layers[0].center(),
            layers[0].size(),
            layers[0].angle_degrees(),
            layers[0].opacity(),
            layers[0].brightness()
        ),
        ([4., 5.], [4., 3.], 90., 0.5, 2.)
    );
    for text in [
        "effect effects/cloud.json",
        "parallax",
        "Object 3",
        "camera",
    ] {
        assert!(snapshot.omissions.iter().any(|line| line.contains(text)));
    }
    for selected in [vec![], vec![1, 1], vec![4], vec![3]] {
        assert!(package.decoder().image_snapshot(&selected, 0).is_err());
    }
    assert!(package.decoder().image_snapshot(&[1], 1).is_err());
}

#[test]
fn snapshot_rejects_unverified_selected_behavior() {
    let texture = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    for (key, value) in [
        ("origin", json!("1 1 2")),
        ("angles", json!("1 0 0")),
        ("scale", json!("-1 -1 1")),
        ("size", json!("-2 -3")),
        ("alpha", json!({"script":"x"})),
        ("visible", json!("true")),
        ("parent", json!(2)),
    ] {
        let mut image = object(1);
        image[key] = value;
        let package = snapshot_package(json!([image]), model(), material(false), &texture);
        assert!(package.decoder().image_snapshot(&[1], 0).is_err(), "{key}");
    }
    let mut cropped = model();
    cropped["cropoffset"] = json!("0 -2");
    let package = snapshot_package(json!([object(1)]), cropped, material(true), &texture);
    assert!(
        package
            .decoder()
            .image_snapshot(&[1], 0)
            .err()
            .unwrap()
            .to_string()
            .contains("cropoffset")
    );
    let mut pass = material(false);
    pass["passes"][0]["blending"] = json!("additive");
    let package = snapshot_package(json!([object(1)]), model(), pass, &texture);
    assert!(package.decoder().image_snapshot(&[1], 0).is_err());
}

#[test]
fn static_crop_metadata_preserves_import_adjusted_geometry() {
    let texture = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    for offset in ["0 -2", "3.5 -2.5"] {
        let mut cropped = model();
        cropped["cropoffset"] = json!(offset);
        let package = snapshot_package(json!([object(1)]), cropped, material(false), &texture);
        let snapshot = package.decoder().image_snapshot(&[1], 0).unwrap();
        let layer = &snapshot.composition.layers()[0];
        assert_eq!(layer.center(), [4., 5.]);
        assert_eq!(layer.size(), [4., 3.]);
        assert_eq!(layer.angle_degrees(), 90.);
        assert_eq!(layer.region().origin(), [0, 0]);
        assert_eq!(layer.region().extent(), [2, 2]);
        let playback = ImagePlayback::open(
            &SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine"),
            },
            &[1],
        )
        .unwrap();
        let sample = playback.try_sample().unwrap().unwrap();
        let layer = &sample.composition.layers()[0];
        assert_eq!(layer.center(), [4., 5.]);
        assert_eq!(layer.size(), [4., 3.]);
        assert_eq!(layer.angle_degrees(), 90.);
        playback.shutdown().unwrap();
    }
    let invalid_png = texture_checks::tex(0, [2, 2], b"invalid PNG", false, true);
    for offset in [json!("0"), json!("0 NaN"), json!("0 inf"), json!([0, -2])] {
        let mut cropped = model();
        cropped["cropoffset"] = offset;
        let package = snapshot_package(json!([object(1)]), cropped, material(false), &invalid_png);
        let error = package
            .decoder()
            .image_snapshot(&[1], 0)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("models/image.json"), "{error}");
        assert!(
            !error.contains("PNG"),
            "crop metadata must fail before pixels: {error}"
        );
    }
}

#[test]
fn snapshot_admits_unique_page_bytes_before_decoding_payloads() {
    let small = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    // Exactly 256 MiB declared RGBA; invalid PNG deliberately fails if decoded.
    let large = texture_checks::tex(0, [8192, 8192], b"invalid PNG", false, true);
    let scene=serde_json::to_vec(&json!({"version":5,"general":{"orthogonalprojection":{"width":16,"height":12}},"objects":[object(1),{"id":2,"image":"models/big.json","size":"1 1"}]})).unwrap();
    let small_model = serde_json::to_vec(&model()).unwrap();
    let big_model = br#"{"autosize":true,"material":"materials/big.json"}"#;
    let small_material = serde_json::to_vec(&material(false)).unwrap();
    let mut big_material = material(false);
    big_material["passes"][0]["textures"] = json!(["big"]);
    let big_material = serde_json::to_vec(&big_material).unwrap();
    let package = Package::new(&[
        ("scene.json", &scene),
        ("models/image.json", &small_model),
        ("models/big.json", big_model),
        ("materials/image.json", &small_material),
        ("materials/big.json", &big_material),
        ("materials/image.tex", &small),
        ("materials/big.tex", &large),
    ]);
    let error = package
        .decoder()
        .image_snapshot(&[1, 2], 0)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("remaining composition source budget before decode"),
        "{error}"
    );
}

#[test]
fn snapshot_rejects_geometry_and_appearance_before_payload_decode() {
    let texture = texture_checks::tex(0, [2, 2], b"invalid PNG", false, true);
    for (key, value) in [
        ("origin", json!("70000 0 0")),
        ("size", json!("70000 1")),
        ("scale", json!("3e38 3e38 1")),
        ("alpha", json!(2.)),
        ("brightness", json!(17.)),
    ] {
        let mut image = object(1);
        image[key] = value;
        let package = snapshot_package(json!([image]), model(), material(false), &texture);
        let error = package
            .decoder()
            .image_snapshot(&[1], 0)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("image layer"), "{key}: {error}");
    }
}

fn playback_sample(playback: &ImagePlayback) -> ImagePlaybackSample {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(sample) = playback.try_sample().unwrap() {
            return sample;
        }
        assert!(
            Instant::now() < deadline,
            "combined playback did not become ready"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn automatic_images_discover_authored_ids_and_report_other_content() {
    let scene = serde_json::to_vec(
        &json!({"version":5,"general":{"orthogonalprojection":{"width":16,"height":12}},"objects":[
            object(901), {"id":6,"particle":"particles/test.json"},
            {"id":7,"image":"models/composition.json"}, object(55),
            {"id":8,"image":"missing-hidden.json","visible":false}
        ]}),
    )
    .unwrap();
    let model = serde_json::to_vec(&model()).unwrap();
    let material = serde_json::to_vec(&material(false)).unwrap();
    let texture = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    let package = Package::new(&[
        ("scene.json", &scene),
        ("models/image.json", &model),
        (
            "models/composition.json",
            br#"{"projectlayer":true,"material":"missing-composition.json"}"#,
        ),
        (
            "particles/test.json",
            br#"{"unsupported_system_field":true}"#,
        ),
        ("materials/image.json", &material),
        ("materials/image.tex", &texture),
    ]);
    let playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    assert_eq!(playback.report().object_ids, [901, 55]);
    for omission in ["particles/test.json", "Object 7", "Object 8"] {
        assert!(
            playback
                .report()
                .omissions
                .iter()
                .any(|line| line.contains(omission))
        );
    }
    let sample = playback.try_sample().unwrap().unwrap();
    assert_eq!(sample.composition.layers().len(), 2);
    assert!(Arc::ptr_eq(
        sample.composition.layers()[0].region(),
        sample.composition.layers()[1].region()
    ));
    playback.shutdown().unwrap();
}

#[test]
fn automatic_images_reject_unsupported_images_and_scene_limits_before_decode() {
    let invalid_texture = texture_checks::tex(0, [2, 2], b"invalid PNG", true, true);
    let mut unsupported = material(false);
    unsupported["passes"][0]["shader"] = json!("custom-shader");
    let mut ambiguous = object(4);
    ambiguous["particle"] = json!("particles/test.json");
    for (objects, material, message) in [
        (
            json!([object(4)]),
            unsupported,
            "unsupported image material",
        ),
        (json!([ambiguous]), material(false), "ambiguous"),
        (json!([object(4), object(4)]), material(false), "duplicate"),
        (
            json!((0..33).map(object).collect::<Vec<_>>()),
            material(false),
            "1..=32",
        ),
        (
            json!([{"id":4,"sound":["muted.mp3"]}]),
            material(false),
            "1..=32",
        ),
    ] {
        let package = snapshot_package(objects, model(), material, &invalid_texture);
        let error = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .err()
        .expect("unsupported input must fail");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn playback_rejects_invisible_selections_and_propagates_sprite_failures() {
    let texture = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    let mut hidden = object(1);
    hidden["visible"] = json!(false);
    let package = snapshot_package(json!([hidden]), model(), material(false), &texture);
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    assert!(ImagePlayback::open(&source, &[1]).is_err());

    let invalid = texture_checks::tex(0, [2, 2], b"invalid PNG", false, true);
    let package = snapshot_package(json!([object(1)]), model(), material(true), &invalid);
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    let playback = ImagePlayback::open(&source, &[1]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match playback.try_sample() {
            Ok(None) => (),
            Ok(Some(_)) => panic!("failed sprite must not publish a composition"),
            Err(error) => {
                assert!(error.to_string().contains("materials/image.tex"));
                break;
            }
        }
        assert!(Instant::now() < deadline, "sprite failure was not reported");
        std::thread::sleep(Duration::from_millis(1));
    }
    playback.shutdown().unwrap();
}

#[test]
fn combined_playback_uses_one_clock_shared_sprites_and_authored_order() {
    let package = Package::new(&[]);
    fs::write(package.root.join("scene.pkg"), playback_checks::package()).unwrap();
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    // Selection order must not become draw order.
    let mut playback = ImagePlayback::open(&source, &[4, 3, 2, 1]).unwrap();
    assert!(fs::write(&source.package, b"must remain read-only during playback").is_err());
    assert_eq!(playback.report().object_ids, [1, 2, 3, 4]);
    assert_eq!(
        playback.report().animated_assets,
        ["materials/birds.tex", "materials/character.tex"]
    );
    assert!(
        playback
            .report()
            .omissions
            .iter()
            .any(|line| line.contains("Object 5"))
    );
    assert!(!playback.playing());
    let (renderer, events) =
        Renderer::new(Size::new(320, 180).unwrap(), DeviceMode::Software).unwrap();
    let initial = playback_sample(&playback);
    let static_region = initial.composition.layers()[0].region().clone();
    for time_ms in [0, 124, 125, 249, 250, 374, 375, 500, 1500] {
        playback.seek(Duration::from_millis(time_ms));
        let sample = playback_sample(&playback);
        assert_eq!(sample.time, Duration::from_millis(time_ms));
        assert_eq!(sample.frame_indices, playback_checks::indices(time_ms));
        let layers = sample.composition.layers();
        assert!(Arc::ptr_eq(layers[1].region(), layers[2].region()));
        assert!(Arc::ptr_eq(layers[0].region(), &static_region));
        let id = renderer.render_images(sample.composition).unwrap();
        playback_checks::check(&wait(&renderer, &events, id).readback().unwrap(), time_ms).unwrap();
    }
    let id = renderer.resize(Size::new(317, 193).unwrap()).unwrap();
    playback_checks::check(&wait(&renderer, &events, id).readback().unwrap(), 1500).unwrap();
    let paused = playback.time();
    std::thread::sleep(Duration::from_millis(2));
    assert_eq!(playback.time(), paused);
    playback.play();
    let deadline = Instant::now() + Duration::from_secs(1);
    while playback.time() == paused {
        assert!(Instant::now() < deadline);
    }
    playback.play(); // Idempotent resume preserves elapsed time.
    assert!(playback.time() > paused);
    playback.pause();
    let paused = playback.time();
    assert!(!playback.playing());
    assert_eq!(playback.time(), paused);
    playback.shutdown().unwrap();
    fs::write(&source.package, playback_checks::package()).unwrap(); // Joined asset handles released.
    let id = renderer.render_images(initial.composition).unwrap();
    playback_checks::check(&wait(&renderer, &events, id).readback().unwrap(), 0).unwrap();
    renderer.shutdown().unwrap();
}
