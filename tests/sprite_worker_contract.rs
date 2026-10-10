//! Background sprite preparation through the public consumer API.

#[path = "../examples/support/texture.rs"]
mod fixture;

use scene_renderer::{DeviceMode, Renderer, SceneSource, Size, SpriteResult, SpriteWorker};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static RUN: AtomicU64 = AtomicU64::new(0);

struct Package(std::path::PathBuf);
impl Package {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sprite-worker-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("engine")).unwrap();
        fs::write(root.join("scene.pkg"), fixture::package(&[])).unwrap();
        Self(root)
    }
    fn source(&self) -> SceneSource {
        SceneSource {
            package: self.0.join("scene.pkg"),
            engine_assets: self.0.join("engine"),
        }
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        assert_eq!(
            self.0.canonicalize().unwrap(),
            self.0
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .join(self.0.file_name().unwrap())
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn request(worker: &SpriteWorker, time: Duration) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(id) = worker.try_request_at(time).unwrap() {
            return id;
        }
        assert!(Instant::now() < deadline, "request metadata stayed busy");
        std::thread::yield_now();
    }
}
fn ready(worker: &SpriteWorker, id: u64) -> SpriteResult {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = worker.try_take().unwrap() {
            assert_eq!(result.request_id(), id);
            return result;
        }
        assert!(Instant::now() < deadline, "sprite preparation timed out");
        std::thread::yield_now();
    }
}

#[test]
fn async_errors_recover_and_prepared_samples_survive_shutdown() {
    let package = Package::new();
    let (worker, events) = SpriteWorker::new(package.source(), "image.tex".into()).unwrap();
    assert!(worker.try_take().unwrap().is_none());
    let id = request(&worker, Duration::ZERO);
    let failed = ready(&worker, id);
    assert_eq!(failed.time(), Duration::ZERO);
    assert_eq!(failed.stats().page_decodes, 0);
    assert!(
        failed
            .into_sample()
            .unwrap_err()
            .to_string()
            .contains("missing")
    );

    let case = fixture::cases().remove(0);
    fs::write(package.0.join("engine/image.tex"), &case.tex).unwrap();
    let id = request(&worker, Duration::ZERO);
    let first = ready(&worker, id);
    assert_eq!(first.stats().page_decodes, 1);
    let sample = first.into_sample().unwrap();
    assert_eq!(sample.frame_index(), 0);
    let pixels = sample.region().page().data().to_vec();
    let (renderer, frame_events) =
        Renderer::new(Size::new(31, 23).unwrap(), DeviceMode::Software).unwrap();
    let rendered = renderer
        .render_texture_region(sample.region().clone())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let frame = loop {
        if let Some(frame) = renderer.try_frame().unwrap()
            && frame.id() == rendered
        {
            break frame;
        }
        assert!(
            Instant::now() < deadline,
            "prepared sprite GPU draw timed out"
        );
        let _ = frame_events.recv_timeout(Duration::from_millis(10));
    };
    let authored = sample.authored_frame();
    let expected = fixture::crop(
        &case.rgba,
        case.extent,
        authored.origin(),
        authored.extent(),
    );
    fixture::check(&frame.readback().unwrap(), authored.extent(), &expected)
        .unwrap_or_else(|error| panic!("{}: {error}", case.name));
    renderer.shutdown().unwrap();
    // Consuming the wakeup is optional; an undrained notification must not
    // prevent another result from being published.
    let id = request(&worker, Duration::from_millis(1));
    let repeated = ready(&worker, id);
    assert_eq!(repeated.stats().frame_extracts, 1);
    assert!(std::sync::Arc::ptr_eq(
        sample.region(),
        repeated.into_sample().unwrap().region()
    ));
    let id = request(&worker, Duration::from_millis(100));
    let next = ready(&worker, id);
    assert_eq!(next.into_sample().unwrap().frame_index(), 1);
    worker.shutdown().unwrap();
    assert_eq!(
        events.try_iter().count(),
        1,
        "notifications were not bounded"
    );
    assert_eq!(sample.region().page().data(), pixels);
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(package.0.join("engine/image.tex"))
            .is_ok()
    );
}
