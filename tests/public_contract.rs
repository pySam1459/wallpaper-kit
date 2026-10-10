//! Public GPU contract under request bursts and a slow notification consumer.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use wallpaper_kit::{DeviceMode, Renderer, Size};

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

fn wait_for(renderer: &Renderer, id: u64) -> Arc<wallpaper_kit::Frame> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(frame) = renderer.try_frame().unwrap()
            && frame.id() == id
        {
            return frame;
        }
        assert!(Instant::now() < deadline, "request {id} timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn retained_frames_bound_output_and_resume_on_release() {
    let (renderer, _events) =
        Renderer::new(Size::new(64, 33).unwrap(), DeviceMode::Software).unwrap();
    let first = wait_for(&renderer, 1);
    let first_pixels = first.readback().unwrap().rgba;
    let second_id = renderer
        .render_fixture_at(Duration::from_millis(500))
        .unwrap();
    let second = wait_for(&renderer, second_id);
    let third_id = renderer.resize(Size::new(65, 35).unwrap()).unwrap();
    let third = wait_for(&renderer, third_id);
    let third_pixels = third.readback().unwrap().rgba;
    let pending = renderer
        .render_fixture_at(Duration::from_millis(1000))
        .unwrap();
    let release_deadline = Instant::now() + Duration::from_secs(10);
    let saturated = loop {
        if let Some(stats) = renderer.try_output_stats().unwrap()
            && stats.waiting_for_frame_release
        {
            break stats;
        }
        assert!(
            Instant::now() < release_deadline,
            "worker did not report pool saturation"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(saturated.resident_textures, 3);
    assert_eq!(saturated.texture_allocations, 3);
    let deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < deadline {
        if let Some(frame) = renderer.try_frame().unwrap() {
            assert_eq!(
                frame.id(),
                third_id,
                "retained frames allowed a fourth output texture"
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(second);
    let resumed = wait_for(&renderer, pending);
    assert_eq!(resumed.fixture_time(), Some(Duration::from_millis(1000)));
    assert_eq!(first.readback().unwrap().rgba, first_pixels);
    assert_eq!(third.readback().unwrap().rgba, third_pixels);
    // Shutdown also succeeds while every slot is leased and another request is pending.
    renderer.resize(Size::new(77, 39).unwrap()).unwrap();
    renderer.shutdown().unwrap();
    assert_eq!(first.readback().unwrap().rgba, first_pixels);
    assert_eq!(third.readback().unwrap().rgba, third_pixels);
}

#[test]
fn steady_motion_reuses_textures_and_resize_stays_bounded() {
    let size = Size::new(64, 33).unwrap();
    let (renderer, _events) = Renderer::new(size, DeviceMode::Software).unwrap();
    let first = wait_for(&renderer, 1);
    let mut allocations = None;
    for index in 0..100 {
        let id = renderer
            .render_fixture_at(Duration::from_millis(index * 50))
            .unwrap();
        let frame = wait_for(&renderer, id);
        assert_eq!(frame.sequence(), index + 2);
        let stats_deadline = Instant::now() + Duration::from_secs(10);
        let stats = loop {
            if let Some(stats) = renderer.try_output_stats().unwrap() {
                break stats;
            }
            assert!(
                Instant::now() < stats_deadline,
                "pool metadata stayed locked"
            );
            std::thread::yield_now();
        };
        assert!(stats.resident_textures <= 3);
        assert!(stats.resident_bytes <= u64::from(size.width()) * u64::from(size.height()) * 4 * 3);
        if index == 10 {
            allocations = Some(stats.texture_allocations);
        }
        if index > 10 {
            assert_eq!(Some(stats.texture_allocations), allocations);
        }
    }
    for burst in 0..20 {
        let mut id = 0;
        for width in 48..80 {
            id = renderer
                .resize(Size::new(width, 35 + burst).unwrap())
                .unwrap();
        }
        let frame = wait_for(&renderer, id);
        assert_eq!(frame.size(), Size::new(79, 35 + burst).unwrap());
        if let Some(stats) = renderer.try_output_stats().unwrap() {
            assert!(stats.resident_textures <= 3);
            assert!(stats.resident_bytes <= 3 * 79 * 54 * 4);
        }
    }
    renderer.shutdown().unwrap();
    assert!(
        first
            .readback()
            .unwrap()
            .rgba
            .chunks_exact(4)
            .all(|pixel| pixel == [16, 24, 32, 255])
    );
}
