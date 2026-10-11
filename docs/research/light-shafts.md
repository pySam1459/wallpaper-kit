# Light-shafts reference groundwork

Workshop scene `3780699842` still omits its light-shaft shape. This note records the first reference probes for an independent implementation. No library rendering operation or complete ray equation exists yet.

## Checked inputs

The read-only inspector resolved 63 assets, 12 textures and 12 objects from the unchanged package on 10 October 2026. The [inspection report](../../artifacts/inspection-48052-1791666429081/report.json) records identities and diagnostics. The package SHA-256 remains `551f74eeca12b2f760bff5a9c79f9f28c623c1e1d31c3642719b67633ff91f14`.

Object 103 is a `quad` shape at the center of the 3840 × 2160 canvas, with no explicit size. Its single visible effect uses `DIRECTDRAW=1`, `RAYMODE=2`, `RAYCORNER=1` and `RENDERING=1`. Parameter metadata identifies those last three values as corner rays, top-right corner and gradient rendering. The material has one `effects/lightshafts` pass, normal blending, disabled depth test/write and no culling. The effect is version 1 with replacement key `lightshafts`. Its `EffectPerspectiveUV` gizmo binds `p0` through `p3` to `point0` through `point3`.

| Setting | Authored value |
| --- | --- |
| `point0` | `[-1.36286, 1.35948]` |
| `point1` | `[-0.57913, -0.17720]` |
| `point2` | `[1.41440, -0.06330]` |
| `point3` | `[1.48965, 1.00833]` |
| `colorastart` | `[1, 1, 1]` |
| `colorend` | `[0.43529412, 0.88627451, 1]` |
| `colorwexponent`, `colorwintensity` | `0.2`, `0.53` |
| `noiseamount`, `noisescale` | `0`, `0` |
| `rayfeather`, `rayradius` | `[0, 0]`, `0.12` |
| `rayscale`, `raysmoothness`, `rayspeed` | `[0.17, 0.43]`, `1`, `0.12` |

There are no texture overrides or authored mask. Metadata defaults identify `util/noise` in slot 1 and `gradient/gradient_iridescent` in slot 2. Zero noise distortion does not remove the base noise dependency. Gradient alpha, masks and image opacity must be treated as separate questions.

The package and installed light-shaft shader identities match:

| Asset | SHA-256 |
| --- | --- |
| `shaders/effects/lightshafts.vert` | `fc65a0011fcae963679c3411c017da95c3c5ddcc9808b0777196f0334547e471` |
| `shaders/effects/lightshafts.frag` | `3e03ff559af5a8eb7951d1be590c48976a78f2e159bcc6778a9e48923d48d940` |

The [official effect description](https://docs.wallpaperengine.io/en/scene/effects/effect/lightshafts.html) explains standalone layers, directions, gradient rendering and editable corners. It does not supply numerical ray, noise, perspective or time equations. Only JSON definitions, shader parameter declarations/comments and black-box output were inspected. Vendor shaders were copied into ignored local reference projects for Wallpaper Engine execution, never translated into renderer code.

## Repeatable probes

Run the generator from the checkout:

```powershell
pwsh -File scripts/prepare-light-shafts-reference.ps1 -EngineAssets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'
```

The script creates 17 local projects under a unique ignored `artifacts/light-shafts-reference-*` directory. They isolate default full-canvas size, explicit size, inset and trapezoid geometry, constant noise channels/values, gradient coordinates/RGB/alpha, mask strength and motion. `probes.json` records each combination, constant, texture and vendor input hash. The isolated target project uses authored settings on a 512 × 288 canvas with other layers absent.

Open each project's `project.json` with Wallpaper Engine's [window playback command](https://help.wallpaperengine.io/en/functionality/cli.html), using one unique `-playInWindow` name and the project's exact canvas dimensions. Observe the selected window through Computer Use, retain the original JPEG screenshot and record its window/screenshot identities and capture start/end milliseconds. Captures use case names from `probes.json`. Record two captures for `motion` and `target`, with `-first` and `-later` suffixes, three to sixty seconds apart. Close only the named reference window and check that it disappears.

`capture-identities.json` is an array with `name`, absolute `path`, `start_ms` and `end_ms` for each capture. Window and screenshot identity fields can also be retained. Determine the client origin from the observed screenshot before measuring:

```powershell
pwsh -File scripts/measure-light-shafts-reference.ps1 -Probes '<generated directory>/probes.json' -CaptureIdentities '<generated directory>/capture-identities.json' -ClientX 1 -ClientY 31
```

The shown client origin belongs to the recorded Windows captures, not to every desktop. The tool validates constants, combinations, canvas and capture completeness, hashes generated inputs and screenshots, and writes `comparison.json`. Static checks compare independently varied controls. The target motion check requires at least 64 pixels to change by more than six colour codes. The synthetic motion result remains separate and can be inconclusive. A frozen target capture must fail.

## Observations and limits

The [reference receipt](../../artifacts/light-shafts-reference-8fae18af/comparison.json) passes the component relations at a four-code tolerance. The [frozen-frame control](../../artifacts/light-shafts-reference-8fae18af/frozen-check.json) confirms rejection of a repeated target frame. A second generator run reproduced all 239 inputs with equivalent JSON values and identical binary bytes. The owned window was closed and its absence was checked. All recorded captures use original JPEG encoding; no third-party inputs or screenshots belong in source commits.

The [required hardware/native gate](../../artifacts/20261010-221727-54aea351/manifest.json) passed with 65 tests, including read-only target inspection and existing seven-image target playback. This verifies the current renderer's regressions, not light-shaft rendering. Ignored receipts may be absent in another checkout.

- A size-free identity quad covers the canvas. A 128 × 128 quad and the perspective inset `[0.25, 0.25]` to `[0.75, 0.75]` produce matching output on a 256 × 256 canvas. This establishes those bounds, not general projective interpolation.
- Red-only and white noise match. Green-only, blue-only and zero noise produce dark output. At a bright interior sample, white noise gives 127 and half noise gives 3. The base ray signal depends on red noise and responds nonlinearly. A complete noise equation remains unresolved.
- Halving gradient RGB halves output. A vertical gradient ramp produces approximately half output, consistent with sampling the gradient's middle row. Transparent gradient alpha does not change output in this direct-draw profile. Horizontal gradient coordinates remain unresolved.
- An explicit zero mask suppresses output, even though the editor metadata exposes masks only for non-direct drawing. A half mask produces approximately one-quarter output, consistent with squared red mask strength. This optional profile is absent from the target.
- Target captures 29.0595 seconds apart contain 26,199 changed interior pixels above six codes, with a maximum change of 42 codes. This establishes visible animation in the isolated reference. It does not determine the clock phase or numerical speed mapping.
- The synthetic ramp-noise motion captures 18.1605 seconds apart are dark and have no interior change above six codes. That probe is inconclusive and needs stronger excitation before it can check motion equations.

The next rendering work must resolve ray shaping, multi-sample noise coordinates, smoothness/exponent/radius semantics, horizontal gradient coordinates, perspective interpolation and blend/alpha behavior over a nonblack background. Controlled temporal probes must then predict a later capture from an initial fitted phase. Implement the smallest checked profile only after those observations supply an independent pixel oracle. Full-resolution target composition and native playback remain later evidence.
