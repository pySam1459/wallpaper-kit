# Consumer playback settings

Initialize selected images or scene discovery with `PlaybackSettings`:

```rust
use wallpaper_kit::{ImagePlayback, ImageQuality, PlaybackSettings};

let settings = PlaybackSettings {
    allow_unsupported_features: true,
    image_quality: ImageQuality::Linear,
    animation: true,
    visual_effects: true,
    reduce_flashing: true,
};
let mut playback = ImagePlayback::open_scene_images_with_settings(&source, settings)?;
playback.play();
```

Opening hashes, validates and prepares assets, so run it on an asset thread or before creating the window. `open_with_settings` takes the same policy plus an explicit list of image IDs. Existing `open` and `open_scene_images` calls keep their defaults. Settings are fixed for a prepared playback; changing effect policy requires preparing a replacement off the UI thread. The report records the applied settings and omissions.

| Setting | Default | Behavior |
| --- | --- | --- |
| `allow_unsupported_features` | `false` | Scene discovery draws base imagery and reports unsupported optional features instead of failing. Corrupt primary assets still fail. Explicit selected-image APIs remain strict. The native viewer enables this on real scene routes. See [Workshop support](workshop-support.md). |
| `image_quality` | `Nearest` | `Linear` uses bilinear interpolation for smoother image scaling, including placed blur output. Sampling clamps inside each atlas crop. Cloud effects and particles already use bilinear sampling. |
| `animation` | `true` | Allows `play` to advance the clock. With `false`, `play` does nothing and time starts at zero. Explicit seeks can select a different still. |
| `visual_effects` | `true` | With `false`, image effects and particles are omitted before loading their assets. Base images and sprites remain. |
| `reduce_flashing` | `false` | With `true`, all particles and Pulse effects are omitted before loading their assets. Other image effects and sprite imagery remain. |

Output resolution remains the `Size` supplied to `Renderer::new` or `resize`. Linear sampling does not add mipmaps, HDR, or source detail. Manual `ImageLayer` consumers can use `with_image_quality` too.

Reduced flashing skips particles and the known Pulse brightness/alpha effect.
It is not an automatic brightness or flash detector and cannot guarantee the
absence of flashes in arbitrary content. For a stationary background, disable
animation or freeze the displayed sample. See [Pulse verification](pulse.md).

## Output resolution

For 3840 × 2160 output, construct `Renderer::new(Size::new(3840, 2160)?, device_mode)` or call `renderer.resize(Size::new(3840, 2160)?)`. Decoding retains the selected source's base-mip pixels; output resolution is independent of the sampling-quality setting. There is no fixed 720p intermediate. Increasing output size cannot restore detail absent from the source.

`Size` rejects zero dimensions, dimensions above 16,384 and RGBA8 outputs above 64 MiB. A 4K output uses 33,177,600 bytes per texture, about 31.6 MiB; the renderer retains at most three output textures. Source pages, effect scratch, host copies and driver memory have separate costs. 7680 × 4320 exceeds the current output budget and is rejected.

`tests/texture_contract.rs::native_4k_png_preserves_single_pixel_detail_through_decode_render_and_resize` decodes a generated 4K PNG with one-pixel stripes, verifies every GPU pixel, resizes to 1080p and back, checks retained pixels and confirms source-upload reuse and the output-pool bound. It passed on Windows WARP in the [2026-10-11 full hardware/native gate](../artifacts/20261011-230208-0f57e32d/manifest.json).

The local Codex adapter requests 4K native frames and sizes its canvas to the imported frame. `node integrations/codex-demo/verify-resolution.cjs` verifies actual shared frames, default 1080p compatibility, invalid-size rejection, pool backpressure and 4K decoded video in the real app. Its [receipt](../artifacts/codex-re/4k/report.json) passed on the RTX 5080. The WebRTC demo uses lossy encoding; this does not establish pixel-exact transport, sustained display cadence or host-performance budgets.

## Freeze when a session starts

Keep the `ImagePlaybackSample` associated with the GPU frame the host actually presents. On session start:

```rust
playback.freeze(&last_presented_sample);
```

Stop scheduling samples and render requests, retain the completed `Arc<Frame>` or copied host texture, and ignore newer in-flight results. The host can blur that retained texture without advancing the wallpaper. `freeze` retains one sample and pauses at its exact time; `try_sample` returns the same composition immediately while frozen, without decoding or waiting. Outstanding asset/GPU work can settle, but the host must not display its results.

`pause` only stops the clock at the current time and allows pending work to finish. `freeze` chooses the sample already displayed, which may be older. The library cannot infer which frame the host presented. Resume with `play`, or select another paused time with `seek`. Disabling animation at initialization also prevents `play` from resuming a frozen sample.

Resize can redraw the frozen composition at a new output size without changing its clock. Normal sampling, rendering, frame ownership and joined shutdown follow [image playback](image-playback.md).

## Example viewer

Use any combination of these flags with `--scene` or `--image-fixture`:

```powershell
cargo run --locked --example native-viewer -- --image-fixture --linear-images --no-animation --no-vfx --reduce-flashing
```

The line-delimited command `{"id":1,"command":"freeze_images"}` holds the last presented frame, discards later completions and stops scheduling. `play_images` resumes; `image_at` selects a new still. Frozen resize renders the retained composition. The report includes settings and the `frozen` state.

## Verification

`tests/image_contract.rs` checks default compatibility, disabled particle/effect assets, still sprite selection, bilinear pixels and atlas boundaries, retained GPU pixels after newer work, frozen sample identity and resume time. `scripts/verify-image-playback.ps1` checks native freeze after live playback, identical retained pixels/clock, frozen resize, resume and owned-process cleanup for both its generated fixture and the seven-image Workshop target.

Run `pwsh -File ./scripts/verify.ps1` for the full gate. Native captures establish output behavior; they do not measure display cadence, GPU duration, host responsiveness or automatic flash detection. The [isolated Codex demo](research/codex-wallpaper-demo.md#settings-and-wallpaper-selection) exposes these options in Settings > Appearance, including freeze during chats and wallpaper selection. Its shared native adapter accepts the options, while the existing T3 settings page still exposes only its original background toggle.

The [Windows hardware/native gate](../artifacts/20261011-182801-11018f1f/manifest.json) passed on 11 October 2026 with all 75 tests, formatting, Clippy, Rustdoc, dependency checks and native consumers. Both the [generated fixture](../artifacts/20261011-182801-11018f1f/native-image-playback/report.json) and [seven-image target](../artifacts/20261011-182801-11018f1f/native-target-images/report.json) passed freeze, frozen resize and resume.

The [Gojo settings report](../artifacts/settings-c4bea099508247efa32583c0e85626ee/report.json) verifies native `--no-animation --no-vfx` and `--linear-images --reduce-flashing` configurations on an NVIDIA RTX 5080 at 640×360. Each retained the two base image layers, skipped particle preparation, produced identical still captures 500 ms apart and respected animation permission. Both owned processes exited. This is consumer output evidence, not a reference fidelity or safety assessment. Evidence is ignored and may be absent in another checkout.
