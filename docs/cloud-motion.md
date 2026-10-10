# Cloud motion

`CloudMotion` displaces an image using repeating red-channel noise and an optional painted mask. Selected snapshots and `ImagePlayback::open_scene_images` load the checked profile from scene data. The target background uses amount `0.01`, granularity `[0.6, 0.1]` and speed `0.013`. The clouds use amount `0.02`, granularity `[0.5, 0.1]` and speed `0.025`, with their authored mask. Both use direction `1.5707964` radians and `util/perlin_256` noise. Rendering contains no target object IDs.

## Public API and sampling

```rust
let settings = CloudMotionSettings::new(0.02, direction, [0.5, 0.1], 0.025)?;
let motion = Arc::new(CloudMotion::new(settings, noise_region, Some(mask_region)));
let layer = ImageLayer::new(source, center, size, angle, opacity, brightness)?
	.with_cloud_motion(motion, Duration::from_millis(1500));
```

Settings require finite values. Amount accepts `0..=0.2`, overall granularity `0.5..=4`, horizontal granularity `0.1..=1`, and speed `0..=0.1` noise periods per second. Direction is normalized to `0..2π`. Zero speed freezes noise progression. Zero amount leaves bilinear source sampling active. Applying cloud motion, Gaussian blur or precise blur replaces the layer's previous effect. Effect chains remain unsupported.

For source coordinates `uv` with rows increasing downward, the implemented model is:

```text
noise_uv = uv * [overall * horizontal, overall] + [time_seconds * speed, 0]
noise = 2 * bilinear_repeat(noise.red, noise_uv) - 1
mask = bilinear_clamp(mask.red, uv), or 1 without a mask
sample_uv = uv + [-sin(direction), cos(direction)] * amount * noise * mask²
color = bilinear_clamp(source.straight_rgba, sample_uv)
```

Noise repeats within its crop, including bilinear interpolation across the seam. Source and mask clamp within their crops. Red controls noise and mask values regardless of alpha. Direction zero samples downward. Direction `π/2` samples leftward. Placement, brightness, opacity and layer blending follow displacement. Time advances only the horizontal noise coordinate, independently of granularity and displacement direction.

`CloudMotion` retains immutable texture regions. Constructing it or assigning a time performs no decoding, upload, timer creation or pixel allocation. Noise and mask pages join the existing 256 MiB unique-source admission and share GPU uploads by page identity. Cloud motion uses one pixel pass and no scratch target. Shader compilation and drawing remain on the renderer worker.

Playback decodes static noise and masks during blocking preparation. Each `try_sample` uses the same clock time for cloud motion and sprites without asset I/O. `ImagePlaybackReport::cloud_motions` records settings and logical texture identities by object ID. Diagnostic `image_snapshot` applies cloud motion at time zero; its frame index selects sprites only.

## Checked scene profile

The loader accepts canonical `effects/cloudmotion/effect.json`, version `2`, replacement key `cloudmotion`, and one material-only pass. The material must have one `effects/cloudmotion` shader pass with normal blending, disabled depth and no culling. There are no intermediate targets or alternate bindings.

An instance can override the five `ui_editor_properties_*` constants for amount, direction, granularity, horizontal granularity and speed. Defaults are `0.1`, `π/2`, `[2, 0.5]` and `0.02`. Texture slot zero preserves the source, slot one supplies an optional mask, and slot two overrides the default noise. An explicit `MASK` combination must match mask presence. Unknown combinations, constants, render fields, invalid ranges, replacement source bindings, multiple supported effects and HDR fail with context. Invisible effects are skipped.

Loaded noise and masks must be static, single-page TEX assets without padding. Their base mip is decoded. Automatic mip selection and fidelity at other sampling footprints remain unverified. The public API also accepts checked cropped regions in RGBA8, R8 or BC3.

The profile requires these measured shader identities:

| Logical asset | SHA-256 |
| --- | --- |
| `shaders/effects/cloudmotion.vert` | `108e4ebd4198fda3842ee8d25b5fa3301df429c2c9bda87c2638b1c366a9116f` |
| `shaders/effects/cloudmotion.frag` | `9481749c00175d40cd7b5a0b9d85c65cdbd484967ed6f479a4d81924c493050f` |
| `shaders/common.h` | `acf56edaeca256e6d40b4fc29ababe1f9742197300c418589137911bc8f82ed1` |

Hashing identifies compatibility inputs. The renderer neither compiles nor translates proprietary shaders. Its HLSL comes from our own model, using the [official cloud effect description](https://docs.wallpaperengine.io/en/scene/effects/effect/cloudmotion.html), parameter metadata and generated black-box observations. The [shader variable reference](https://docs.wallpaperengine.io/en/scene/shader/variables.html) defines time in seconds. Changed identities require renewed compatibility evidence.

## Reference procedure and limits

`scripts/prepare-cloud-reference.ps1 -EngineAssets <installed assets> -Mode <mode>` creates a unique ignored directory with gradients, steps, noise, masks and a local scene. Modes `constant`, `mask`, `ramp` and `motion` isolate channel choice, direction, squared mask response, noise scaling, repeat and temporal progression. Engine input files are copied locally for reference execution only. The generator records their hashes and never modifies the installation or Workshop package.

Open the generated `project.json` with Wallpaper Engine's [window playback command](https://help.wallpaperengine.io/en/functionality/cli.html), a unique `-playInWindow` name and dimensions `512 × 768`. Activate the observed window, verify the scene, capture its window and screenshot identities, and save the original JPEG encoding. Client coordinates must come from that capture. For motion, take two captures seven to twenty seconds apart and retain each observation's `start_ms` and `end_ms` in `capture-identities.json`. Close only that named reference window.

`scripts/measure-cloud-reference.ps1 -Probes <probes.json> -Capture <capture.jpg> -ClientX <left> -ClientY <top>` evaluates the generated model and writes a hashed `comparison.json`. Motion also requires `-LaterCapture` and `-CaptureIdentities`. The tool validates the complete placement and measurement counts. Motion fits only the first capture's unknown engine phase, then predicts the second from elapsed time. Identical captures, short intervals and predictions that cannot reject frozen first-capture pixels fail.

The [constant reference](../artifacts/cloud-reference-d271185c/comparison.json), [mask reference](../artifacts/cloud-reference-342408d4/comparison.json), [ramp reference](../artifacts/cloud-reference-0c747a0d/comparison.json) and [motion reference](../artifacts/cloud-reference-b1e3f356/comparison.json) passed approximate checks on 10 October 2026. Gradient tolerance is six colour codes. The sharp-step tolerance is 0.05 source pixel. The observed maximum step residual was about 0.035 source pixel, or nine codes at a full-contrast edge; its initial six-code check failed. JPEG, rasterization and filtering limit this evidence, and the residual's exact cause remains unconfirmed. The model establishes squared mask displacement, including R8 and RGBA masks, without claiming an exact filtering match.

Motion captures were 8.07 seconds apart, with a maximum three-code error and a detected frozen-motion control. This checks relative speed and noise-coordinate progression, not exact Wallpaper Engine time-zero phase. Full target appearance, arbitrary chains, mip selection, smooth display cadence and host impact remain open.

## Renderer verification

The required gate runs independent CPU-versus-GPU composition checks on WARP and the native consumer. Public contracts vary time, direction, amount, granularity, noise crops, transparent mask RGB, mask absence and operation replacement. Reusing time-zero output must fail the moving oracle. Unsupported overrides, changed bindings and unverified shader identities fail before a corrupt PNG can decode.

The six-layer generated fixture mixes masked Gaussian blur, precise blur and cloud motion with RGBA8, R8 and BC3 sources. Its independent double-precision oracle evaluates inverse placement and bilinear crop sampling. The native route verifies initial and resized captures, retained frames, source sharing, idle and scratch release. The target controller asserts both authored cloud settings and mask/noise identities, then checks common-time seeks, play, pause, resize and owned exit. It compares time zero with 28.8 seconds, when both sprites return to frame zero and only cloud motion can change the current output. At least 64 pixels must differ. Seeking back to zero must reproduce the original pixels exactly. These are temporal checks; target captures have no independent full-scene pixel oracle.

The [hardware/native gate](../artifacts/20261010-192159-423f0c4a/manifest.json) and [WARP/headless gate](../artifacts/20261010-192142-e7d47694/manifest.json) passed with 65 tests on Windows x64 and Rust 1.91. Hardware used a Ryzen 9 9950X3D and NVIDIA RTX 5080. The [native composition report](../artifacts/20261010-192159-423f0c4a/native-layers/report.json) passed both capture sizes. The [target report](../artifacts/20261010-192159-423f0c4a/native-target-images/report.json) recorded 129,508 changed pixels across the sprite loop, exact time-zero rewind, 126 live presentation calls in its 4.2-second interval, and owned-process exit. The [source report](../artifacts/20261010-192159-423f0c4a/native-target-images/image-playback.json) records both cloud instances and explicit omissions. WARP skipped native and proprietary target checks. Ignored receipts may be absent in another checkout; rerun the procedures there.
