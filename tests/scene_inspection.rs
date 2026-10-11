//! Public inspection contract using generated, redistributable packages.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use wallpaper_kit::{SceneSource, inspect_scene};

static RUN: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(entries: &[(&str, &[u8])]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "scene-inspection-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("engine")).unwrap();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&8_u32.to_le_bytes());
        bytes.extend_from_slice(b"PKGV0024");
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        let mut offset = 0_u32;
        for (name, data) in entries {
            bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
            offset += data.len() as u32;
        }
        for (_, data) in entries {
            bytes.extend_from_slice(data);
        }
        fs::write(root.join("scene.pkg"), bytes).unwrap();
        Self { root }
    }

    fn source(&self) -> SceneSource {
        SceneSource {
            package: self.root.join("scene.pkg"),
            engine_assets: self.root.join("engine"),
        }
    }

    fn image(material: &[u8], texture: &[u8], fragment: &[u8]) -> Self {
        Self::new(&[
            ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":7,"image":"models/sprite.json"}]}"#),
            ("models/sprite.json", br#"{"material":"materials/sprite.json"}"#),
            ("materials/sprite.json", material),
            ("materials/sprite.tex", texture),
            ("shaders/genericimage4.vert", b""),
            ("shaders/genericimage4.frag", fragment),
        ])
    }
}

impl Drop for Fixture {
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

const EMPTY: &[u8] = br#"{"version":5,"general":{"orthogonalprojection":{"width":3840,"height":2160}},"objects":[]}"#;

#[test]
fn legacy_package_preserves_index_validation() {
    for signature in ["PKGV0016", "PKGV0018", "PKGV0022"] {
        let fixture = Fixture::new(&[("scene.json", EMPTY)]);
        let path = fixture.root.join("scene.pkg");
        let mut bytes = fs::read(&path).unwrap();
        bytes[4..12].copy_from_slice(signature.as_bytes());
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            inspect_scene(&fixture.source()).unwrap().package.signature,
            signature
        );
        // The payload length must still stay inside the package.
        bytes[34..38].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        assert!(inspect_scene(&fixture.source()).is_err());
    }
}

#[test]
fn inspects_a_scene_without_a_gpu_or_extracted_assets() {
    let fixture = Fixture::new(&[("scene.json", EMPTY)]);
    let report = inspect_scene(&fixture.source()).unwrap();
    assert_eq!(report.package.signature, "PKGV0024");
    assert_eq!(report.package.entries, 1);
    assert_eq!(report.scene.canvas, [3840, 2160]);
    assert_eq!(report.scene.version, 5);
    assert!(report.scene.objects.is_empty());
    assert!(report.diagnostics.is_empty());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 2);
}

#[test]
fn rejects_invalid_scene_structure_without_panicking() {
    for scene in [
        br#"{"version":5,"general":{},"objects":[]}"#.as_slice(),
        br#"{"version":6,"general":{"orthogonalprojection":{"width":1,"height":1}},"objects":[]}"#,
        br#"{"version":5,"general":{"orthogonalprojection":{"width":0,"height":1}},"objects":[]}"#,
    ] {
        let fixture = Fixture::new(&[("scene.json", scene)]);
        assert!(inspect_scene(&fixture.source()).is_err());
    }
}

fn sprite_texture() -> Vec<u8> {
    let mut bytes = b"TEXV0005\0TEXI0001\0".to_vec();
    for value in [0_u32, 4, 4, 4, 4, 4, 0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(b"TEXB0004\0");
    for value in [2_u32, u32::MAX, 0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for _ in 0..2 {
        for value in [1_u32, 4, 4, 0, 64, 64] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 64]);
    }
    bytes.extend_from_slice(b"TEXS0003\0");
    for value in [2_u32, 4, 4] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for page in 0_u32..2 {
        bytes.extend_from_slice(&page.to_le_bytes());
        for value in [0.06_f32, 0., 0., 4., 0., 0., 4.] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

#[test]
fn follows_dependencies_and_indexes_shared_sprite_pages() {
    let texture = sprite_texture();
    let fixture = Fixture::new(&[
        ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":1,"image":"models/sprite.json"},{"id":2,"image":"models/sprite.json"}]}"#),
        ("models/sprite.json", br#"{"material":"materials/sprite.json"}"#),
        ("materials/sprite.json", br#"{"passes":[{"shader":"genericimage4","textures":["sprite"],"combos":{"SPRITESHEET":1}}]}"#),
        ("materials/sprite.tex", &texture),
        ("shaders/genericimage4.vert", b""),
        ("shaders/genericimage4.frag", b""),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    assert_eq!(report.assets.len(), 6);
    assert_eq!(report.textures.len(), 1);
    let texture = &report.textures[0];
    assert_eq!(texture.pages.len(), 2);
    assert_eq!(texture.frames, 2);
    assert_eq!(texture.pages[0].decoded_bytes, 64);
    assert!((texture.duration_seconds - 0.12).abs() < 0.000001);
    assert_eq!(
        report
            .scene
            .objects
            .iter()
            .map(|object| object.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn reports_unsupported_shaders_with_object_context() {
    let fixture = Fixture::new(&[
        ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":7,"image":"models/custom.json"}]}"#),
        ("models/custom.json", br#"{"material":"materials/custom.json"}"#),
        ("materials/custom.json", br#"{"passes":[{"shader":"custom"}]}"#),
        ("shaders/custom.vert", b""),
        ("shaders/custom.frag", b""),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    assert!(report.has_blocking_findings());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "unsupported-shader" && finding.object_id == Some(7))
    );
}

#[test]
fn rejects_unsafe_package_index_ranges_and_paths() {
    for (offset, value) in [(12, u32::MAX), (30, u32::MAX), (34, u32::MAX)] {
        let fixture = Fixture::new(&[("scene.json", EMPTY)]);
        let path = fixture.source().package;
        let mut bytes = fs::read(&path).unwrap();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        fs::write(path, bytes).unwrap();
        assert!(inspect_scene(&fixture.source()).is_err());
    }
    for name in [
        "../scene.json",
        "/scene.json",
        "C:/scene.json",
        "scene.json:stream",
        "models//image.json",
    ] {
        let fixture = Fixture::new(&[(name, EMPTY)]);
        assert!(inspect_scene(&fixture.source()).is_err(), "{name}");
    }
    let duplicate = Fixture::new(&[("scene.json", EMPTY), ("SCENE.JSON", EMPTY)]);
    assert!(
        inspect_scene(&duplicate.source())
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    let overlapping = Fixture::new(&[("scene.json", EMPTY), ("a", b"[]")]);
    let path = overlapping.source().package;
    let mut bytes = fs::read(&path).unwrap();
    bytes[43..47].copy_from_slice(&0_u32.to_le_bytes());
    fs::write(path, bytes).unwrap();
    assert!(
        inspect_scene(&overlapping.source())
            .unwrap_err()
            .to_string()
            .contains("overlapping")
    );
}

#[test]
fn reports_missing_assets_and_upgrades_conditional_requirements() {
    let material = br#"{"passes":[{"shader":"genericimage4","textures":["missing"]}]}"#;
    let fragment = b"uniform sampler2D g_Texture0; // {\"default\":\"missing\"}";
    let fixture = Fixture::image(material, &sprite_texture(), fragment);
    let report = inspect_scene(&fixture.source()).unwrap();
    let missing: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "missing-asset")
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(
        missing[0].severity,
        wallpaper_kit::DiagnosticSeverity::Error
    );
    assert_eq!(missing[0].object_id, Some(7));
    assert_eq!(missing[0].asset, "materials/missing.tex");
    assert!(report.has_blocking_findings());
}

#[test]
fn detects_include_cycles_without_recursing_forever() {
    let fixture = Fixture::image(
        br#"{"passes":[{"shader":"genericimage4"}]}"#,
        &sprite_texture(),
        b"#include \"genericimage4.frag\"",
    );
    let report = inspect_scene(&fixture.source()).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "dependency-cycle" && finding.object_id == Some(7))
    );
}

#[test]
fn rejects_invalid_sprite_timing_pages_and_rectangles() {
    for (offset_from_end, value) in [
        (60, 0_u32),
        (60, f32::NAN.to_bits()),
        (64, 99_u32),
        (48, 99_f32.to_bits()),
    ] {
        let mut texture = sprite_texture();
        let offset = texture.len() - offset_from_end;
        texture[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        let fixture = Fixture::image(
            br#"{"passes":[{"shader":"genericimage4","textures":["sprite"]}]}"#,
            &texture,
            b"",
        );
        let report = inspect_scene(&fixture.source()).unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "invalid-asset"
                    && finding.asset == "materials/sprite.tex"),
            "{:?}",
            report.diagnostics
        );
        assert!(report.textures.is_empty());
    }
}

#[test]
fn particle_sampling_accepts_fractional_clamped_cells_but_image_crops_do_not() {
    let mut texture = sprite_texture();
    let end = texture.len();
    // First frame starts at the page edge and extends by a fractional cell.
    texture[end - 56..end - 52].copy_from_slice(&4_f32.to_le_bytes());
    texture[end - 48..end - 44].copy_from_slice(&0.5_f32.to_le_bytes());
    let particle = br#"{"passes":[{"shader":"genericparticle","textures":["sprite"]}]}"#;
    let image = br#"{"passes":[{"shader":"genericimage4","textures":["sprite"]}]}"#;
    let fixture = Fixture::new(&[
        ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":7,"image":"models/sprite.json"}]}"#),
        ("models/sprite.json", br#"{"material":"materials/sprite.json"}"#),
        ("materials/sprite.json", particle),
        ("materials/sprite.tex", &texture),
        ("shaders/genericparticle.vert", b""),
        ("shaders/genericparticle.frag", br#"uniform sampler2D g_Texture0; // {"default":"sprite"}"#),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "invalid-asset")
    );
    assert_eq!(report.textures.len(), 1);
    let decoder = wallpaper_kit::TextureDecoder::open(&fixture.source()).unwrap();
    assert!(decoder.decode_page("materials/sprite.tex", 0).is_ok());
    assert!(decoder.sprite_frames("materials/sprite.tex").is_err());
    assert!(decoder.sprite_stream("materials/sprite.tex").is_err());

    // A texture shared by both materials must receive image validation even
    // when the particle reference reaches it first.
    for materials in [
        [particle.as_slice(), image.as_slice()],
        [image.as_slice(), particle.as_slice()],
    ] {
        let fixture = Fixture::new(&[
            ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":7,"image":"models/a.json"},{"id":8,"image":"models/b.json"}]}"#),
            ("models/a.json", br#"{"material":"materials/a.json"}"#),
            ("models/b.json", br#"{"material":"materials/b.json"}"#),
            ("materials/a.json", materials[0]),
            ("materials/b.json", materials[1]),
            ("materials/sprite.tex", &texture),
            ("shaders/genericparticle.vert", b""),
            ("shaders/genericparticle.frag", b""),
            ("shaders/genericimage4.vert", b""),
            ("shaders/genericimage4.frag", b""),
        ]);
        let report = inspect_scene(&fixture.source()).unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "invalid-asset"
                    && finding.asset == "materials/sprite.tex")
        );
        assert!(report.textures.len() <= 1);
    }
    texture[end - 56..end - 52].copy_from_slice(&16_385_f32.to_le_bytes());
    let fixture = Fixture::image(particle, &texture, b"");
    let report = inspect_scene(&fixture.source()).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "invalid-asset"
                && finding.asset == "materials/sprite.tex")
    );
}

#[test]
fn package_assets_take_precedence_over_engine_files() {
    let fixture = Fixture::image(
        br#"{"passes":[{"shader":"genericimage4","textures":["sprite"]}]}"#,
        &sprite_texture(),
        b"",
    );
    fs::create_dir(fixture.root.join("engine/models")).unwrap();
    fs::write(
        fixture.root.join("engine/models/sprite.json"),
        b"invalid JSON",
    )
    .unwrap();
    let report = inspect_scene(&fixture.source()).unwrap();
    assert!(!report.has_blocking_findings());
    assert!(report.assets.iter().all(|asset| asset.origin == "package"));
}

#[test]
fn validates_blur_bindings_and_composition_order() {
    let scene = br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":1,"shape":"quad","effects":[{"file":"effects/blur/effect.json"}]},{"id":2,"image":"models/project.json"},{"id":3,"shape":"quad"}]}"#;
    let effect = br#"{"version":1,"replacementkey":"blur_precise","fbos":[{"name":"_rt_Blur","format":"rgba_backbuffer"}],"passes":[{"material":"materials/blur.json","target":"_rt_Blur"},{"material":"materials/blur_y.json","bind":[{"name":"_rt_Blur","index":0},{"name":"previous","index":1}]}]}"#;
    let fixture = Fixture::new(&[
        ("scene.json", scene),
        ("effects/blur/effect.json", effect),
        (
            "materials/blur.json",
            br#"{"passes":[{"shader":"effects/blur_precise_gaussian"}]}"#,
        ),
        (
            "materials/blur_y.json",
            br#"{"passes":[{"shader":"effects/blur_precise_gaussian","combos":{"VERTICAL":1}}]}"#,
        ),
        ("shaders/effects/blur_precise_gaussian.vert", b""),
        ("shaders/effects/blur_precise_gaussian.frag", b""),
        (
            "models/project.json",
            br#"{"material":"materials/composition.json","projectlayer":true}"#,
        ),
        (
            "materials/composition.json",
            br#"{"passes":[{"shader":"composelayer","textures":["_rt_FullFrameBuffer"]}]}"#,
        ),
        ("shaders/composelayer.vert", b""),
        ("shaders/composelayer.frag", b""),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    assert!(!report.has_blocking_findings(), "{:?}", report.diagnostics);
    assert_eq!(
        report.scene.objects[1].kind,
        wallpaper_kit::ObjectKind::Composition
    );
    assert_eq!(report.scene.objects[2].id, 3);
    let bindings = &report.effects[0].passes[1].bindings;
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].source, "_rt_Blur");
    assert_eq!(bindings[1].source, "previous");
    assert_eq!(bindings[1].index, 1);
    let path = fixture.source().package;
    let mut bytes = fs::read(&path).unwrap();
    let index = bytes
        .windows(8)
        .position(|window| window == b"previous")
        .unwrap();
    bytes[index..index + 8].copy_from_slice(b"bad_name");
    fs::write(path, bytes).unwrap();
    assert!(
        inspect_scene(&fixture.source())
            .unwrap()
            .has_blocking_findings()
    );
}

#[test]
fn rejects_invalid_visual_flags_and_combination_values() {
    let scene = br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8},"hdr":"yes"},"objects":[]}"#;
    let fixture = Fixture::new(&[("scene.json", scene)]);
    assert!(inspect_scene(&fixture.source()).is_err());
    let fixture = Fixture::image(
        br#"{"passes":[{"shader":"genericimage4","combos":{"SPRITESHEET":3}}]}"#,
        &sprite_texture(),
        b"",
    );
    let report = inspect_scene(&fixture.source()).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "unsupported-combo-value")
    );
}

#[test]
fn validates_an_asset_first_reached_as_binary() {
    let fixture = Fixture::new(&[
        ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":1,"sound":["models/shared.json"]},{"id":2,"image":"models/shared.json"}]}"#),
        ("models/shared.json", b"invalid JSON"),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    assert_eq!(report.assets.len(), 2);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "invalid-asset" && finding.object_id == Some(2))
    );
}

#[test]
fn propagates_conditional_reachability_and_required_upgrades() {
    for required in [false, true] {
        let mut fragment = "#if OPTIONAL\n#include \"shared.h\"\n#endif\n".to_owned();
        if required {
            fragment.push_str("#include \"shared.h\"\n");
        }
        let fixture = Fixture::image(
            br#"{"passes":[{"shader":"genericimage4"}]}"#,
            &sprite_texture(),
            fragment.as_bytes(),
        );
        fs::create_dir(fixture.root.join("engine/shaders")).unwrap();
        fs::write(
            fixture.root.join("engine/shaders/shared.h"),
            b"#include \"missing.h\"",
        )
        .unwrap();
        let report = inspect_scene(&fixture.source()).unwrap();
        let missing: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "missing-asset")
            .collect();
        assert_eq!(missing.len(), 1);
        assert_eq!(
            missing[0].severity,
            if required {
                wallpaper_kit::DiagnosticSeverity::Error
            } else {
                wallpaper_kit::DiagnosticSeverity::Warning
            }
        );
        let child = report
            .dependencies
            .iter()
            .find(|edge| edge.to == "shaders/missing.h")
            .unwrap();
        assert_eq!(child.conditional, !required);
        assert_eq!(
            report
                .assets
                .iter()
                .filter(|asset| asset.path == "shaders/shared.h")
                .count(),
            1
        );
    }
}

#[test]
fn normalizes_paths_for_composition_and_effect_overrides() {
    let fixture = Fixture::new(&[
        ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":1,"image":"models\\project.json","effects":[{"file":"effects\\cloud.json","passes":[{"combos":{"UNKNOWN":1}}]}]}]}"#),
        ("models/project.json", br#"{"material":"materials\\composition.json","projectlayer":true}"#),
        ("materials/composition.json", br#"{"passes":[{"shader":"composelayer"}]}"#),
        ("shaders/composelayer.vert", b""),
        ("shaders/composelayer.frag", b""),
        ("effects/cloud.json", br#"{"version":2,"replacementkey":"cloudmotion","passes":[{"material":"materials\\cloud.json"}]}"#),
        ("materials/cloud.json", br#"{"passes":[{"shader":"effects/cloudmotion"}]}"#),
        ("shaders/effects/cloudmotion.vert", b""),
        ("shaders/effects/cloudmotion.frag", b""),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    assert_eq!(
        report.scene.objects[0].kind,
        wallpaper_kit::ObjectKind::Composition
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "unsupported-combo" && finding.object_id == Some(1))
    );
}

#[test]
fn diagnoses_changed_effect_shader_and_binding_profiles() {
    for shader in ["genericimage4", "effects/blur_precise_gaussian"] {
        let material =
            serde_json::to_vec(&serde_json::json!({"passes":[{"shader":shader}]})).unwrap();
        let vertex = format!("shaders/{shader}.vert");
        let fragment = format!("shaders/{shader}.frag");
        let fixture = Fixture::new(&[
            ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":1,"shape":"quad","effects":[{"file":"effects/blur.json"}]}]}"#),
            ("effects/blur.json", br#"{"version":1,"replacementkey":"blur_precise","passes":[{"material":"materials/blur.json"},{"material":"materials/blur.json"}]}"#),
            ("materials/blur.json", &material),
            (&vertex, b""),
            (&fragment, b""),
        ]);
        let report = inspect_scene(&fixture.source()).unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "unsupported-effect-profile")
        );
    }
}

#[test]
fn diagnoses_unrecognized_general_and_asset_fields() {
    let fixture = Fixture::new(&[
        ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8},"customlighting":true},"objects":[{"id":1,"image":"models/image.json"}]}"#),
        ("models/image.json", br#"{"material":"materials/image.json","mesh":"custom.obj"}"#),
        ("materials/image.json", br#"{"passes":[{"shader":"genericimage4","customblend":true}]}"#),
        ("shaders/genericimage4.vert", b""),
        ("shaders/genericimage4.frag", b""),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    for field in ["customlighting", "mesh", "customblend"] {
        assert!(
            report.diagnostics.iter().any(|finding| finding.severity
                == wallpaper_kit::DiagnosticSeverity::Unsupported
                && finding.message.contains(field)),
            "{field}: {:?}",
            report.diagnostics
        );
    }
}

#[test]
fn diagnoses_unrecognized_effect_instance_and_override_fields() {
    let fixture = Fixture::new(&[
        ("scene.json", br#"{"version":5,"general":{"orthogonalprojection":{"width":8,"height":8}},"objects":[{"id":1,"shape":"quad","effects":[{"file":"effects/cloud.json","customtransition":true,"passes":[{"customblend":true}]}]}]}"#),
        ("effects/cloud.json", br#"{"version":2,"replacementkey":"cloudmotion","passes":[{"material":"materials/cloud.json"}]}"#),
        ("materials/cloud.json", br#"{"passes":[{"shader":"effects/cloudmotion"}]}"#),
        ("shaders/effects/cloudmotion.vert", b""),
        ("shaders/effects/cloudmotion.frag", b""),
    ]);
    let report = inspect_scene(&fixture.source()).unwrap();
    for field in ["customtransition", "customblend"] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "unsupported-field"
                    && finding.message.contains(field)
                    && finding.object_id == Some(1)),
            "{field}: {:?}",
            report.diagnostics
        );
    }
}
