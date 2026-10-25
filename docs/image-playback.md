# Selected image playback

[Scene property bindings](property-bindings.md) resolve user-option visibility
and text/color links. Checked clocks/calendars retain their update behavior;
`ImagePlaybackSample::text_values` exposes submitted text for verification.
Prepare property changes as a replacement playback outside the host UI thread.

Scene discovery supports basic masked UV Spin, planar single-wave Water Waves
and [periodic Shake](shake.md). Their masks decode once, share asset identity, and are sampled on
the GPU at the common playback time. `ImagePlaybackReport::image_motions` records
the first applied motion profile per object. Seeking and pausing use the same clock as sprites.
Perspective, dual waves, elliptical/vertex spin and additional constants remain
unsupported and explicitly reported. The independent sine-wave
and rotation implementations have not been calibrated for exact Wallpaper Engine
phase, speed or displacement agreement.

[Ordered image effects](image-effect-chains.md) apply supported passes in authored
order, including repeated profiles. `report.effect_chains` records the complete
applied sequence. Unsupported optional passes remain named omissions under
tolerant loading. The effect-specific report fields retain the first applied
pass of each kind.

Run `pwsh -File scripts/verify-image-motion.ps1 -ScenePath '<scene.pkg>'
-EngineAssets '<engine assets>'` after building the native viewer. It checks real
scene pixel changes, exact rewind, an eight-second live interval without readback,
paused idle, 4K output and owned-process exit. Panda Store's eight applied profiles
passed in [the native receipt](../artifacts/panda-fidelity/native-verified/report.json).
Generated GPU tests also check masked pixels and an independent cardinal rotation
oracle. Live frame observations establish rendering progress, not display cadence.

Scene discovery applies enabled authored `general.clearcolor` to uncovered and
transparent pixels. The composition exposes it through `background_color()`;
the playback report records the applied input. Selected-image playback retains
the diagnostic background. See [collection fidelity priorities](workshop-fidelity.md)
for verification and the remaining reference limits.

`ImagePlayback` combines the checked selected-image profile with sprite prefetch and a library-owned clock. The native viewer can play all seven image objects from Workshop item `3780699842` together. Full scene rendering remains incomplete.

## Public consumer path

Open `ImagePlayback::open(&SceneSource, &[object_ids])` on an asset thread or before creating a window. Opening performs blocking package hashing, JSON validation and static-image decoding. It starts one background pager per unique animated source. Duplicate bird instances share the same pager and sampled region. The package stays read-locked until playback is dropped or shut down.

`ImagePlayback::open_scene_images(&SceneSource)` discovers image objects in authored order without an ID list. It uses the same checked image profile and preparation boundaries. Invisible objects, non-image content and models marked `projectlayer=true` are reported as omitted; the latter are composition points rather than source images. Unsupported visible image properties/materials and malformed inputs fail with context. Generated contracts use different IDs/order, duplicate sources and omitted content to check discovery independently of the target. The checked [precise-blur profile](precise-blur.md) loads authored scales, and [cloud motion](cloud-motion.md) loads noise, masks and time parameters. Other effects and complete scene compilation remain open.

Playback starts paused at zero. `play`, `pause` and `seek(Duration)` control its monotonic clock. Seek pauses at the exact requested time; play resumes from there. Repeated play calls preserve the start instant. Time saturates at `Duration::MAX`.

[Tolerant Workshop discovery](workshop-support.md) uses `allow_unsupported_features` to retain base imagery and report optional feature failures. The native scene route enables it; explicit selected-image APIs keep their checked profile. `open_video` supplies standalone video through the same clock, composition and control API using a bounded FFmpeg worker. Embedded TEX videos use that worker during tolerant scene preparation. `set_calendar_time(SystemTime)` fixes calendar text for repeatable captures.

[Consumer initialization settings](playback-settings.md) select image sampling, animation, VFX and reduced flashing. `freeze(&last_presented_sample)` retains the displayed composition at its exact time; the host retains its GPU frame and discards later in-flight completions.

The host schedules `try_sample` through its event loop. One call captures a common time, polls every animated source without waiting, and constructs ordered layer descriptors. It returns `None` until all sources for that time are ready. Keep the previous completed GPU frame while retrying. Errors identify invalid assets or compositions. Submit a ready sample's composition to `Renderer::render_images`, then consume `Renderer::try_frame` as usual. Retained samples and frames remain valid after shutdown.

Retries keep one pending selection time while decoding finishes. A successful
sample releases that selection; the next call captures the current clock and
can skip elapsed frames. This prevents live video from starving when decode
takes longer than a host update. All layers use the selected sample time, which
can lag the clock. Seek, transition to pause, and freeze discard the selection.
Repeated paused polls preserve it so an unfinished still can complete.

Static images, noise and masks decode once. Cloud effects use the same sample time as sprites. Sprite decoding runs on the existing pager workers. Sampling allocates descriptors and Arc references, without copying pixels, decoding, shader compilation or file I/O. GPU uploads and drawing run on the renderer worker. Drop and `shutdown` join pagers, so they belong outside latency-critical UI updates.

Existing limits apply: 1..=32 distinct selected objects, 256 MiB of static source pages, 256 MiB of unique pages in a sampled composition, bounded per-pager batches and the three-output GPU pool. These limits do not constitute an aggregate scene or process memory budget. The host still chooses when to sample; autonomous rendering, replacement loading, device-loss recovery and visibility controls remain later work.

## Native controls

Scene discovery also includes the [bounded 2D particle profile](scene-particles.md). Prepared particle IDs are recorded separately from image IDs; returned compositions can include up to 4,096 particle quads between the authored images. Explicit image selection remains image-only. Unsupported particle components are reported as omitted.

Scene discovery additionally prepares translucent normal-map particle refraction and authored HDR bloom when VFX is enabled. `report.hdr_bloom` records activation. HDR accumulates into a float scene target before resolving to the existing eight-bit output; refraction samples an ordered background snapshot. SDR refraction retains eight-bit accumulation. Both paths run on the renderer worker. Generated pixel contracts and the native Gojo route verify the independent model. Exact engine refraction and bloom fidelity remain unverified; see [the particle guide](scene-particles.md).

The generated fixture uses a static background, two instances of a shared bird animation and a character animation with different durations:

```powershell
cargo run --locked --example native-viewer -- --image-fixture
```

Run the actual target with explicit local roots:

```powershell
cargo run --locked --example native-viewer -- --scene 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg' --engine-assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'
```

This target route discovers objects `28, 32, 22, 72, 89, 77, 86` in authored order, including the cropped wire layer. The viewer supplies no object IDs. [Static crop metadata](image-layers.md#static-import-cropping) preserves the import-adjusted placement. It starts playback after the first complete composition is presented. The viewer's stdout readiness event includes the package hash, loaded IDs, unique animated assets, applied precise-blur scales, cloud settings and input identities, and omissions. It retains the previous frame when a source is unavailable.

The existing line-delimited JSON protocol adds these commands:

| Command | Result |
| --- | --- |
| `{"id":1,"command":"image_at","time_ms":1500}` | Pause, seek and reply only after that exact complete sample is presented |
| `{"id":2,"command":"play_images"}` | Resume the library clock and host sampling at the initial monitor refresh rate (60 Hz fallback, 240 Hz maximum) |
| `{"id":3,"command":"pause_images"}` | Freeze the clock; an outstanding GPU request may still finish |
| `{"id":4,"command":"freeze_images"}` | Hold the last presented sample and GPU frame; ignore newer in-flight results |
| `inspect`, `resize`, `capture`, `stop` | Existing operations; captures refer to the completed composition |

Settle outstanding work with an acknowledged `image_at` before measuring paused idle. Play/pause commands are rejected while seek or resize replies are pending. `image-playback.json` records source identity and omissions. `image-timeline.json` records bounded presentation-call observations. Exact nanosecond times are decimal strings to preserve values beyond JSON's integer range. Capture readback blocks only the diagnostic command.

## Verification

The required gate runs public WARP contracts and a separate native combined fixture. Paired `-ScenePath` and `-EngineAssets` also run the target image playback controller when native checks are enabled. Use `-Software -SkipNative` without a desktop and report native playback as skipped.

`scripts/verify-image-playback.ps1` drives the owned viewer through controlled frame-duration/page/loop checkpoints, pause, resize and a 4.2-second live interval with no capture readback. The fixture's independent pixel oracle checks every pixel, including static crop metadata, shared instances and layer order. Fixture placement avoids exact texel-boundary ties at the tested resolutions. Target checks assert seven layers, authored IDs including the wires, common-time sprite indices and explicit omissions; its captures have no independent pixel oracle.

Public contracts check clock freeze/resume, shared region identities, static reuse, GPU pixels, looping, resized output, retained samples after joined shutdown, invisible selections and asynchronous sprite failures. The package guard prevents replacement during playback and releases on shutdown. Target captures at zero and 28.8 seconds use the same sprite frame indices. Their pixels must change from cloud motion, and rewinding must reproduce the original output. See [cloud verification](cloud-motion.md#renderer-verification) for the scope and reference limits.

The [hardware/native gate](../artifacts/20261010-163305-dd2dd065/manifest.json) and [WARP/headless gate](../artifacts/20261010-163353-49ef08e8/manifest.json) passed on 10 October 2026 with Rust 1.91, Windows x64, a Ryzen 9 9950X3D and an NVIDIA RTX 5080 for hardware. All 55 tests, formatting, Clippy, Rustdoc, dependency boundaries and consumer scenarios passed. WARP explicitly skipped native and proprietary target checks. Doctor accepts the matching diagnostic receipt and still reports full scene support unavailable.

The [target report](../artifacts/20261010-163305-dd2dd065/native-target-images/report.json) records seven layers in authored order, controlled sprite indices, pause, resize and owned-process exit. The native fixture and target observed 126 and 125 live presentation calls respectively during their defined 4.2-second intervals. [Target captures](../artifacts/20261010-163305-dd2dd065/native-target-images/image-01500.png) show the wires and both sprite sequences at different times. They do not establish visual fidelity against Wallpaper Engine, smooth display cadence, GPU duration or host responsiveness. The live target interval crosses multiple sprite page boundaries; it does not cover its complete 28.8-second loop. Controlled checkpoints cover the loop boundary separately. Ignored evidence may be absent in other checkouts; rerun there.

Automatic discovery passed the [hardware/native gate](../artifacts/20261010-170230-b7b71e44/manifest.json) and [WARP/headless gate](../artifacts/20261010-170653-342d95b2/manifest.json) with 59 tests. The [target report](../artifacts/20261010-170230-b7b71e44/native-target-images/report.json) verifies the same seven authored IDs, shared sprite assets, exact seek indices, pause, resize, live clock advancement and owned exit through `open_scene_images`. The viewer supplies no target IDs. Those receipts predate the [precise-blur profile](precise-blur.md); see its guide for the current procedure and evidence.

Authored precise blur passed the [hardware/native gate](../artifacts/20261010-180918-03edde55/manifest.json) and [WARP/headless gate](../artifacts/20261010-180904-2fb68d95/manifest.json) with 62 tests. The [target report](../artifacts/20261010-180918-03edde55/native-target-images/report.json) checks the wire scale `[0.32, 0.32]` and first bird scale `[2, 2]` alongside the existing playback controls. See [precise-blur evidence and limits](precise-blur.md#renderer-verification) for the reference comparison and earlier paging timeout.

Authored cloud motion passed the [hardware/native gate](../artifacts/20261010-192159-423f0c4a/manifest.json) and [WARP/headless gate](../artifacts/20261010-192142-e7d47694/manifest.json) with 65 tests. The target controller verifies settings for both instances, masked/noise inputs, changed cloud pixels at repeated sprite indices and exact rewind. [Cloud-motion evidence](cloud-motion.md) records the approximate reference model and filtering limits. Full scene fidelity remains unverified.

## Remaining target features

The report names omitted parallax, camera/color/postprocessing settings and other omitted content. The wire layer and first bird use authored unmasked precise blur. Background and clouds use authored cloud motion, including the painted cloud mask. Nonzero animated crop offsets remain unsupported. The particle layer, procedural light shafts, full-scene composition effect and muted audio remain omitted. Point placement sampling and straight-alpha SDR blending are the current image profile. Full scene and MVP acceptance still require those visible features and comparison with Wallpaper Engine, followed by resource and host-impact measurements.
