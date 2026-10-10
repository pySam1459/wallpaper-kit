# Gaussian image blur

`GaussianBlur` is a reusable source-space operation on `ImageLayer`, independent of scene names, object IDs and host frameworks. It supports separate horizontal and vertical standard deviations and an optional red-channel texture mask. Wallpaper Engine's `blurprecise` profile is not mapped to this operation yet. Its `scale` units, kernel and filtering conventions still require reference validation; scene playback continues to report that effect as omitted.

## Public consumer path

```rust
let blur = GaussianBlur::new([0.8, 1.4], Some(mask_region))?;
let layer = ImageLayer::new(source_region, center, size, angle, opacity, brightness)?
    .with_blur(blur);
let composition = Arc::new(ImageComposition::new(canvas, vec![layer])?);
renderer.render_images(composition)?;
```

Each standard deviation is finite and between zero and eight cropped source texels. An axis with zero deviation performs no filtering. Both zero bypass the effect and allocate no scratch texture. The normalized discrete Gaussian kernel extends to `ceil(3 * sigma)` on each side, at most 49 taps per axis. Samples clamp to the source crop, including crops within BC3 blocks; surrounding atlas pixels do not enter the filter.

Filtering uses premultiplied RGBA code values. The optional mask mixes the original and blurred values before converting back to straight alpha for composition. Zero mask red keeps the original; one applies the full blur. Mask coordinates cover its own crop, independently of source dimensions, using point sampling. Mask alpha is ignored. Brightness, opacity, placement and layer blending apply afterward. These SDR conventions are explicit and still need comparison with Wallpaper Engine for compatibility.

The renderer worker executes a horizontal pass into an RGBA16F texture, followed by vertical filtering and mask mixing while drawing the placed layer into the output. The original source, horizontal result and mask are distinct bindings. Inputs are unbound before scratch reuse. Source decoding, uploads and shader compilation remain outside host sampling/presentation.

## Bounds and ownership

Masks join the composition's existing 256 MiB unique source-page budget; shared page Arcs count once. One scratch texture is reused across layers and repeated requests. Its dimensions are the maximum crop width and height among active blurred layers, capped at 128 MiB of RGBA16F payload. A combination of narrow but perpendicular large crops may therefore fail admission even if each would fit separately. Construction returns `InvalidBlur` before GPU allocation for an oversized target.

`OutputStats::effect_texture_bytes` reports scratch separately from source and output textures. Replacement releases the completed preceding target before allocating another. A request without active blur releases scratch; shutdown releases it too. These payload bounds exclude driver overhead, consumer allocations and caller-retained CPU data. They are not scene-wide performance acceptance.

## Verification

Run `pwsh -File ./scripts/verify.ps1`. Use `-Software -SkipNative` without a desktop and report native checks as skipped. The generated composition in `texture-checks` and native `--layers-fixture` includes masked RGBA blur and unmasked horizontal BC3 blur. Its independent oracle performs a direct two-dimensional f64 convolution, rather than reproducing the GPU's separable intermediate. Pixel checks allow two RGB code values of rounding tolerance and require opaque output alpha.

Public contracts vary both radii, masks, axis bypasses, zero blur, a subnormal radius and the maximum radius. They also exercise crop boundaries, hidden RGB under zero alpha, placement, opacity, brightness, resizing, shared sources, retained output and early scratch admission. Missing blur and reversed composition must fail the oracle. The native controller checks initial/resized consumer-device pixels, 288 scratch bytes for the fixture, scratch release on switching to motion, idle behavior and owned-process exit.

GPU pixels establish this operation's defined behavior. They do not establish Wallpaper Engine precise-blur fidelity, cloud motion, display cadence, GPU duration or consuming-app responsiveness. The scene loader does not silently substitute this operation for an unverified effect profile.

The [hardware/native gate](../artifacts/20261010-170230-b7b71e44/manifest.json) and [WARP/headless gate](../artifacts/20261010-170653-342d95b2/manifest.json) passed on 10 October 2026. Both ran 59 tests, formatting, Clippy, Rustdoc, dependency boundaries and generated consumers. Hardware used Windows x64, Rust 1.91, a Ryzen 9 9950X3D and NVIDIA RTX 5080. The [native fixture report](../artifacts/20261010-170230-b7b71e44/native-layers/report.json) records correct pixels, matched-frame scratch observations of 288 and zero bytes, resize, idle and owned exit. WARP explicitly skipped native and proprietary target scenarios. Ignored evidence may be absent in another checkout; rerun the gate there.
