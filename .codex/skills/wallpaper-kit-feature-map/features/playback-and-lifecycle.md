# Playback and lifecycle

## Status

Blank resize, retained-frame lifetime and shutdown are implemented through the [blank infrastructure](blank-renderer.md). [Diagnostic motion](diagnostic-motion.md) adds consumer-owned play/pause and controlled stepping. [Bounded frame-pool backpressure](../../../../docs/frame-pool.md) is implemented and verified. [Sprite extraction](sprite-extraction.md) adds deterministic frame selection and loop boundaries on an asset worker/tool; [background preparation](../../../../docs/sprite-worker.md) handles async asset work, seek generations and joined shutdown. [Selected image playback](../../../../docs/image-playback.md) adds a library-owned clock, host-scheduled native playback, exact seeks and joined source shutdown. Device-loss recovery and full scene lifecycle remain planned.

## Consumer outcome

The host can control playback and output size, consume completed frames with defined ownership, and close the renderer without stranded workers or resources. Background work does not require the host UI to wait.

## How to reach it

Use the example viewer's public library path. Fixture controls and pause semantics are in [motion verification](../../../../docs/motion-verification.md#native-controls). Use `Frame::sequence()` for publication identity and `Renderer::try_output_stats()` for output ownership and saturation. The required gate tests release-triggered progress and shutdown while saturated. Selected image controls are documented in [image playback](../../../../docs/image-playback.md#native-controls). Full scene lifecycle controls remain planned.

[Consumer settings](../../../../docs/playback-settings.md) expose nearest/linear image sampling, animation permission, visual-effects preparation and conservative particle suppression for reduced flashing. Use `open_scene_images_with_settings` or `open_with_settings`. The viewer accepts `--linear-images`, `--no-animation`, `--no-vfx` and `--reduce-flashing` on its image routes. `freeze_images` retains the last presented sample/frame and ignores later completions. Native frozen resize and resume use the same public API; the library cannot infer the host's displayed frame or automatically detect arbitrary flashes.

## Drive and observe

Observe animation, pause it, then resume it. Check rendered behavior as well as reported state. Resize during playback and verify dimensions and composition. Observe frame delivery under a slow consumer, bounded resource use, and the documented drop or backpressure behavior. Release frames according to the API contract. Close the viewer and establish that its owned process and workers exit.

## Evidence and limits

Retain actions, timestamps, captures, state observations, and lifecycle diagnostics. Check GPU allocations over repeated load/close cycles once measurement exists. Define pause semantics and allowed in-flight work before setting expectations. Process exit alone does not prove resource behavior in a long-lived host.

Initialization policy and freeze-last-frame passed the [11 October 2026 hardware/native gate](../../../../artifacts/20261011-182801-11018f1f/manifest.json) with 75 tests. Native generated and seven-image target controllers verified retained pixels/clock, resize using the frozen composition, resume and owned-process exit. The [Gojo settings run](../../../../artifacts/settings-c4bea099508247efa32583c0e85626ee/report.json) also verified the static/no-VFX and linear/reduced-flashing viewer flags with two base layers, stable still captures and cleanup. See [settings verification](../../../../docs/playback-settings.md#verification) for scope and limits.

Bounded frame exchange was verified on 2026-10-10 by the [hardware/native gate](../../../../artifacts/20261010-010846-85f196dd/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-011028-f819e26f/manifest.json). Native presentation passed in the hardware run and was explicitly skipped in the WARP run. See [frame exchange verification](../../../../docs/frame-pool.md#verification) for allocation, saturation, queued-copy and timing evidence. These receipts establish diagnostic output behavior, not scene compatibility.
