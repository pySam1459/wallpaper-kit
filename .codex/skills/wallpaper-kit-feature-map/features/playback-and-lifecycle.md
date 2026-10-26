# Playback and lifecycle

## Status

Verified 26 October: `ImagePlayback::freeze` retains the exact displayed
sample while source workers release extra sprite windows/atlases and close
idle decoders. `Renderer::trim_memory` asynchronously drops GPU caches and
unleased spare outputs after pending work, retaining latest/consumer-held
frames and still-resize content. Later draws rebuild resources; resumed sources
may temporarily return no sample. Public sprite/video/GPU contracts and the
[software gate](../../../../artifacts/20261026-000945-f4bc23d0/manifest.json)
pass. The browser example uses blur/visibility callbacks and explicit
`setActive` for host policy, resumes only requested playback, and preserves
manual pause. Hardware sprite/video callback tests and still resize pass;
actual desktop focus delivery and production adapters are unverified. Read the
[idle procedure](../../../../docs/idle-performance-2026-10-25.md) for costs and
reproduction. Warm `pause` remains available for short interruptions.

The 12 October cadence update preserves source-rate video decoding and paces the
native viewer against the initial monitor refresh rate. Generated 60 fps live
video and existing 30 fps seek/loop/embedded contracts passed. Panda and Tram
native checks passed exact rewind, paused idle and owned exit; their T3 addon
checks retain three leases and verify hidden pause and still/remount. See
[cadence evidence and limits](../../../../docs/smooth-playback.md).

Live video selections now retain one common sample time until all sources finish
decoding. Seek, pause and freeze invalidate pending work; polling while paused
allows a still to settle. The generated advancing-clock video regression failed
with zero samples before the fix and passed in the
[software gate](../../../../artifacts/20261012-154837-bcad709d/manifest.json).
Native presentation was skipped in that gate. The separate
[Japanese Summer Tram native check](../../../../artifacts/tram-playback/native/report.json)
verified actual 1920×1080 pixels, exact rewind, paused idle, 182 presentation-call
observations over eight seconds and owned-process exit on the RTX 5080.
The [T3 addon check](../../../../artifacts/tram-playback/t3-native-receipt.json)
passed live GPU exports, release/resume, hidden pause and still/remount. Current
T3 UI presentation and exact Wallpaper Engine agreement were not compared.

Blank resize, retained-frame lifetime and shutdown are implemented through the [blank infrastructure](blank-renderer.md). [Diagnostic motion](diagnostic-motion.md) adds consumer-owned play/pause and controlled stepping. [Bounded frame-pool backpressure](../../../../docs/frame-pool.md) is implemented and verified. [Sprite extraction](sprite-extraction.md) adds deterministic frame selection and loop boundaries on an asset worker/tool; [background preparation](../../../../docs/sprite-worker.md) handles async asset work, seek generations and joined shutdown. [Selected image playback](../../../../docs/image-playback.md) adds a library-owned clock, host-scheduled native playback, exact seeks and joined source shutdown. Device-loss recovery and full scene lifecycle remain planned.

## Consumer outcome

The host can control playback and output size, consume completed frames with defined ownership, and close the renderer without stranded workers or resources. Background work does not require the host UI to wait.

## How to reach it

The [Workshop/video route](../../../../docs/workshop-support.md) additionally
uses `ImagePlayback::open_video` or tolerant scene discovery for embedded video.
Installed FFmpeg tools are required. A demand-driven worker retains one ready
video frame and one queued request, reuses forward decoding, and cancels/reaps
its owned codec on joined shutdown. The native viewer accepts `--video` and the
same seek/play/pause/freeze controls. The generated video contract passed in
the [12 October hardware/native gate](../../../../artifacts/20261012-140148-627c94b7/manifest.json).
Separate actual [standalone-video](../../../../artifacts/native-video-final/report.json)
and [embedded-video](../../../../artifacts/native-embedded-final/report.json)
native checks passed seeking, exact rewind pixels, pause, resize, eight seconds
of live sampling and owned-process exit. Presentation-call observations do not
prove smooth display cadence or responsiveness in a consuming host.

Use the example viewer's public library path. Fixture controls and pause semantics are in [motion verification](../../../../docs/motion-verification.md#native-controls). Use `Frame::sequence()` for publication identity and `Renderer::try_output_stats()` for output ownership and saturation. The required gate tests release-triggered progress and shutdown while saturated. Selected image controls are documented in [image playback](../../../../docs/image-playback.md#native-controls). Full scene lifecycle controls remain planned.

[Consumer settings](../../../../docs/playback-settings.md) expose nearest/linear image sampling, animation permission, visual-effects preparation and conservative particle suppression for reduced flashing. Use `open_scene_images_with_settings` or `open_with_settings`. The viewer accepts `--linear-images`, `--no-animation`, `--no-vfx` and `--reduce-flashing` on its image routes. `freeze_images` retains the last presented sample/frame and ignores later completions. Native frozen resize and resume use the same public API; the library cannot infer the host's displayed frame or automatically detect arbitrary flashes.

## Drive and observe

Observe animation, pause it, then resume it. Check rendered behavior as well as reported state. Resize during playback and verify dimensions and composition. Observe frame delivery under a slow consumer, bounded resource use, and the documented drop or backpressure behavior. Release frames according to the API contract. Close the viewer and establish that its owned process and workers exit.

## Evidence and limits

Retain actions, timestamps, captures, state observations, and lifecycle diagnostics. Check GPU allocations over repeated load/close cycles once measurement exists. Define pause semantics and allowed in-flight work before setting expectations. Process exit alone does not prove resource behavior in a long-lived host.

Initialization policy and freeze-last-frame passed the [11 October 2026 hardware/native gate](../../../../artifacts/20261011-182801-11018f1f/manifest.json) with 75 tests. Native generated and seven-image target controllers verified retained pixels/clock, resize using the frozen composition, resume and owned-process exit. The [Gojo settings run](../../../../artifacts/settings-c4bea099508247efa32583c0e85626ee/report.json) also verified the static/no-VFX and linear/reduced-flashing viewer flags with two base layers, stable still captures and cleanup. See [settings verification](../../../../docs/playback-settings.md#verification) for scope and limits.

Bounded frame exchange was verified on 2026-10-10 by the [hardware/native gate](../../../../artifacts/20261010-010846-85f196dd/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-011028-f819e26f/manifest.json). Native presentation passed in the hardware run and was explicitly skipped in the WARP run. See [frame exchange verification](../../../../docs/frame-pool.md#verification) for allocation, saturation, queued-copy and timing evidence. These receipts establish diagnostic output behavior, not scene compatibility.
