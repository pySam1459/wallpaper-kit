//! Generated local-video contracts; require FFmpeg tools on PATH.

use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use wallpaper_kit::{ImagePlayback, ImagePlaybackSample, PlaybackSettings, SceneSource, Size};

#[path = "../examples/support/texture.rs"]
#[allow(dead_code)] // Reuse the generated format helpers shared with other consumers.
mod fixture;

static RUN: AtomicU64 = AtomicU64::new(0);
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        fs::remove_file(self.0.join("input.mkv")).unwrap();
        if self.0.join("scene.pkg").exists() {
            fs::remove_file(self.0.join("scene.pkg")).unwrap();
        }
        fs::remove_dir(self.0.join("engine")).unwrap();
        fs::remove_dir(&self.0).unwrap();
    }
}
fn sample(playback: &ImagePlayback) -> ImagePlaybackSample {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(sample) = playback.try_sample().unwrap() {
            return sample;
        }
        assert!(Instant::now() < deadline, "video sample timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires installed ffmpeg.exe and ffprobe.exe; run with --ignored"]
fn live_video_finishes_pending_samples_while_the_clock_advances() {
    let temporary = Temporary(std::env::temp_dir().join(format!(
        "wallpaper-live-video-contract-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&temporary.0).unwrap();
    fs::create_dir(temporary.0.join("engine")).unwrap();
    let video = temporary.0.join("input.mkv");
    let output = Command::new("ffmpeg.exe")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=16x16:rate=60:duration=4",
            "-c:v",
            "ffv1",
        ])
        .arg(&video)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut playback = ImagePlayback::open_video(
        &video,
        Size::new(16, 16).unwrap(),
        PlaybackSettings::default(),
    )
    .unwrap();
    playback.play();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut indices = Vec::new();
    while Instant::now() < deadline && indices.len() < 3 {
        // A host polls once per update, allowing decode to finish between polls.
        std::thread::sleep(Duration::from_millis(40));
        if let Some(sample) = playback.try_sample().unwrap() {
            assert!(sample.time <= playback.time());
            assert_eq!(
                sample.frame_indices,
                [(sample.time.as_nanos() * 60 / 1_000_000_000) as usize]
            );
            if indices.last() != sample.frame_indices.first() {
                indices.push(sample.frame_indices[0]);
            }
        }
    }
    assert_eq!(
        indices.len(),
        3,
        "Live video must publish progressing frames, got {indices:?}"
    );
    // A seek replaces an unfinished live selection with the exact requested time.
    playback.try_sample().unwrap();
    playback.seek(Duration::ZERO);
    let rewind = sample(&playback);
    assert_eq!(rewind.time, Duration::ZERO);
    assert_eq!(rewind.frame_indices, [0]);
    playback.shutdown().unwrap();
}

#[test]
#[ignore = "requires installed ffmpeg.exe and ffprobe.exe; run with --ignored"]
fn video_seeks_loops_retains_frames_and_decodes_embedded_payloads() {
    let temporary = Temporary(std::env::temp_dir().join(format!(
        "wallpaper-video-contract-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&temporary.0).unwrap();
    fs::create_dir(temporary.0.join("engine")).unwrap();
    let video = temporary.0.join("input.mkv");
    let output = Command::new("ffmpeg.exe")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=16x16:rate=30:duration=1",
            "-c:v",
            "ffv1",
        ])
        .arg(&video)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut playback = ImagePlayback::open_video(
        &video,
        Size::new(16, 16).unwrap(),
        PlaybackSettings::default(),
    )
    .unwrap();
    let first = sample(&playback);
    assert_eq!(first.frame_indices, [0]);
    let first_pixels = first.composition.layers()[0]
        .region()
        .page()
        .data()
        .to_vec();
    playback.seek(Duration::from_millis(500));
    assert_eq!(sample(&playback).frame_indices, [15]);
    assert_ne!(
        sample(&playback).composition.layers()[0]
            .region()
            .page()
            .data(),
        first_pixels
    );
    playback.seek(Duration::from_secs(1));
    assert_eq!(
        sample(&playback).composition.layers()[0]
            .region()
            .page()
            .data(),
        first_pixels
    );
    playback.seek(Duration::ZERO);
    assert_eq!(
        sample(&playback).composition.layers()[0]
            .region()
            .page()
            .data(),
        first_pixels
    );
    playback.play();
    std::thread::sleep(Duration::from_millis(100));
    assert!(playback.time() >= Duration::from_millis(100));
    playback.pause();
    let time = playback.time();
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(playback.time(), time);
    playback.shutdown().unwrap();
    assert_eq!(
        first.composition.layers()[0].region().page().data(),
        first_pixels
    );

    let payload = fs::read(&video).unwrap();
    let mut texture = fixture::tex(0, [16, 16], &payload, false, false);
    texture[22..26].copy_from_slice(&34_u32.to_le_bytes());
    texture[83..87].copy_from_slice(&0_u32.to_le_bytes());
    for legacy in [false, true] {
        let mut texture = texture.clone();
        if legacy {
            texture[46..55].copy_from_slice(b"TEXB0003\0");
            texture.drain(63..67);
        }
        let scene=br#"{"version":0,"general":{"orthogonalprojection":{"width":16,"height":16}},"objects":[{"id":1,"image":"models/image.json","origin":"8 8 0","size":"16 16"}]}"#;
        let package = fixture::package(&[
            ("gifscene.json", scene),
            (
                "models/image.json",
                br#"{"material":"materials/image.json"}"#,
            ),
            (
                "materials/image.json",
                br#"{"passes":[{"shader":"genericimage2","textures":["image"]}]}"#,
            ),
            ("materials/image.tex", &texture),
        ]);
        fs::write(temporary.0.join("scene.pkg"), package).unwrap();
        let source = SceneSource {
            package: temporary.0.join("scene.pkg"),
            engine_assets: temporary.0.join("engine"),
        };
        let playback = ImagePlayback::open_scene_images_with_settings(
            &source,
            PlaybackSettings {
                allow_unsupported_features: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            sample(&playback).composition.layers()[0]
                .region()
                .page()
                .data(),
            first_pixels
        );
        playback.shutdown().unwrap();
    }
}
