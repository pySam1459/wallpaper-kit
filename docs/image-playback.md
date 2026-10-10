# Selected image playback

`ImagePlayback` combines the checked selected-image profile with sprite prefetch and a library-owned clock. The native viewer can play six supported image objects from Workshop item `3780699842` together. Full scene rendering remains incomplete.

## Public consumer path

Open `ImagePlayback::open(&SceneSource, &[object_ids])` on an asset thread or before creating a window. Opening performs blocking package hashing, JSON validation and static-image decoding. It starts one background pager per unique animated source. Duplicate bird instances share the same pager and sampled region. The package stays read-locked until playback is dropped or shut down.

Playback starts paused at zero. `play`, `pause` and `seek(Duration)` control its monotonic clock. Seek pauses at the exact requested time; play resumes from there. Repeated play calls preserve the start instant. Time saturates at `Duration::MAX`.

The host schedules `try_sample` through its event loop. One call captures a common time, polls every animated source without waiting, and constructs ordered layer descriptors. It returns `None` until all sources for that time are ready. Keep the previous completed GPU frame while retrying. Errors identify invalid assets or compositions. Submit a ready sample's composition to `Renderer::render_images`, then consume `Renderer::try_frame` as usual. Retained samples and frames remain valid after shutdown.

Static pixels decode once. Sprite decoding runs on the existing pager workers. Sampling allocates descriptors and Arc references, without copying pixels, decoding, shader compilation or file I/O. GPU uploads and drawing run on the renderer worker. Drop and `shutdown` join pagers, so they belong outside latency-critical UI updates.

Existing limits apply: 1..=32 distinct selected objects, 256 MiB of static source pages, 256 MiB of unique pages in a sampled composition, bounded per-pager batches and the three-output GPU pool. These limits do not constitute an aggregate scene or process memory budget. The host still chooses when to sample; autonomous rendering, replacement loading, device-loss recovery and visibility controls remain later work.

## Native controls

The generated fixture uses a static background, two instances of a shared bird animation and a character animation with different durations:

```powershell
cargo run --locked --example native-viewer -- --image-fixture
```

Run the actual target with explicit local roots:

```powershell
cargo run --locked --example native-viewer -- --scene 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg' --engine-assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'
```

This target route selects objects `28, 32, 72, 89, 77, 86` in authored order. It starts playback after the first complete composition is presented. The viewer's stdout readiness event includes the package hash, selected IDs, unique animated assets and omissions. It retains the previous frame when a source is unavailable.

The existing line-delimited JSON protocol adds these commands:

| Command | Result |
| --- | --- |
| `{"id":1,"command":"image_at","time_ms":1500}` | Pause, seek and reply only after that exact complete sample is presented |
| `{"id":2,"command":"play_images"}` | Resume the library clock and host sampling at a requested 30 Hz |
| `{"id":3,"command":"pause_images"}` | Freeze the clock; an outstanding GPU request may still finish |
| `inspect`, `resize`, `capture`, `stop` | Existing operations; captures refer to the completed composition |

Settle outstanding work with an acknowledged `image_at` before measuring paused idle. Play/pause commands are rejected while seek or resize replies are pending. `image-playback.json` records source identity and omissions. `image-timeline.json` records bounded presentation-call observations. Exact nanosecond times are decimal strings to preserve values beyond JSON's integer range. Capture readback blocks only the diagnostic command.

## Verification

The required gate runs public WARP contracts and a separate native combined fixture. Paired `-ScenePath` and `-EngineAssets` also run the target image playback controller when native checks are enabled. Use `-Software -SkipNative` without a desktop and report native playback as skipped.

`scripts/verify-image-playback.ps1` drives the owned viewer through controlled frame-duration/page/loop checkpoints, pause, resize and a 4.2-second live interval with no capture readback. The fixture's independent pixel oracle checks every pixel, including shared instances and layer order. Fixture placement avoids exact texel-boundary ties at the tested resolutions. Target checks assert six layers, common-time sprite indices and explicit omissions; its captures have no independent pixel oracle.

Public contracts check clock freeze/resume, shared region identities, static reuse, GPU pixels, looping, resized output, retained samples after joined shutdown, invisible selections and asynchronous sprite failures. The package guard prevents replacement during playback and releases on shutdown.

The [hardware/native gate](../artifacts/20261010-161844-39ba6c38/manifest.json) and [WARP/headless gate](../artifacts/20261010-162025-a9546dab/manifest.json) passed on 10 October 2026 with Rust 1.91, Windows x64, a Ryzen 9 9950X3D and an NVIDIA RTX 5080 for hardware. All 54 tests, formatting, Clippy, Rustdoc, dependency boundaries and consumer scenarios passed. WARP explicitly skipped native and proprietary target checks. Doctor accepts the matching diagnostic receipt and still reports full scene support unavailable.

The [target report](../artifacts/20261010-161844-39ba6c38/native-target-images/report.json) records six layers, controlled sprite indices, pause, resize and owned-process exit. Both native fixture and target runs observed 126 live presentation calls during their defined 4.2-second intervals. [Target captures](../artifacts/20261010-161844-39ba6c38/native-target-images/image-01500.png) show the supported layers and both sprite sequences at different times. They do not establish visual fidelity against Wallpaper Engine, smooth display cadence, GPU duration or host responsiveness. The live target interval crosses multiple sprite page boundaries; it does not cover its complete 28.8-second loop. Controlled checkpoints cover the loop boundary separately. Ignored evidence may be absent in other checkouts; rerun there.

## Remaining target features

The report names omitted cloud motion, precise blur, parallax, camera/color/postprocessing settings and unselected objects. Object `22` has an unresolved crop offset and remains excluded. The particle layer, procedural light shafts, full-scene composition effect and muted audio remain omitted. Point sampling and straight-alpha SDR blending are the current image profile. Full scene and MVP acceptance still require those visible features and comparison with Wallpaper Engine, followed by resource and host-impact measurements.
