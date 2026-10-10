//! Generated public composition/snapshot contracts, with real WARP draws.

#[path = "../examples/support/layers.rs"]
mod layers_checks;
#[path = "../examples/support/texture.rs"]
mod texture_checks;

use scene_renderer::{
    DeviceMode, Frame, ImageComposition, ImageLayer, OutputStats, Renderer, SceneSource, Size,
    TextureDecoder, TextureRegion,
};
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
    assert_eq!(first.image_layer_count(), Some(4));
    let initial_stats = stats(&renderer, first.sequence());
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
    assert_eq!(
        (page_stats.texture_uploads, page_stats.source_texture_bytes),
        (3, 16)
    );
    renderer.shutdown().unwrap();
    fixture.check(&first.readback().unwrap()).unwrap();
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
    let package = snapshot_package(json!([object(1)]), cropped, material(false), &texture);
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
