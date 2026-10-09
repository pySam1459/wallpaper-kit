//! Public GPU contract under request bursts and a slow notification consumer.

use scene_renderer::{DeviceMode, Renderer, Size};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn pending_requests_and_slow_notifications_preserve_latest_output() {
    let (renderer, _undrained_notifications) =
        Renderer::new(Size::new(64, 33).unwrap(), DeviceMode::Software).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let first = loop {
        if let Some(frame) = renderer.try_frame().unwrap() {
            break frame;
        }
        assert!(Instant::now() < deadline, "startup timed out");
        std::thread::sleep(Duration::from_millis(1));
    };
    // Keep the original notification queued: a slow consumer must not block
    // publishing the result of the final coalesced request.
    let mut id = 1;
    for width in 100..600 {
        id = renderer.resize(Size::new(width, 31).unwrap()).unwrap();
    }
    let final_frame = loop {
        if let Some(frame) = renderer.try_frame().unwrap()
            && frame.id() == id
        {
            break frame;
        }
        assert!(Instant::now() < deadline, "final request timed out");
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(final_frame.size(), Size::new(599, 31).unwrap());
    renderer.shutdown().unwrap();
    // Concurrent diagnostic consumers use separate contexts and synchronize
    // the same retained immutable GPU frame after its producer has stopped.
    let other = Arc::clone(&final_frame);
    let capture = std::thread::spawn(move || other.readback().unwrap());
    let pixels = final_frame.readback().unwrap();
    let other_pixels = capture.join().unwrap();
    assert_eq!(pixels.rgba, other_pixels.rgba);
    assert!(
        pixels
            .rgba
            .chunks_exact(4)
            .all(|pixel| pixel == [16, 24, 32, 255])
    );
    assert_eq!(first.readback().unwrap().size, Size::new(64, 33).unwrap());
}
