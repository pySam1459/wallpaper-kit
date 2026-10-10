# Precise image blur

`PreciseBlur` implements the unmasked, default 13-texel SDR blur profile needed by the target scene. `ImagePlayback::open_scene_images` and selected snapshots load its authored scales. The wires use `[0.32, 0.32]`; the first bird uses `[2, 2]`. Loading depends on checked effect data and shader identities, without special object IDs or artwork paths in rendering code.

## Operation and supported profile

```rust
let blur = PreciseBlur::new([0.32, 2.0])?;
let layer = ImageLayer::new(region, center, size, angle, opacity, brightness)?
    .with_precise_blur(blur);
```

Scale controls the distance between samples, not Gaussian standard deviation. Each axis accepts finite values from zero to two source texels. Zero bypasses that axis; both zero allocate no scratch. This fixed kernel uses three pairs of bilinear samples and a centre sample per axis. Samples clamp to the source crop, including extracted sprites and BC3 crops. Filtering uses straight RGBA code values, including RGB under zero alpha. The horizontal pass rounds to eight-bit RGBA before vertical filtering. Placement, brightness, opacity and ordinary layer blending follow filtering.

The coefficients and paired sampling come from the MIT-licensed [glsl-fast-gaussian-blur](https://github.com/Experience-Monks/glsl-fast-gaussian-blur/blob/master/13.glsl). [Third-party notices](../THIRD-PARTY-NOTICES.md) preserve its license. Our HLSL implements D3D11 loads, crop boundaries and pass bindings. Proprietary shader code is not part of the renderer.

`GaussianBlur` remains a separate premultiplied operation with variable sigma. Applying either operation replaces the previous blur. `ImageLayer::blur` returns the Gaussian descriptor; `precise_blur` returns the fixed-kernel descriptor. Both reuse the existing scratch allocation and resource admission. See [Gaussian bounds](gaussian-blur.md#bounds-and-ownership).

The scene loader accepts the canonical `effects/blurprecise/effect.json` profile with version `1`, replacement key `blur_precise`, two normal material passes and one `rgba_backbuffer` target. The vertical pass binds the horizontal result at slot zero and the original input at slot one. Material and instance combinations must preserve `VERTICAL` and `ENABLEMASK`, default `KERNEL=0`, `BLURALPHA=1`, and no active `MASK`. Optional null texture slots carry no mask. The horizontal scale comes from the first pass and the vertical scale from the second; omitted values default to one.

Unknown combinations, nonfinite or excessive scales, masks, extra passes, changed bindings, unknown render fields, HDR scenes and multiple blur instances on one layer fail with asset or object context. Invisible effects are skipped. Other effect profiles remain explicitly omitted by partial image loading. `ImagePlaybackReport::precise_blurs` records applied object IDs and scales.

The loader hashes the shader identity inputs before enabling the profile. These hashes were measured from the installed reference and target package, rather than inferred from names:

| Logical asset | SHA-256 |
| --- | --- |
| `shaders/effects/blur_precise_gaussian.frag` | `7d613ded8d61b1ca68b109d84a8f7f68d2901c65268af7e0ea7628adeb07b16f` |
| `shaders/effects/blur_precise_gaussian.vert` | `4900e7a84bc5c1dc6e9d3b95a1c1862638a3ced4408a9bd17962be55149a10ec` |
| `shaders/common_blur.h` | `b594e72584c3f9d2dff8ecb74cf71ac8e5b3e0ab7e57f9fd0dd1d964269cbc2e` |

Revised algorithms with familiar filenames are rejected. Hashing identifies compatibility inputs; their code is neither compiled nor translated by our renderer. New identities require another compatibility check.

## Reference procedure

`scripts/prepare-blur-reference.ps1 -EngineAssets <installed assets>` writes a unique ignored directory containing generated vertical impulses, scene metadata and local reference-only engine inputs. `-HiddenColor` puts white RGB under zero alpha to distinguish straight from premultiplied filtering. No original assets or installation files are modified.

Use Wallpaper Engine's documented [window playback command](https://help.wallpaperengine.io/en/functionality/cli.html) with that generated `project.json`, a unique `-playInWindow` name and `-width 585 -height 585`. Activate that window before capture and wait until the probe scene is visible. Record the returned window and screenshot identities. Save a Computer Use capture as JPEG, keeping its original encoding. Close only the named reference window with `-control closeWallpaper -location <name>`.

Run `scripts/measure-blur-reference.ps1 -Probes <probes.json> -Capture <capture.jpg> -ClientX <left> -ClientY <top> -PixelScale 3`. Client coordinates come from the observed capture, excluding its frame/title bar; do not guess them for another machine. The tool checks all three default-kernel scales against the independently evaluated impulse response, hashes its inputs and writes `comparison.json`. It allows four code values because desktop captures are JPEG and final scaling is linear. It deliberately does not establish exact coefficients from compressed pixels.

The [opaque reference](../artifacts/blur-reference-91a63598/comparison.json) and [transparent-color reference](../artifacts/blur-reference-711a8b04/comparison.json) passed this approximate check on 10 October 2026. The comparison supports default-kernel spacing and straight-RGBA filtering. The 7-texel and 3-texel probes are observations only; those variants remain unsupported. Mask semantics, edge behavior against Wallpaper Engine, arbitrary effect chains, HDR and full scene fidelity remain unverified.

## Renderer verification

Run `pwsh -File ./scripts/verify.ps1`, supplying both target roots to exercise real package loading. Public WARP contracts vary equal and unequal scales, axis bypasses, maximum scale, transparent colors, R8, RGBA, BC3 crops and operation replacement. Invalid overrides fail before corrupt texture decoding. The generated five-layer composition combines Gaussian and precise blur through the native consumer, with resize, retained frames and scratch release. Its precise oracle expands paired samples into integer CPU convolution taps and quantizes its horizontal byte array, independently of the HLSL loads.

The target native controller asserts both applied IDs and authored scales, exact sprite selections, paused idle, resize, clock advancement and owned exit. Complete target appearance still needs comparison against Wallpaper Engine, including the remaining effects. Controlled pixels and approximate reference probes do not establish display cadence, GPU duration or host responsiveness.

The final [hardware/native gate](../artifacts/20261010-180918-03edde55/manifest.json) and [WARP/headless gate](../artifacts/20261010-180904-2fb68d95/manifest.json) passed on Windows x64 with Rust 1.91.0 and 62 tests. Hardware used an NVIDIA GeForce RTX 5080. The [native composition report](../artifacts/20261010-180918-03edde55/native-layers/report.json) checks the mixed five-layer fixture; the [target playback report](../artifacts/20261010-180918-03edde55/native-target-images/report.json) and [source report](../artifacts/20261010-180918-03edde55/native-target-images/image-playback.json) record the seven image layers and both applied blur scales. WARP skipped native checks. These ignored receipts may be absent in another checkout; rerun the procedures there.

An [initial hardware run](../artifacts/20261010-174014-be0ce014/failure.json) timed out while requesting an ordinary character-paging frame, before the target blur snapshots. Two subsequent texture runs passed that stage. The timeout now reports the requested/latest frame and pool counters; the original cause remains unconfirmed. This change does not claim to fix intermittent paging stalls.
