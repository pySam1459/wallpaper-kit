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

fn masked_motion_package(kind: &str, constants: Value, combos: Value) -> Package {
    motion_package(kind, constants, combos, None, None, false)
}

fn motion_package(
    kind: &str,
    constants: Value,
    combos: Value,
    phase: Option<u8>,
    opacity: Option<u8>,
    hdr: bool,
) -> Package {
    let textures = if matches!(kind, "foliagesway" | "waterripple") {
        json!([null, "mask", phase.map(|_| "phase")])
    } else if kind == "shake" {
        json!([
            null,
            "mask",
            phase.map(|_| "phase"),
            opacity.map(|_| "opacity")
        ])
    } else {
        json!([null, "mask"])
    };
    let scene = serde_json::to_vec(&json!({"version":5,"general":{"hdr":hdr,"orthogonalprojection":{"width":8,"height":8}},"objects":[{
        "id":1,"image":"models/image.json","origin":"4 4 0","size":"8 8","effects":[{
            "file":format!("effects/{kind}/effect.json"),"passes":[{"constantshadervalues":constants,"combos":combos,"textures":textures}]
        }]
    }]})).unwrap();
    let definition = serde_json::to_vec(
        &json!({"version":1,"replacementkey":kind,"passes":[{"material":"materials/effect.json"}]}),
    )
    .unwrap();
    let effect_material =
        serde_json::to_vec(&json!({"passes":[{"shader":format!("effects/{kind}")}]})).unwrap();
    let pixels: Vec<_> = (0..64)
        .flat_map(|i| {
            [
                (i % 8 * 31) as u8,
                (i / 8 * 31) as u8,
                if kind == "shake" && i % 2 == 1 {
                    200
                } else {
                    0
                },
                255,
            ]
        })
        .collect();
    let mask: Vec<_> = if kind == "shake" {
        (0..64)
            .flat_map(|i| [if i % 8 < 4 { 127 } else { 255 }, 127, 0, 255])
            .collect()
    } else {
        (0..64).map(|i| if i % 8 < 4 { 0 } else { 255 }).collect()
    };
    Package::new(&[
        ("scene.json", &scene),
        ("models/image.json", &serde_json::to_vec(&model()).unwrap()),
        (
            "materials/image.json",
            &serde_json::to_vec(&material(false)).unwrap(),
        ),
        ("materials/effect.json", &effect_material),
        (&format!("effects/{kind}/effect.json"), &definition),
        (
            "materials/image.tex",
            &texture_checks::tex(0, [8, 8], &pixels, false, false),
        ),
        (
            "materials/mask.tex",
            &texture_checks::tex(
                if kind == "shake" { 0 } else { 9 },
                [8, 8],
                &mask,
                false,
                false,
            ),
        ),
        (
            "materials/phase.tex",
            &if kind == "waterripple" {
                texture_checks::tex(
                    0,
                    [1, 1],
                    &[phase.unwrap_or(255), 128, 255, 255],
                    false,
                    false,
                )
            } else if kind == "foliagesway" {
                texture_checks::tex(0, [1, 1], &[17, phase.unwrap_or(0), 255, 255], false, false)
            } else {
                texture_checks::tex(9, [1, 1], &[phase.unwrap_or(0)], false, false)
            },
        ),
        (
            "materials/util/noise.tex",
            &texture_checks::tex(0, [1, 1], &[17, 64, 255, 255], false, false),
        ),
        (
            "materials/effects/waterripplenormal.tex",
            &texture_checks::tex(
                0,
                [2, 2],
                &[
                    255, 128, 240, 255, 128, 255, 240, 255, 0, 128, 240, 255, 128, 0, 240, 255,
                ],
                false,
                false,
            ),
        ),
        (
            "materials/opacity.tex",
            &texture_checks::tex(9, [1, 1], &[opacity.unwrap_or(255)], false, false),
        ),
    ])
}

#[test]
fn masked_spin_and_water_waves_animate_rewind_and_preserve_unmasked_pixels() {
    for (kind, constants) in [
        (
            "spin",
            json!({"center":"0.5 0.5","speed":std::f32::consts::FRAC_PI_2}),
        ),
        (
            "waterwaves",
            json!({"direction":0,"scale":0,"speed":std::f32::consts::FRAC_PI_2,"strength":0.2,"perspective":0}),
        ),
    ] {
        let package = masked_motion_package(kind, constants, json!({}));
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
        assert!(
            !playback
                .report()
                .omissions
                .iter()
                .any(|s| s.contains(&format!("effect effects/{kind}/")))
        );
        let (renderer, events) =
            Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
        let mut captures = Vec::new();
        for seconds in [0, 1, 0] {
            playback.seek(Duration::from_secs(seconds));
            let sample = playback_sample(&playback);
            let id = renderer.render_images(sample.composition).unwrap();
            captures.push(wait(&renderer, &events, id).readback().unwrap().rgba);
        }
        assert_eq!(captures[0], captures[2], "{kind} rewind");
        assert_ne!(captures[0], captures[1], "{kind} must change pixels");
        for y in 0..8 {
            for x in 0..4 {
                let i = (y * 8 + x) * 4;
                assert_eq!(
                    &captures[0][i..i + 4],
                    &captures[1][i..i + 4],
                    "{kind} mask at {x},{y}"
                );
            }
        }
        // Independent cardinal-turn oracle: UV (5.5,1.5) samples (1.5,2.5).
        if kind == "spin" {
            let i = (8 + 5) * 4;
            assert_eq!(&captures[1][i..i + 4], &[31, 62, 0, 255]);
        }
        renderer.shutdown().unwrap();
        playback.shutdown().unwrap();
    }
}

#[test]
fn image_motion_rejects_bad_profiles_and_respects_effect_policy() {
    for (kind, constants, combos) in [
        ("spin", json!({"center":"2 0.5"}), json!({})),
        ("spin", json!({"speed":"fast"}), json!({})),
        ("waterwaves", json!({"strength":-1}), json!({})),
        ("waterwaves", json!({"perspective":1}), json!({})),
        ("waterwaves", json!({}), json!({"DUALWAVES":1})),
        ("shake", json!({"strength":-1}), json!({})),
        ("shake", json!({"friction":"2 1"}), json!({})),
        ("shake", json!({"bounds":"0 0.5"}), json!({})),
        ("shake", json!({}), json!({"NOISE":1})),
        ("shake", json!({}), json!({"DIRECTION":1})),
        ("shake", json!({}), json!({"AUDIOPROCESSING":3})),
        ("shake", json!({}), json!({"UNKNOWN":0})),
    ] {
        let package = masked_motion_package(kind, constants, combos);
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        assert!(ImagePlayback::open_scene_images(&source).is_err());
        let playback = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                allow_unsupported_features: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(playback.report().image_motions.is_empty());
        assert!(
            playback
                .report()
                .omissions
                .iter()
                .any(|s| s.contains("effect") && s.contains("omitted"))
        );
        playback.shutdown().unwrap();
        // Disabled VFX must bypass unsupported profiles before preparing masks.
        let playback = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                visual_effects: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(playback.report().image_motions.is_empty());
        playback.shutdown().unwrap();
    }
}

#[test]
fn periodic_shake_is_discovered_and_rewinds() {
    let package = masked_motion_package(
        "shake",
        json!({"speed":1,"strength":0.5,"friction":"1 1","bounds":"0 1"}),
        json!({"AUDIOPROCESSING":0,"DIRECTION":0,"NOISE":0}),
    );
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
    assert_eq!(playback.report().image_motions.get(&1).unwrap(), "shake");
    let (renderer, events) = Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
    let mut captures = Vec::new();
    for seconds in [0, 1, 0] {
        playback.seek(Duration::from_secs(seconds));
        let id = renderer
            .render_images(playback_sample(&playback).composition)
            .unwrap();
        captures.push(wait(&renderer, &events, id).readback().unwrap().rgba);
    }
    assert_ne!(captures[0], captures[1]);
    assert_eq!(captures[0], captures[2]);
    renderer.shutdown().unwrap();
    playback.shutdown().unwrap();
}

#[test]
fn planar_water_ripple_combines_normals_with_squared_masked_strength() {
    let (renderer, events) = Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
    for strength in [0., 0.25, 0.5] {
        let package = motion_package(
            "waterripple",
            json!({"ripplestrength":strength}),
            json!({}),
            Some(255),
            None,
            false,
        );
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        let playback = ImagePlayback::open_scene_images(&source).unwrap();
        assert_eq!(
            playback.report().image_motions.get(&1).unwrap(),
            "waterripple"
        );
        let id = renderer
            .render_images(playback_sample(&playback).composition)
            .unwrap();
        let pixels = wait(&renderer, &events, id).readback().unwrap().rgba;
        // Two encoded normals combine to (2, 2/255, 1), then normalize.
        let normal = [2_f64, 2. / 255., 1.];
        let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
        let amount = 8. * strength * strength;
        let expected = [
            (5. + amount * normal[0] / length).min(7.) * 31.,
            (1. + amount * normal[1] / length) * 31.,
            0.,
            255.,
        ];
        for (channel, value) in expected.into_iter().enumerate() {
            assert!((f64::from(pixels[(8 + 5) * 4 + channel]) - value).abs() <= 1.);
        }
        assert_eq!(&pixels[(8 + 1) * 4..][..4], &[31, 31, 0, 255]);
        playback.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn planar_water_ripple_animates_rewinds_and_respects_hdr_and_vfx_policy() {
    let (renderer, events) = Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
    for hdr in [false, true] {
        let package = motion_package(
            "waterripple",
            json!({"ripplestrength":0.4,"animationspeed":0.5}),
            json!({}),
            None,
            None,
            hdr,
        );
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
        let mut frames = Vec::new();
        for millis in [0, 1500, 0] {
            playback.seek(Duration::from_millis(millis));
            let id = renderer
                .render_images(playback_sample(&playback).composition)
                .unwrap();
            frames.push(wait(&renderer, &events, id).readback().unwrap().rgba);
        }
        assert_ne!(frames[0], frames[1]);
        assert_eq!(frames[0], frames[2]);
        playback.shutdown().unwrap();
        let disabled = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                visual_effects: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(disabled.report().image_motions.is_empty());
        disabled.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn planar_water_ripple_keeps_unsupported_and_malformed_profiles_explicit() {
    for (constants, combos) in [
        (json!({}), json!({"PERSPECTIVE":1})),
        (json!({}), json!({"SPECULAR":1})),
        (json!({}), json!({"UNKNOWN":0})),
        (json!({"animationspeed":"NaN"}), json!({})),
        (json!({"ratio":-1}), json!({})),
        (json!({"scale":11}), json!({})),
        (json!({"ripplestrength":1.1}), json!({})),
        (json!({"point0":"0 0"}), json!({})),
    ] {
        let package = motion_package("waterripple", constants, combos, None, None, false);
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        assert!(ImagePlayback::open_scene_images(&source).is_err());
        let playback = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                allow_unsupported_features: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(playback.report().object_ids, [1]);
        assert!(playback.report().image_motions.is_empty());
        assert!(
            playback
                .report()
                .omissions
                .iter()
                .any(|v| v.contains("waterripple") && v.contains("omitted"))
        );
        playback.shutdown().unwrap();
    }
    let mut image = object(1);
    image["effects"] = json!([{"file":"effects/waterripple/effect.json", "passes":[{
        "textures":[null,null,"missing-normal"]}]}]);
    let texture = texture_checks::tex(0, [1, 1], &[80, 40, 20, 255], false, false);
    let package = snapshot_package(json!([image]), model(), material(false), &texture);
    for (name, data) in [
        (
            "effects/waterripple/effect.json",
            json!({"version":1,"replacementkey":"waterripple","passes":[{"material":"materials/ripple.json"}]}),
        ),
        (
            "materials/ripple.json",
            json!({"passes":[{"shader":"effects/waterripple"}]}),
        ),
    ] {
        let path = package.root.join("engine").join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
    }
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    assert!(ImagePlayback::open_scene_images(&source).is_err());
    let playback = ImagePlayback::open_scene_images_with_settings(
        &source,
        PlaybackSettings {
            allow_unsupported_features: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(playback.report().object_ids, [1]);
    assert!(playback.report().image_motions.is_empty());
    assert!(
        playback
            .report()
            .omissions
            .iter()
            .any(|v| v.contains("waterripple/effect.json is omitted")
                && v.contains("missing-normal"))
    );
    assert_eq!(playback_sample(&playback).composition.layers().len(), 1);
    playback.shutdown().unwrap();
}

#[test]
fn foliage_uv_masks_animate_rewind_and_preserve_hdr_and_consumer_policy() {
    let (renderer, events) = Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
    for hdr in [false, true] {
        let package = motion_package(
            "foliagesway",
            json!({"strength":1,"speeduv":5,"phase":0.5}),
            json!({"MODE":0}),
            Some(64),
            None,
            hdr,
        );
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
        assert_eq!(
            playback.report().image_motions.get(&1).unwrap(),
            "foliagesway"
        );
        let mut frames = Vec::new();
        for millis in [0, 1500, 0] {
            playback.seek(Duration::from_millis(millis));
            let id = renderer
                .render_images(playback_sample(&playback).composition)
                .unwrap();
            frames.push(wait(&renderer, &events, id).readback().unwrap().rgba);
        }
        assert_eq!(frames[0], frames[2]);
        assert_ne!(frames[0], frames[1]);
        for y in 0..8 {
            for x in 0..3 {
                assert_eq!(
                    &frames[0][(y * 8 + x) * 4..][..4],
                    &frames[1][(y * 8 + x) * 4..][..4]
                );
            }
        }
        playback.shutdown().unwrap();
        let disabled = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                visual_effects: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(disabled.report().image_motions.is_empty());
        disabled.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn foliage_uv_rejects_vertex_legacy_and_malformed_profiles() {
    for (constants, combos) in [
        (json!({}), json!({"MODE":1})),
        (json!({"ratio":0}), json!({})),
        (json!({"scale":"NaN"}), json!({})),
        (json!({"power":0}), json!({})),
        (json!({"strengthuv":0.4}), json!({})),
        (json!({"friction":"1 1"}), json!({})),
        (json!({}), json!({"UNKNOWN":0})),
    ] {
        let package = motion_package("foliagesway", constants, combos, Some(64), None, false);
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        assert!(ImagePlayback::open_scene_images(&source).is_err());
        let tolerant = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                allow_unsupported_features: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(tolerant.report().object_ids, [1]);
        assert!(tolerant.report().image_motions.is_empty());
        assert!(
            tolerant
                .report()
                .omissions
                .iter()
                .any(|v| v.contains("foliagesway") && v.contains("omitted"))
        );
        tolerant.shutdown().unwrap();
    }
}

#[test]
fn shake_direction_phase_strength_and_opacity_match_independent_pixels() {
    for (phase, opacity, strength) in [
        (None, None, 0.5),
        (None, None, 0.25),
        (Some(64), None, 0.5),
        (Some(128), None, 0.5),
        (Some(255), None, 0.5),
        (None, Some(0), 0.5),
        (None, Some(128), 0.5),
    ] {
        let package = motion_package(
            "shake",
            json!({"speed":1,"strength":strength}),
            json!({}),
            phase,
            opacity,
            false,
        );
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
        let (renderer, events) =
            Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
        playback.seek(Duration::from_secs(1));
        let id = renderer
            .render_images(playback_sample(&playback).composition)
            .unwrap();
        let pixels = wait(&renderer, &events, id).readback().unwrap().rgba;
        let phase_offset = f64::from(phase.unwrap_or(0)) / 255. * std::f64::consts::TAU;
        let x = (5. + 8. * strength * strength * (1. + phase_offset).sin()).clamp(0., 7.);
        let left = x.floor() as usize;
        let fraction = x - x.floor();
        let blue = |x: usize| if x % 2 == 1 { 200. } else { 0. };
        let displaced = [
            x * 31.,
            31.,
            blue(left) * (1. - fraction) + blue((left + 1).min(7)) * fraction,
            255.,
        ];
        let amount = f64::from(opacity.unwrap_or(255)) / 255.;
        for (channel, original) in [155., 31., 200., 255.].into_iter().enumerate() {
            let expected = (original * (1. - amount) + displaced[channel] * amount).round() as i32;
            assert!(
                (i32::from(pixels[(8 + 5) * 4 + channel]) - expected).abs() <= 1,
                "phase={phase:?} opacity={opacity:?} strength={strength} channel={channel} expected={expected}"
            );
        }
        // Painted neutral is byte 127 on both axes, independent of phase.
        assert_eq!(&pixels[(8 + 1) * 4..(8 + 1) * 4 + 4], &[31, 31, 200, 255]);
        renderer.shutdown().unwrap();
        playback.shutdown().unwrap();
    }
}

#[test]
fn shake_preserves_hdr_scene_rendering() {
    let package = motion_package(
        "shake",
        json!({"speed":1,"strength":0.5}),
        json!({}),
        None,
        None,
        true,
    );
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
    assert_eq!(playback.report().image_motions.get(&1).unwrap(), "shake");
    let (renderer, events) = Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
    let mut frames = Vec::new();
    for time in [0, 1, 0] {
        playback.seek(Duration::from_secs(time));
        let id = renderer
            .render_images(playback_sample(&playback).composition)
            .unwrap();
        frames.push(wait(&renderer, &events, id).readback().unwrap().rgba);
    }
    assert_ne!(frames[0], frames[1]);
    assert_eq!(frames[0], frames[2]);
    renderer.shutdown().unwrap();
    playback.shutdown().unwrap();
}

#[test]
fn supported_motion_profiles_keep_both_passes_in_authored_order() {
    for (new_kind, old_kind) in [
        ("spin", "waterflow"),
        ("shake", "waterflow"),
        ("shake", "spin"),
        ("shake", "waterwaves"),
        ("foliagesway", "waterflow"),
        ("foliagesway", "spin"),
        ("foliagesway", "waterwaves"),
        ("foliagesway", "shake"),
        ("waterripple", "waterflow"),
        ("waterripple", "spin"),
        ("waterripple", "waterwaves"),
        ("waterripple", "shake"),
        ("waterripple", "foliagesway"),
    ] {
        let new_textures = if matches!(new_kind, "foliagesway" | "waterripple") {
            json!([null, "image", "image"])
        } else {
            json!([null, "image"])
        };
        let new = json!({"file":format!("effects/{new_kind}/effect.json"),"passes":[{"textures":new_textures}]});
        let old_textures = if matches!(old_kind, "waterflow" | "foliagesway") {
            json!([null, "image", "image"])
        } else {
            json!([null, "image"])
        };
        let old = json!({"file":format!("effects/{old_kind}/effect.json"),"passes":[{"textures":old_textures}]});
        for effects in [json!([new, old]), json!([old, new])] {
            let mut image = object(1);
            image["effects"] = effects;
            let texture = texture_checks::tex(0, [1, 1], &[127, 127, 127, 255], false, false);
            let package =
                snapshot_package(json!([image.clone()]), model(), material(false), &texture);
            for kind in [new_kind, old_kind] {
                let directory = package.root.join("engine/effects").join(kind);
                fs::create_dir_all(&directory).unwrap();
                fs::create_dir_all(package.root.join("engine/materials")).unwrap();
                fs::write(directory.join("effect.json"),serde_json::to_vec(&json!({"version":1,"replacementkey":kind,"passes":[{"material":format!("materials/test-{kind}.json")}]})).unwrap()).unwrap();
                fs::write(
                    package
                        .root
                        .join(format!("engine/materials/test-{kind}.json")),
                    serde_json::to_vec(&json!({"passes":[{"shader":format!("effects/{kind}")}]}))
                        .unwrap(),
                )
                .unwrap();
            }
            let source = SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine"),
            };
            for tolerant in [false, true] {
                let playback = ImagePlayback::open_scene_images_with_settings(
                    &source,
                    PlaybackSettings {
                        allow_unsupported_features: tolerant,
                        ..Default::default()
                    },
                )
                .unwrap();
                let expected: Vec<_> = image["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|effect| {
                        effect["file"]
                            .as_str()
                            .unwrap()
                            .split('/')
                            .nth(1)
                            .unwrap()
                            .to_owned()
                    })
                    .collect();
                assert_eq!(playback.report().effect_chains[&1], expected);
                assert!(
                    !playback
                        .report()
                        .omissions
                        .iter()
                        .any(|s| s.contains("additional effect"))
                );
                assert_eq!(playback_sample(&playback).composition.layers().len(), 1);
                playback.shutdown().unwrap();
            }
        }
    }
}

#[test]
fn image_chains_apply_every_pass_before_appearance_and_release_scratch() {
    let pixels: Vec<_> = (0..64)
        .flat_map(|i| {
            [
                (i % 8 * 29) as u8,
                (i / 8 * 27) as u8,
                (i * 3) as u8,
                (i % 8 * 31) as u8,
            ]
        })
        .collect();
    let spin = |center: [f32; 2]| json!({"file":"effects/spin/effect.json","passes":[{"constantshadervalues":{"speed":std::f32::consts::TAU,"center":format!("{} {}",center[0],center[1])}}]});
    let (renderer, events) = Renderer::new(Size::new(8, 8).unwrap(), DeviceMode::Software).unwrap();
    for (count, hdr) in [(2, false), (3, false), (16, false), (2, true)] {
        let centers: Vec<_> = (0..count)
            .map(|i| if i % 2 == 0 { [0.5, 0.5] } else { [0.25, 0.5] })
            .collect();
        let effects: Vec<_> = centers.iter().map(|center| if hdr { json!({"file":"effects/shake/effect.json","passes":[{"textures":[null,"mask"],"constantshadervalues":{"strength":0}}]}) } else { spin(*center) }).collect();
        let scene = serde_json::to_vec(&json!({"version":5,"general":{"hdr":hdr,"bloom":false,"orthogonalprojection":{"width":8,"height":8}},"objects":[{
            "id":1,"image":"models/image.json","origin":"4 4 0","size":"8 8","alpha":0.5,"brightness":1.5,"effects":effects
        }]})).unwrap();
        let package = Package::new(&[
            ("scene.json", &scene),
            ("models/image.json", &serde_json::to_vec(&model()).unwrap()),
            ("materials/image.json", &serde_json::to_vec(&material(false)).unwrap()),
            ("materials/image.tex", &texture_checks::tex(0, [8,8], &pixels, false, false)),
            ("effects/spin/effect.json", br#"{"version":1,"replacementkey":"spin","passes":[{"material":"materials/spin.json"}]}"#),
            ("materials/spin.json", br#"{"passes":[{"shader":"effects/spin"}]}"#),
            ("effects/shake/effect.json", br#"{"version":1,"replacementkey":"shake","passes":[{"material":"materials/shake.json"}]}"#),
            ("materials/shake.json", br#"{"passes":[{"shader":"effects/shake"}]}"#),
            ("materials/mask.tex", &texture_checks::tex(0, [1,1], &[127,127,0,255], false, false)),
        ]);
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        let mut playback = ImagePlayback::open_scene_images(&source).unwrap();
        assert_eq!(playback.report().effect_chains[&1].len(), count);
        let mut expected = pixels.clone();
        // A quarter turn maps exact pixel centres to integer texels. The
        // off-centre pass clamps two rows/columns, making order observable.
        for center in centers.iter().filter(|_| !hdr) {
            let previous = expected.clone();
            for y in 0_i32..8 {
                for x in 0_i32..8 {
                    let (sx, sy) = if center[0] == 0.5 {
                        (y, 7 - x)
                    } else {
                        (y - 2, 5 - x)
                    };
                    let offset = ((sy.clamp(0, 7) * 8 + sx.clamp(0, 7)) * 4) as usize;
                    let out = ((y * 8 + x) * 4) as usize;
                    expected[out..out + 4].copy_from_slice(&previous[offset..offset + 4]);
                }
            }
        }
        playback.seek(Duration::from_millis(250));
        let sample = playback_sample(&playback);
        let id = renderer.render_images(sample.composition.clone()).unwrap();
        let frame = wait(&renderer, &events, id);
        let rgba = frame.readback().unwrap().rgba;
        for (pixel, source) in rgba.chunks_exact(4).zip(expected.chunks_exact(4)) {
            let alpha = f32::from(source[3]) / 255. * 0.5;
            for c in 0..3 {
                let bright = f32::from(source[c]) * 1.5;
                let value = (if hdr { bright } else { bright.min(255.) }) * alpha
                    + f32::from(wallpaper_kit::BLANK_RGBA[c]) * (1. - alpha);
                assert!(
                    (f32::from(pixel[c]) - value).abs() <= 1.1,
                    "{count} passes: {pixel:?}, expected {value}"
                );
            }
            assert_eq!(pixel[3], 255);
        }
        assert_eq!(
            stats(&renderer, frame.sequence()).effect_texture_bytes,
            if hdr { 8 * 8 * 8 * 3 } else { 8 * 8 * 4 * 2 }
        );
        // A retained frame must survive switching to content without effects.
        let retained = frame.readback().unwrap().rgba;
        let id = renderer
            .render_texture_region(sample.composition.layers()[0].region().clone())
            .unwrap();
        let next = wait(&renderer, &events, id);
        assert_eq!(stats(&renderer, next.sequence()).effect_texture_bytes, 0);
        assert_eq!(frame.readback().unwrap().rgba, retained);
        drop(next);
        playback.seek(Duration::ZERO);
        let zero = playback_sample(&playback);
        let id = renderer.render_images(zero.composition).unwrap();
        let zero_pixels = wait(&renderer, &events, id).readback().unwrap().rgba;
        if !hdr {
            assert_ne!(rgba, zero_pixels);
        }
        playback.seek(Duration::from_millis(250));
        let id = renderer
            .render_images(playback_sample(&playback).composition)
            .unwrap();
        assert_eq!(wait(&renderer, &events, id).readback().unwrap().rgba, rgba);
        playback.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn image_chain_limit_is_explicit_in_strict_and_tolerant_playback() {
    let mut image = object(1);
    image["effects"] = json!(
        (0..17)
            .map(|_| json!({"file":"effects/spin/effect.json","passes":[{}]}))
            .collect::<Vec<_>>()
    );
    let texture = texture_checks::tex(0, [1, 1], &[64, 128, 192, 255], false, false);
    let package = snapshot_package(json!([image]), model(), material(false), &texture);
    let effect_dir = package.root.join("engine/effects/spin");
    fs::create_dir_all(&effect_dir).unwrap();
    fs::create_dir_all(package.root.join("engine/materials")).unwrap();
    fs::write(
        effect_dir.join("effect.json"),
        br#"{"version":1,"replacementkey":"spin","passes":[{"material":"materials/spin.json"}]}"#,
    )
    .unwrap();
    fs::write(
        package.root.join("engine/materials/spin.json"),
        br#"{"passes":[{"shader":"effects/spin"}]}"#,
    )
    .unwrap();
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    let error = ImagePlayback::open_scene_images(&source).err().unwrap();
    assert!(error.to_string().contains("exceeds 16 passes"));
    let playback = ImagePlayback::open_scene_images_with_settings(
        &source,
        PlaybackSettings {
            allow_unsupported_features: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(playback.report().effect_chains[&1].len(), 16);
    assert!(
        playback
            .report()
            .omissions
            .iter()
            .any(|v| v.contains("16-pass image limit"))
    );
    playback.shutdown().unwrap();
}

fn background_package(general: Value, texture: &[u8]) -> Package {
    let mut general = general;
    general["orthogonalprojection"] = json!({"width":6,"height":1});
    let scene = serde_json::to_vec(&json!({"version":5,"general":general,"objects":[{
        "id":1,"image":"models/image.json","origin":"2 0.5 0","size":"4 1"
    }]}))
    .unwrap();
    let model = serde_json::to_vec(&model()).unwrap();
    let material = serde_json::to_vec(&material(false)).unwrap();
    Package::new(&[
        ("scene.json", &scene),
        ("models/image.json", &model),
        ("materials/image.json", &material),
        ("materials/image.tex", texture),
    ])
}

#[test]
fn scene_background_survives_transparency_resize_and_retained_samples() {
    let texture = texture_checks::tex(
        0,
        [4, 1],
        &[
            255, 255, 255, 0, 0, 0, 0, 128, 255, 0, 0, 255, 255, 255, 255, 128,
        ],
        false,
        false,
    );
    let expected = [
        [51, 102, 153, 255],
        [25, 51, 76, 255],
        [255, 0, 0, 255],
        [153, 179, 204, 255],
        [51, 102, 153, 255],
        [51, 102, 153, 255],
    ];
    for hdr in [false, true] {
        let package = background_package(
            json!({"clearcolor":"0.2 0.4 0.6","clearenabled":true,"hdr":hdr,"bloom":false}),
            &texture,
        );
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        let playback = ImagePlayback::open_scene_images(&source).unwrap();
        assert_eq!(playback.report().background_color, [0.2, 0.4, 0.6, 1.]);
        let sample = playback_sample(&playback);
        assert_eq!(sample.composition.background_color(), [0.2, 0.4, 0.6, 1.]);
        playback.shutdown().unwrap();
        let (renderer, events) =
            Renderer::new(Size::new(6, 1).unwrap(), DeviceMode::Software).unwrap();
        let mut id = renderer.render_images(sample.composition).unwrap();
        for width in [6, 12] {
            if width == 12 {
                id = renderer.resize(Size::new(12, 2).unwrap()).unwrap();
            }
            let pixels = wait(&renderer, &events, id).readback().unwrap();
            for (index, pixel) in pixels.rgba.chunks_exact(4).enumerate() {
                let target = expected[index % width / (width / 6)];
                assert!(
                    pixel.iter().zip(target).all(|(a, b)| a.abs_diff(b) <= 1),
                    "hdr={hdr}: {pixel:?} != {target:?}"
                );
            }
        }
        renderer.shutdown().unwrap();
        let selected = ImagePlayback::open(&source, &[1]).unwrap();
        assert_eq!(
            playback_sample(&selected).composition.background_color(),
            wallpaper_kit::BLANK_RGBA.map(|v| f32::from(v) / 255.)
        );
        selected.shutdown().unwrap();
    }
}

#[test]
fn scene_background_rejects_bad_values_before_image_decode_and_reports_disabled_clearing() {
    let corrupt = texture_checks::tex(0, [2, 2], b"invalid PNG", false, true);
    for general in [
        json!({"clearcolor":"0 NaN 1"}),
        json!({"clearcolor":"0.2 0.4"}),
        json!({"clearcolor":"0 2 1"}),
        json!({"clearcolor":[0,0,0]}),
        json!({"clearenabled":"true"}),
    ] {
        let package = background_package(general, &corrupt);
        let error = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .err()
        .unwrap()
        .to_string();
        assert!(error.contains("scene.json general.clear"), "{error}");
        assert!(!error.contains("PNG"), "{error}");
    }
    let texture = texture_checks::tex(0, [4, 1], &[255; 16], false, false);
    let package = background_package(json!({"clearcolor":"1 0 0","clearenabled":false}), &texture);
    let playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    assert!(
        playback
            .report()
            .omissions
            .iter()
            .any(|m| m.contains("Disabled scene clearing"))
    );
    assert_eq!(
        playback_sample(&playback).composition.background_color(),
        wallpaper_kit::BLANK_RGBA.map(|v| f32::from(v) / 255.)
    );
    playback.shutdown().unwrap();
}

#[test]
fn tolerant_playback_draws_base_images_and_reports_optional_failures() {
    let tex = texture_checks::tex(0, [2, 2], &[255; 16], false, false);
    let mut image = object(1);
    image["origin"] = json!("8 6 0");
    image["size"] = json!("16 12");
    image["animationlayers"] = json!([{"animation":"unimplemented"}]);
    image["alpha"] = json!({"script":"unimplemented", "value":2.0});
    image["effects"] = json!([{"file":"effects/cloudmotion/effect.json"}]);
    let mut puppet = model();
    puppet["puppet"] = json!("models/unimplemented.mdl");
    let mut custom = material(false);
    custom["passes"][0]["shader"] = json!("unimplemented");
    let package = snapshot_package(
        json!([image,{"id":2,"particle":"missing-particle.json"},{"id":3,"text":"hello","pointsize":-1}]),
        puppet,
        custom,
        &tex,
    );
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    assert!(ImagePlayback::open_scene_images(&source).is_err());
    let playback = ImagePlayback::open_scene_images_with_settings(
        &source,
        PlaybackSettings {
            allow_unsupported_features: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(playback.report().object_ids, [1]);
    for feature in [
        "animationlayers",
        "authored default",
        "clamped",
        "puppet",
        "custom shader",
        "cloudmotion",
        "Object 2",
        "Object 3",
    ] {
        assert!(
            playback
                .report()
                .omissions
                .iter()
                .any(|line| line.contains(feature)),
            "missing {feature}: {:?}",
            playback.report().omissions
        );
    }
    let sample = playback_sample(&playback);
    let (renderer, events) = Renderer::new(Size::new(2, 2).unwrap(), DeviceMode::Software).unwrap();
    let id = renderer.render_images(sample.composition).unwrap();
    let frame = wait(&renderer, &events, id);
    assert_eq!(frame.readback().unwrap().rgba, [255; 16]);
    renderer.shutdown().unwrap();
    playback.shutdown().unwrap();
}

#[test]
fn tolerant_playback_still_rejects_corrupt_source_pixels_and_unsafe_references() {
    for (texture, material) in [
        (
            texture_checks::tex(0, [2, 2], b"invalid PNG", false, true),
            material(false),
        ),
        (
            texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false),
            {
                let mut material = material(false);
                material["passes"][0]["textures"][0] = json!("../../outside");
                material
            },
        ),
    ] {
        let package = snapshot_package(json!([object(1)]), model(), material, &texture);
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        assert!(
            ImagePlayback::open_scene_images_with_settings(
                &source,
                PlaybackSettings {
                    allow_unsupported_features: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}

#[test]
fn large_logical_canvas_preserves_image_coverage_without_larger_gpu_outputs() {
    let tex = texture_checks::tex(0, [2, 2], &texture_checks::rgba(), false, false);
    let definition = json!({"version":5,"general":{"orthogonalprojection":{"width":8192,"height":8192}},"objects":[{
        "id":1,"image":"models/image.json","origin":"4096 4096 0","size":"8192 8192"
    }]});
    let package = Package::new(&[
        ("scene.json", &serde_json::to_vec(&definition).unwrap()),
        ("models/image.json", &serde_json::to_vec(&model()).unwrap()),
        (
            "materials/image.json",
            &serde_json::to_vec(&material(false)).unwrap(),
        ),
        ("materials/image.tex", &tex),
    ]);
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    assert!(ImagePlayback::open_scene_images(&source).is_err());
    let playback = ImagePlayback::open_scene_images_with_settings(
        &source,
        PlaybackSettings {
            allow_unsupported_features: true,
            ..Default::default()
        },
    )
    .unwrap();
    let sample = playback_sample(&playback);
    assert_eq!(sample.composition.canvas(), Size::new(4096, 4096).unwrap());
    assert_eq!(sample.composition.layers()[0].center(), [2048., 2048.]);
    assert_eq!(sample.composition.layers()[0].size(), [4096., 4096.]);
    let (renderer, events) = Renderer::new(Size::new(2, 2).unwrap(), DeviceMode::Software).unwrap();
    let id = renderer.render_images(sample.composition).unwrap();
    assert_eq!(
        wait(&renderer, &events, id).readback().unwrap().rgba,
        [
            255, 0, 0, 255, 8, 140, 16, 255, 16, 24, 32, 255, 240, 160, 80, 255
        ]
    );
    renderer.shutdown().unwrap();
    playback.shutdown().unwrap();
}
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
    let white = texture_checks::tex(0, [1, 1], &[255, 255, 255, 255], false, false);
    particle_package_with_texture(root, child, visible, &white)
}

fn particle_package_with_texture(
    root: Value,
    child: Option<Value>,
    visible: Value,
    white: &[u8],
) -> Package {
    let scene=serde_json::to_vec(&json!({"version":1,"general":{"orthogonalprojection":{"width":64,"height":64}},"objects":[
        {"id":1,"image":"models/base.json","origin":"32 32 0","size":"64 64","color":"1 1 1","alignment":"center","perspective":false},
        {"id":2,"particle":"particles/root.json","origin":"32 32 0","visible":visible}
    ]})).unwrap();
    let root = serde_json::to_vec(&root).unwrap();
    let child = serde_json::to_vec(&child).unwrap();
    let dark = texture_checks::tex(0, [1, 1], &[32, 32, 32, 255], false, false);
    let mut package_bytes=texture_checks::package(&[
        ("scene.json",&scene),("particles/root.json",&root),("particles/child.json",&child),
        ("models/base.json",br#"{"material":"materials/base.json"}"#),
        ("materials/base.json",br#"{"passes":[{"shader":"genericimage2","blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","combos":{"VERSION":2},"textures":["dark"]}]}"#),
        ("materials/star.json",br#"{"passes":[{"shader":"genericparticle","blending":"additive","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","textures":["white"]}]}"#),
        ("materials/dark.tex",&dark),("materials/white.tex",white)
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
fn particle_vector_rotation_uses_radians_and_accepts_null_defaults() {
    for (rotation, expected) in [(json!(90), 90.), (json!("0 0 1.5707963267948966"), 270.)] {
        let mut root = burst();
        root["sequencemultiplier"] = Value::Null;
        root["initializer"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"rotationrandom","min":rotation,"max":rotation,"exponent":null}));
        let package = particle_package(root, None, json!(true));
        let mut playback = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .unwrap();
        assert_eq!(playback.report().particle_objects, [2]);
        for time in [0, 500, 0] {
            playback.seek(Duration::from_millis(time));
            let sample = playback.try_sample().unwrap().unwrap();
            let layer = &sample.composition.layers()[1];
            assert!((layer.angle_degrees() - expected).abs() < 0.001);
        }
        playback.shutdown().unwrap();
    }
}

#[test]
fn particle_box_directions_scale_birth_positions_and_rewind() {
    let mut root = burst();
    root["emitter"] = json!([{"name":"boxrandom","instantaneous":2,
        "distancemin":"10 20 0","distancemax":"10 20 0","directions":"2 0.25 0"}]);
    let package = particle_package(root, None, json!(true));
    let mut playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    assert_eq!(playback.report().particle_objects, [2]);
    for time in [0, 500, 0] {
        playback.seek(Duration::from_millis(time));
        let sample = playback.try_sample().unwrap().unwrap();
        assert_eq!(sample.composition.layers().len(), 3);
        for layer in &sample.composition.layers()[1..] {
            let center = layer.center();
            assert!(((center[0] - 32.).abs() - 20.).abs() < 0.001);
            assert!(((center[1] - 32.).abs() - 5.).abs() < 0.001);
        }
    }
    playback.shutdown().unwrap();
}

#[test]
fn particle_sphere_sign_and_missing_rotation_minimum_preserve_authored_ranges() {
    let mut root = burst();
    root["maxcount"] = json!(80);
    root["emitter"] = json!([{"name":"sphererandom","instantaneous":80,
        "distancemin":8,"distancemax":16,"directions":"1 1 0","sign":"-1 1 0"}]);
    root["initializer"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"rotationrandom","max":"0 0 1.5707963267948966"}));
    let package = particle_package(root, None, json!(true));
    let playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    let sample = playback.try_sample().unwrap().unwrap();
    assert_eq!(sample.composition.layers().len(), 81);
    let mut angles = Vec::new();
    for layer in &sample.composition.layers()[1..] {
        let [x, y] = layer.center();
        assert!(x <= 32. && y >= 32.);
        assert!((8. ..=16.01).contains(&(x - 32.).hypot(y - 32.)));
        assert!((270. ..=360.).contains(&layer.angle_degrees()));
        angles.push(layer.angle_degrees());
    }
    assert!(angles.iter().any(|v| *v < 300.) && angles.iter().any(|v| *v > 330.));
    playback.shutdown().unwrap();
}

#[test]
fn particle_vector_rotation_draws_the_engine_observed_quarter_turn() {
    let pixels: Vec<_> = (0..8)
        .flat_map(|y| {
            (0..8).flat_map(move |x| [255, 255, 255, if x < 2 || y < 2 { 255 } else { 0 }])
        })
        .collect();
    let texture = texture_checks::tex(0, [8, 8], &pixels, false, false);
    let (renderer, events) =
        Renderer::new(Size::new(64, 64).unwrap(), DeviceMode::Software).unwrap();
    for (rotation, lit, dark) in [
        ("0 0 0", [(20, 32), (32, 20)], (44, 32)),
        ("0 0 1.5707963267948966", [(44, 32), (32, 20)], (20, 32)),
    ] {
        let mut root = burst();
        root["emitter"][0]["instantaneous"] = json!(1);
        root["initializer"] = json!([
            {"name":"lifetimerandom","min":2,"max":2},
            {"name":"sizerandom","min":64,"max":64},
            {"name":"rotationrandom","min":rotation,"max":rotation}]);
        let package = particle_package_with_texture(root, None, json!(true), &texture);
        let playback = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .unwrap();
        let sample = playback.try_sample().unwrap().unwrap();
        let id = renderer.render_images(sample.composition).unwrap();
        let capture = wait(&renderer, &events, id).readback().unwrap();
        for (x, y) in lit {
            assert_eq!(capture.rgba[(y * 64 + x) * 4], 255);
        }
        assert_eq!(capture.rgba[(dark.1 * 64 + dark.0) * 4], 32);
        playback.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn particle_two_channel_textures_use_grayscale_and_alpha() {
    let (renderer, events) =
        Renderer::new(Size::new(64, 64).unwrap(), DeviceMode::Software).unwrap();
    for (channels, expected) in [([255, 128], 160), ([64, 255], 96), ([255, 0], 32)] {
        let texture = texture_checks::tex(8, [1, 1], &channels, false, false);
        let mut root = burst();
        root["emitter"][0]["instantaneous"] = json!(1);
        root["initializer"] = json!([
            {"name":"lifetimerandom","min":2,"max":2},
            {"name":"sizerandom","min":64,"max":64}]);
        let package = particle_package_with_texture(root, None, json!(true), &texture);
        let playback = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .unwrap();
        let sample = playback.try_sample().unwrap().unwrap();
        let id = renderer.render_images(sample.composition).unwrap();
        let capture = wait(&renderer, &events, id).readback().unwrap();
        let pixel = &capture.rgba[(32 * 64 + 32) * 4..][..4];
        assert_eq!(pixel, &[expected, expected, expected, 255]);
        playback.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn particle_alpha_change_scales_initial_alpha_and_samples_smoothly() {
    let mut root = burst();
    root["emitter"][0]["instantaneous"] = json!(1);
    root["initializer"][0]["min"] = json!(4);
    root["initializer"][0]["max"] = json!(4);
    root["operator"] = json!([{"name":"alphachange","starttime":0.25,"endtime":0.75,"startvalue":0.25,"endvalue":0.75}]);
    let package = particle_package(root, None, json!(true));
    let mut playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    assert_eq!(playback.report().particle_objects, [2]);
    for (time, expected) in [
        (0., 0.125),
        (1., 0.125),
        (2., 0.25),
        (3.5, 0.375),
        (2., 0.25),
    ] {
        playback.seek(Duration::from_secs_f64(time));
        let sample = playback.try_sample().unwrap().unwrap();
        assert!((sample.composition.layers()[1].opacity() - expected).abs() < 0.00001);
    }
    let mut previous = 0.125;
    for frame in 0..=240 {
        playback.seek(Duration::from_secs_f64(1. + f64::from(frame) / 120.));
        let sample = playback.try_sample().unwrap().unwrap();
        let opacity = sample.composition.layers()[1].opacity();
        assert!((opacity - previous).abs() <= 0.00105);
        previous = opacity;
    }
    playback.shutdown().unwrap();
}

#[test]
fn particle_alpha_change_rejects_invalid_ranges_and_reports_extra_fields() {
    for change in [
        json!({"name":"alphachange","starttime":0.5,"endtime":0.5}),
        json!({"name":"alphachange","endvalue":1.1}),
        json!({"name":"alphachange","startvalue":"NaN"}),
    ] {
        let mut root = burst();
        root["operator"] = json!([change]);
        let package = particle_package(root, None, json!(true));
        assert!(
            ImagePlayback::open_scene_images(&SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine"),
            })
            .is_err()
        );
    }
    let mut root = burst();
    root["operator"] = json!([{"name":"alphachange","audiomode":1}]);
    let package = particle_package(root, None, json!(true));
    let playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    assert!(playback.report().particle_objects.is_empty());
    assert!(
        playback
            .report()
            .omissions
            .iter()
            .any(|v| v.contains("operator component"))
    );
    playback.shutdown().unwrap();
}

#[test]
fn tolerant_particles_keep_supported_operators_when_an_operator_is_missing() {
    for tolerant in [false, true] {
        let mut root = burst();
        root["emitter"][0]["instantaneous"] = json!(1);
        root["initializer"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"velocityrandom","min":"8 0 0","max":"8 0 0"}));
        root["operator"] = json!([
            {"name":"movement","gravity":"0 0 0"},
            {"name":"oscillateposition","mask":"1 0 0","frequencymin":1},
            {"name":"sizechange","startvalue":0.5,"endvalue":0.5},
            {"name":"alphachange","startvalue":0.5,"endvalue":0.5}]);
        let package = particle_package(root, None, json!(true));
        let mut playback = ImagePlayback::open_scene_images_with_settings(
            &SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine"),
            },
            PlaybackSettings {
                allow_unsupported_features: tolerant,
                ..PlaybackSettings::default()
            },
        )
        .unwrap();
        playback.seek(Duration::from_millis(500));
        let sample = playback.try_sample().unwrap().unwrap();
        assert_eq!(
            sample.composition.layers().len(),
            if tolerant { 2 } else { 1 }
        );
        if tolerant {
            assert_eq!(playback.report().particle_objects, [2]);
            assert!(
                playback
                    .report()
                    .omissions
                    .iter()
                    .any(|v| v.contains("operator component oscillateposition is omitted"))
            );
            let particle = &sample.composition.layers()[1];
            assert_eq!(particle.center(), [36., 32.]);
            assert_eq!(particle.size(), [2., 2.]);
            assert_eq!(particle.opacity(), 0.25);
            playback.seek(Duration::from_secs(1));
            assert_eq!(
                playback.try_sample().unwrap().unwrap().composition.layers()[1].center(),
                [40., 32.]
            );
            playback.seek(Duration::from_millis(500));
            assert_eq!(
                playback.try_sample().unwrap().unwrap().composition.layers()[1].center(),
                particle.center()
            );
        } else {
            assert!(playback.report().particle_objects.is_empty());
        }
        playback.shutdown().unwrap();
    }
}

#[test]
fn tolerant_turbulence_integrates_masked_force_and_rewinds_gpu_pixels() {
    let (renderer, events) =
        Renderer::new(Size::new(64, 64).unwrap(), DeviceMode::Software).unwrap();
    for mask in ["1 0 0", "0 1 0", "0.5 0 0", "-1 0 0", "0 0 0"] {
        let mut root = burst();
        root["emitter"][0]["instantaneous"] = json!(1);
        root["initializer"] = json!([
            {"name":"lifetimerandom","min":4,"max":4},
            {"name":"sizerandom","min":8,"max":8}]);
        root["operator"] = json!([
            {"name":"movement","gravity":"0 0 0"},
            {"name":"turbulence","mask":mask,"scale":0,"timescale":0,
                "speedmin":8,"speedmax":8,"phasemin":std::f32::consts::FRAC_PI_2,
                "phasemax":std::f32::consts::FRAC_PI_2}]);
        let package = particle_package(root, None, json!(true));
        let source = SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        };
        // Approximate noise is admitted only by the explicit tolerant policy.
        let strict = ImagePlayback::open_scene_images(&source).unwrap();
        assert!(strict.report().particle_objects.is_empty());
        strict.shutdown().unwrap();
        let mut playback = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                allow_unsupported_features: true,
                ..PlaybackSettings::default()
            },
        )
        .unwrap();
        assert_eq!(playback.report().particle_objects, [2]);
        assert!(playback.report().omissions.iter().any(|v| {
            v.contains(
                "turbulence uses independent harmonic noise; exact engine noise is not reproduced",
            )
        }));
        let direction = match mask {
            "1 0 0" => [1., 0.],
            "0 1 0" => [0., 1.],
            "0.5 0 0" => [0.5, 0.],
            "-1 0 0" => [-1., 0.],
            _ => [0., 0.],
        };
        let mut captures = Vec::new();
        // Constant 8 units/s² force gives displacement 4*t², independently
        // of how many intervening samples the consumer requests.
        for millis in [0, 1000, 2000, 1000] {
            let time = millis as f32 / 1000.;
            playback.seek(Duration::from_millis(millis));
            let sample = playback.try_sample().unwrap().unwrap();
            let expected = direction.map(|d| 32. + 4. * time * time * d);
            let layer = &sample.composition.layers()[1];
            for (value, expected) in layer.center().into_iter().zip(expected) {
                assert!((value - expected).abs() < 0.0001);
            }
            let id = renderer.render_images(sample.composition).unwrap();
            let capture = wait(&renderer, &events, id).readback().unwrap();
            let x = expected[0] as usize;
            let y = 63 - expected[1] as usize;
            assert_eq!(&capture.rgba[(y * 64 + x) * 4..][..4], &[255; 4]);
            captures.push(capture.rgba);
        }
        assert_eq!(captures[1], captures[3]);
        if direction != [0., 0.] {
            assert_ne!(captures[0], captures[1]);
        }
        playback.shutdown().unwrap();
    }
    renderer.shutdown().unwrap();
}

#[test]
fn tolerant_turbulence_trails_follow_the_changed_velocity() {
    let mut root = burst();
    root["emitter"][0]["instantaneous"] = json!(1);
    root["initializer"][0] = json!({"name":"lifetimerandom","min":4,"max":4});
    root["renderer"] = json!([{"name":"spritetrail","length":0.25}]);
    root["operator"] = json!([
        {"name":"movement"},
        {"name":"turbulence","mask":"1 0 0","scale":0,"timescale":0,
            "speedmin":8,"speedmax":8,"phasemin":std::f32::consts::FRAC_PI_2,
            "phasemax":std::f32::consts::FRAC_PI_2}]);
    let package = particle_package(root, None, json!(true));
    let mut playback = ImagePlayback::open_scene_images_with_settings(
        &SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        },
        PlaybackSettings {
            allow_unsupported_features: true,
            ..PlaybackSettings::default()
        },
    )
    .unwrap();
    for seconds in [1, 2, 1] {
        playback.seek(Duration::from_secs(seconds));
        let sample = playback.try_sample().unwrap().unwrap();
        let trail = &sample.composition.layers()[1];
        assert!((trail.size()[1] - 8. * seconds as f32).abs() < 0.0001);
        assert!((trail.center()[0] - 32. - 4. * (seconds * seconds) as f32).abs() < 0.0001);
    }
    playback.shutdown().unwrap();
}

#[test]
fn tolerant_turbulence_is_continuous_and_preserves_unmasked_motion() {
    let mut root = burst();
    root["emitter"][0]["instantaneous"] = json!(1);
    root["initializer"] = json!([
        {"name":"lifetimerandom","min":4,"max":4},
        {"name":"sizerandom","min":8,"max":8},
        {"name":"velocityrandom","min":"0 8 0","max":"0 8 0"}]);
    root["operator"] = json!([
        {"name":"movement","gravity":"0 0 0"},
        {"name":"turbulence","mask":"1 0 0","scale":0.002,
            "phasemax":5,"speedmin":100,"speedmax":150}]);
    let package = particle_package(root, None, json!(true));
    let mut playback = ImagePlayback::open_scene_images_with_settings(
        &SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        },
        PlaybackSettings {
            allow_unsupported_features: true,
            ..PlaybackSettings::default()
        },
    )
    .unwrap();
    let mut positions = Vec::new();
    for frame in 0..=240 {
        let time = f64::from(frame) / 120.;
        playback.seek(Duration::from_secs_f64(time));
        let sample = playback.try_sample().unwrap().unwrap();
        let position = sample.composition.layers()[1].center();
        assert!((position[1] - (32. + time as f32 * 8.)).abs() < 0.0001);
        positions.push(position[0]);
    }
    assert!(positions.windows(2).all(|w| (w[1] - w[0]).abs() < 2.5));
    assert!(
        positions
            .windows(2)
            .filter(|w| (w[1] - w[0]).abs() > 0.0001)
            .count()
            > 220
    );
    assert!((positions[120] - 32.).abs() > 1.);
    playback.seek(Duration::from_secs(1));
    assert_eq!(
        playback.try_sample().unwrap().unwrap().composition.layers()[1].center()[0],
        positions[120]
    );
    playback.shutdown().unwrap();
}

#[test]
fn tolerant_turbulence_keeps_invalid_and_unimplemented_profiles_explicit() {
    for extra in [
        json!({"scale":"NaN"}),
        json!({"speedmin":-1}),
        json!({"timescale":101}),
        json!({"mask":"0 0 1"}),
        json!({"audiomode":1}),
    ] {
        let mut root = burst();
        let mut op = json!({"name":"turbulence","speedmin":1,"speedmax":1});
        op.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        root["operator"] = json!([{"name":"movement"},op]);
        let package = particle_package(root, None, json!(true));
        let playback = ImagePlayback::open_scene_images_with_settings(
            &SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine"),
            },
            PlaybackSettings {
                allow_unsupported_features: true,
                ..PlaybackSettings::default()
            },
        )
        .unwrap();
        assert!(playback.report().omissions.iter().any(|v| {
            v.contains("finite scalar")
                || v.contains("outside supported bounds")
                || v.contains("operator component turbulence is omitted")
        }));
        playback.shutdown().unwrap();
    }
    for movement in [
        json!({"name":"movement","drag":0.1}),
        json!({"name":"movement","gravity":"0 -10 0"}),
    ] {
        let mut root = burst();
        root["operator"] = json!([movement,{"name":"turbulence"}]);
        let package = particle_package(root, None, json!(true));
        let playback = ImagePlayback::open_scene_images_with_settings(
            &SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine"),
            },
            PlaybackSettings {
                allow_unsupported_features: true,
                ..PlaybackSettings::default()
            },
        )
        .unwrap();
        assert_eq!(playback.report().particle_objects, [2]);
        assert!(
            playback
                .report()
                .omissions
                .iter()
                .any(|v| v.contains("operator component turbulence is omitted"))
        );
        playback.shutdown().unwrap();
    }
}

#[test]
fn tolerant_particle_children_retain_draws_but_invalid_values_still_fail_preparation() {
    let mut root = burst();
    root["children"] = json!([{"name":"particles/child.json","type":"eventdeath"}]);
    let mut child = burst();
    child["operator"] = json!([{"name":"oscillateposition","mask":"1 0.5 0"}]);
    let package = particle_package(root, Some(child), json!(true));
    let source = SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    };
    for tolerant in [false, true] {
        let mut playback = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                allow_unsupported_features: tolerant,
                ..PlaybackSettings::default()
            },
        )
        .unwrap();
        playback.seek(Duration::from_secs(3));
        assert_eq!(
            playback
                .try_sample()
                .unwrap()
                .unwrap()
                .composition
                .layers()
                .len(),
            if tolerant { 5 } else { 1 }
        );
        playback.seek(Duration::from_secs(100));
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
    let mut root = burst();
    root["operator"] = json!([{"name":"alphachange","endvalue":2}]);
    let package = particle_package(root, None, json!(true));
    let playback = ImagePlayback::open_scene_images_with_settings(
        &SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        },
        PlaybackSettings {
            allow_unsupported_features: true,
            ..PlaybackSettings::default()
        },
    )
    .unwrap();
    assert!(playback.report().particle_objects.is_empty());
    assert!(
        playback
            .report()
            .omissions
            .iter()
            .any(|v| v.contains("particle number outside supported bounds"))
    );
    playback.shutdown().unwrap();
}

#[test]
fn turbulent_velocity_zero_scale_matches_engine_forward_and_offset() {
    for offset in [0., -0.5_f32] {
        let mut root = burst();
        root["emitter"][0]["instantaneous"] = json!(1);
        root["initializer"] = json!([
            {"name":"lifetimerandom","min":4,"max":4},
            {"name":"sizerandom","min":8,"max":8},
            {"name":"turbulentvelocityrandom","scale":0,"offset":offset,"speedmin":32,"speedmax":32}]);
        root["operator"] = json!([{"name":"movement","gravity":"0 0 0"}]);
        let package = particle_package(root, None, json!(true));
        let mut playback = ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine"),
        })
        .unwrap();
        playback.seek(Duration::from_secs(1));
        let sample = playback.try_sample().unwrap().unwrap();
        let center = sample.composition.layers()[1].center();
        assert!((center[0] - (32. - offset.sin() * 32.)).abs() < 0.001);
        assert!((center[1] - (32. + offset.cos() * 32.)).abs() < 0.001);
        playback.shutdown().unwrap();
    }
}

#[test]
fn turbulent_velocity_missing_speeds_use_the_engine_100_to_250_range() {
    let mut root = burst();
    root["maxcount"] = json!(1000);
    root["emitter"][0]["instantaneous"] = json!(1000);
    root["initializer"] = json!([
        {"name":"lifetimerandom","min":4,"max":4},
        {"name":"sizerandom","min":2,"max":2},
        {"name":"turbulentvelocityrandom","scale":0,"offset":0}]);
    root["operator"] = json!([{"name":"movement","gravity":"0 0 0"}]);
    let package = particle_package(root, None, json!(true));
    let mut playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    playback.seek(Duration::from_secs(1));
    let sample = playback.try_sample().unwrap().unwrap();
    let speeds: Vec<_> = sample.composition.layers()[1..]
        .iter()
        .map(|p| {
            assert_eq!(p.center()[0], 32.);
            p.center()[1] - 32.
        })
        .collect();
    assert_eq!(speeds.len(), 1000);
    assert!(speeds.iter().all(|s| (100. ..=250.).contains(s)));
    assert!(speeds.iter().copied().fold(0_f32, f32::max) > 249.);
    assert!(speeds.iter().copied().fold(250_f32, f32::min) < 101.);
    playback.shutdown().unwrap();
}

#[test]
fn particle_profiles_keep_invalid_values_and_3d_rotations_explicit() {
    for (field, value) in [
        ("min", json!("0 0 NaN")),
        ("min", json!("0 0")),
        ("max", json!("0 0 100")),
        ("exponent", json!(0)),
    ] {
        let mut root = burst();
        let mut rotation = json!({"name":"rotationrandom","min":"0 0 0","max":"0 0 1"});
        rotation[field] = value;
        root["initializer"].as_array_mut().unwrap().push(rotation);
        let package = particle_package(root, None, json!(true));
        assert!(
            ImagePlayback::open_scene_images(&SceneSource {
                package: package.root.join("scene.pkg"),
                engine_assets: package.root.join("engine")
            })
            .is_err()
        );
    }
    let mut root = burst();
    root["initializer"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"rotationrandom","min":"1 0 0","max":"1 0 0"}));
    let package = particle_package(root, None, json!(true));
    let playback = ImagePlayback::open_scene_images(&SceneSource {
        package: package.root.join("scene.pkg"),
        engine_assets: package.root.join("engine"),
    })
    .unwrap();
    assert!(playback.report().particle_objects.is_empty());
    assert!(
        playback
            .report()
            .omissions
            .iter()
            .any(|v| v.contains("3D rotation"))
    );
    playback.shutdown().unwrap();
    let mut root = burst();
    root["emitter"][0]["sign"] = json!("0 0 2");
    let package = particle_package(root, None, json!(true));
    assert!(
        ImagePlayback::open_scene_images(&SceneSource {
            package: package.root.join("scene.pkg"),
            engine_assets: package.root.join("engine")
        })
        .is_err()
    );
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
