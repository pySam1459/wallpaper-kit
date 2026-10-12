# Workshop loading progress

From the repository root, run:

```powershell
pwsh -File scripts/workshop-support.ps1
```

The local Git Bash convenience wrapper is `.local/loop.sh`. The `.local/`
directory is ignored and is not included in a fresh checkout.

The only standard-output line is a percentage, for example `100.0000`. Build
messages, per-folder results, unsupported IDs and the evidence path go to
standard error. `report.json` contains every folder's verdict, source hash,
omissions and pixel hashes. `unsupported.txt` lists failed folder numbers.
Every numbered folder under the Workshop root counts, including missing,
malformed and unsupported projects. An empty collection fails the command.
Progress changes if the collection, engine assets, decoder tools or code change.

`-WorkshopRoot`, `-EngineAssets` and `-Output` override the local paths. The default
roots are Steam's `workshop/content/431960` and `wallpaper_engine/assets`.
Each run requires a new evidence directory. The script builds the real library
consumer with the locked dependency graph; `-SkipBuild` is available for repeated
runs of an unchanged build. An executable snapshot pins the whole collection to
one build and lets Cargo rebuild without replacing a running Windows executable.

## What passes

Each folder runs in its own owned process using the public `ImagePlayback` and
`Renderer` APIs. It loads the actual scene package or authored video, samples
0, 1,500 and 4,000 milliseconds, and then rewinds to zero. Each sample must
produce a nonempty D3D11 WARP composition and pixels beyond its background
color. Rewind must reproduce the first raw-pixel hash. The first PNG is
retained for inspection. Calendar text uses a fixed wall time; particles use
the library's deterministic seeds. There are separate 20-second preparation
and 30-second GPU completion budgets, plus a 90-second process limit.

The probe also starts the playback clock and polls at 33-millisecond intervals.
It requires three completed samples at increasing times within 20 seconds,
recorded in `live_sample_times_ns`. This catches live video starvation that exact
seeks cannot detect. It verifies source-sampling progress, not live GPU presentation
or smooth display timing.

The [Tram regression batch](../artifacts/workshop-support-tram-live/report.json)
passed all 65 folders with three progressing live samples after fixing pending
selection starvation. The batch records sample objects; the final probe records
their times as `live_sample_times_ns`. Both use the same progress criterion.
The [Tram verification receipt](../artifacts/tram-playback/report.json) includes
the original live failure and successful native/addon checks.

The metric measures loading and rendering with reported omissions, as requested.
Unsupported optional effects, particle/text profiles, scripts, puppet deformation
and composition effects may be omitted. Base-image decode failures, unsafe paths,
invalid package ranges, empty output, worker errors and timeouts fail. A preview
image is never used as a substitute. This does not measure complete Wallpaper
Engine fidelity, display cadence or host responsiveness.

The [fidelity catalog](workshop-fidelity.md) counts remaining gaps and documents
the fix order. Scene discovery honors enabled authored clear color; that color
alone does not pass the loading probe.

## Library and native viewer

Enable tolerant discovery explicitly:

```rust
let settings = wallpaper_kit::PlaybackSettings {
    allow_unsupported_features: true,
    ..Default::default()
};
let playback = wallpaper_kit::ImagePlayback::open_scene_images_with_settings(
    &source, settings,
)?;
```

`report().omissions` describes every fallback. The default checked profile and
explicit selected-image APIs remain strict. The native viewer's `--scene` route
enables tolerant discovery. Package signatures `PKGV0001` through `PKGV0024`,
scene versions zero through five, absent legacy versions, `gifscene.json` roots,
and `TEXS0001` integer sprite tables are admitted with structural validation.
Large logical canvases are normalized to bounded coordinates independently of
source texture and consumer output resolution.

Standalone videos use `ImagePlayback::open_video(path, decode_size, settings)`.
The same sampling, seeking, play/pause, freeze and shutdown API applies. For the
native viewer:

```powershell
cargo run --locked --example native-viewer -- --video 'C:/path/to/video.mp4'
```

Video requires installed `ffmpeg.exe` and `ffprobe.exe` on PATH. The implementation
uses FFmpeg's documented [seek, stream-selection and raw output options](https://ffmpeg.org/ffmpeg.html).
No FFmpeg code is linked or redistributed. A background worker keeps one ready
frame, one active decode and one coalesced request. It sleeps without demand,
reuses a running decoder for nearby forward samples, and restarts on rewind or
larger seeks. Videos loop at their rational average source rate, up to 240 fps,
and omit audio. Variable-rate videos use this average cadence; playback does
not synthesize intermediate frames. Frames are decoded on the CPU
and uploaded as RGBA; this is not GPU video sharing. Caller-retained samples and
codec internal buffers are outside the frame-storage bound.

TEXB0003/0004 embedded-video textures copy their validated local payload to a
uniquely owned temporary file using a 64 KiB buffer. The scene package remains
read-only. Shutdown cancels and reaps the owned codec process, joins its worker,
then removes its temporary asset. Missing tools or undecodable primary media
fail with context.

## Verification

The 12 October 2026 collection baseline was 4/65 (`6.1538`). The
[final collection run](../artifacts/workshop-support-final-b/report.json)
loaded and drew 65/65 (`100.0000`). The
[repeat run](../artifacts/workshop-support-final-c/report.json) used the same
executable and reproduced all 65 verdicts and every sampled pixel hash. No probe
or codec processes, or temporary embedded-video files, remained after the runs.
The
[required hardware/native gate](../artifacts/20261012-140148-627c94b7/manifest.json)
passed, including the generated video contract. Separate native checks passed
for [standalone video](../artifacts/native-video-final/report.json) and
[embedded video](../artifacts/native-embedded-final/report.json), including seek,
rewind, pause, resize, live sampling and owned-process exit. These receipts
establish loading with omissions; puppet meshes and complete authored effects
remain unsupported.

`scripts/verify.ps1` runs the regular library/native gate and the generated video
contract when both decoder tools are available. Otherwise its manifest records
video decoding as skipped. Run that contract directly with:

```powershell
cargo test --locked --test video_contract -- --ignored
```

Generated contracts cover tolerant omissions while preserving corrupt-source and
path rejection, legacy sprite bounds, large-canvas GPU coverage, video seek,
loop, pause, retained samples and both embedded-video container versions.

`scripts/verify-workshop-playback.ps1` accepts either paired `-ScenePath` and
`-EngineAssets`, or `-VideoPath`. It drives native seeks, rewind, pause, resize,
eight seconds of live playback and owned-process cleanup. Controlled pixels and
live presentation observations are separate evidence.
