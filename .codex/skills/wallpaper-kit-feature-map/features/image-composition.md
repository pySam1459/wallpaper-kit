# Image composition

## Status

[Periodic Pulse](../../../../docs/pulse.md) is verified through headless Chrome
on 25 October 2026. Image and composition passes support additive/hard-light
tint, alpha, painted opacity, noise, bounds, power and continuous timing. Both
WARP and RTX 5080 pass 18 installed-shader cases within two channel values,
including transparency, HDR, composition, resize and fault checks. Public GPU
contracts independently check the signal, blending, masks, grayscale tint,
rewind, invalid profiles and consumer policy. Actual `2374964197` and
`2288914800` pass Playwright/DevTools controls and cleanup. Nine Pulse passes
across nine folders are added while prior objects/effect chains and 65/65 loading
are retained. Three audio-driven Pulse profiles remain omitted; zero power and
other blend modes are outside the checked profile. Reduced-flashing mode now
skips Pulse before asset preparation. The required software/headless gate passes
with native windows skipped. Engine runtime agreement and display cadence remain
unverified.

[Composition layers](../../../../docs/composition-layers.md) are verified through
headless Chrome on 25 October 2026. Scene discovery applies supported passes to
earlier draws at the authored position. Full-screen captures use output
resolution; adjustable captures retain authored source resolution. WARP and
hardware pass nine independent capture/installed-shader cases: native-size
comparisons allow two channel values, resized comparisons three. Fault checks
reject reversed/missing passes, frozen motion and corrupt pixels. Public GPU
contracts cover two points, opacity, resize, HDR, limits and target release.
Actual Gojo and adjustable scenes pass Playwright/DevTools controls and cleanup.
The collection retains 65/65 loading and previous objects/effect inputs, adding
14 passes at six points across five folders. All 17 points are recognized;
11 have no supported effect profiles and omissions remain explicit. The required
software/headless gate passes with native windows skipped. Complete engine
runtime agreement, display cadence and host responsiveness remain unverified.

[Property bindings](../../../../docs/property-bindings.md) are verified through
headless Chrome on 25 October 2026. Direct checkbox/combo visibility and text/color
links resolve during preparation. Eight checked clock/calendar profiles retain
script options, including nested user links, without a JavaScript runtime.
Nine WARP/hardware reference cases compare installed-script formatting and exact
literal-text pixels, including blurred tint, failed updates, rewind, bounds and
cleanup. Actual `3299011444` passes Playwright/DevTools controls and property
replacement. The collection retains 65/65 loading and all previous objects/effect
inputs; saved-default binding notices fall from 106 in 13 folders to 29 in seven.
Recognized bindings can still belong to hidden or unrenderable optional objects.
Arbitrary scripts, media text, localized profiles and native cadence remain open.

[Ordered image effects](../../../../docs/image-effect-chains.md) are verified on
25 October 2026. Scene snapshots/playback apply up to 16 supported passes in
authored order; unsupported optional passes remain named omissions. WARP and
hardware browser comparisons pass eight cases within two channel code values,
including reversed-order/missing-pass fault checks. Public GPU tests cover
16-pass pixels, HDR, scratch accounting/release and retained frames. Actual
`3565747436` passes Playwright/DevTools controls and cleanup. The collection adds
70 passes across 21 folders and retains 65/65 loading. Native display timing and
complete engine/runtime fidelity remain unverified. This supersedes the older
single-effect limits in the historical receipts below.

[Periodic Shake](../../../../docs/shake.md) is implemented in SDR and HDR,
including painted direction, time offset and effect opacity. The independent
GPU pixel tests and [engine probe comparison](../../../../artifacts/shake-reference-v8/comparison.json)
passed. Actual Gojo [native playback](../../../../artifacts/shake-native-gojo-final/report.json)
and [T3 addon lifecycle](../../../../artifacts/shake-native-gojo-final/t3-native.json)
passed. Nondefault curves and noise/audio remain omitted.
The [actual Gojo comparison](../../../../artifacts/gojo-engine-comparison-20261012/comparison.json)
observed both viewers at 1280×720 with the full character and animated particles.
Foreground placement and particle arrangement differ; clocks, seeds and omitted
camera/parallax prevent exact frame agreement. Complete scene agreement and T3
application appearance remain unverified. Use the generator/verifier and native
motion procedure in the guide; presentation counts do not prove displayed cadence.

Basic masked UV Spin and planar single-wave Water Waves are implemented. The
[Panda Store native check](../../../../artifacts/panda-fidelity/native-verified/report.json)
verified six Spin and two Water Waves image objects, changed fixed-time pixels,
exact rewind, 241 completed frames over an eight-second live interval, paused idle,
1280×720 and 3840×2160 output, and cleanup. The
[hardware/native gate](../../../../artifacts/20261012-151513-229484f0/manifest.json)
passed the generated mask/cardinal-rotation GPU test and existing contracts.
The final [software gate](../../../../artifacts/20261012-153210-1618f7ac/manifest.json)
also passed invalid-profile and mixed-effect-order regressions; native presentation
was skipped in that run. That receipt predates ordered-pass support.
See [image playback](../../../../docs/image-playback.md) for the command and limits.
Wallpaper Engine's Panda Store window was observed; grain was visible in its
artwork too. Exact effect phase/amplitude and complete scene fidelity remain
unverified. The T3 visual comparison was interrupted by Computer Use's Escape stop.

Scene discovery now honors enabled authored background clear color. Generated
SDR/HDR public GPU tests verify transparent/uncovered pixels, alpha blending,
resize and retained samples in the [12 October hardware/native
gate](../../../../artifacts/20261012-143444-c036087f/manifest.json). Selected-image
and manual compositions retain the diagnostic background. The engine reference
comparison was interrupted; exact engine alpha/color agreement is unverified.
See the [collection fidelity priorities](../../../../docs/workshop-fidelity.md).

Implemented and verified through generated public tests and the native composition scenario on Windows x64. The latest [hardware/native gate](../../../../artifacts/20261010-192159-423f0c4a/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-192142-e7d47694/manifest.json) passed 65 tests, including mixed Gaussian blur, precise blur and cloud motion. Procedures and limits are recorded in [the guide](../../../../docs/image-layers.md). Full Workshop fidelity remains unverified.

## Consumer outcome

A host submits an immutable composition of cropped image layers with optional masked Gaussian blur, unmasked precise blur or masked cloud motion at an explicit time. Scene preparation also supplies ordered supported effect passes. Completed GPU frames preserve placement, order, opacity, brightness and filtering across resize. Unique page identities share uploads. A diagnostic selected-object snapshot retains supported authored transforms, checked precise-blur scales and time-zero cloud motion, and names omitted features and objects.

## How to reach it

Read [image composition](../../../../docs/image-layers.md) for the public API, blocking preparation boundaries and limits. Run `pwsh -File ./scripts/verify.ps1`; use `-Software -SkipNative` for headless WARP. Native `--layers-fixture` prepares generated sources before window creation. Paired scene/engine roots add real target snapshots through `texture-checks`.

## Drive and observe

Check generated six-layer composition pixels, masked RGBA and horizontal BC3 Gaussian blur, precise blur, cloud motion, crops, cardinal rotations, overlap, clipping, opacity/brightness, source reuse and retained output. The Gaussian oracle uses independent direct two-dimensional convolution; the precise oracle expands paired samples into integer CPU taps with horizontal byte quantization. Check scratch payload accounting and release when switching to motion. Reversed-order and missing-blur submissions must fail the oracle. Native captures check initial and resized back buffers, then idle, motion switching and owned exit. Public tests check cloud masks, crops, time progression, frozen-motion rejection, malformed geometry/appearance and unsupported effect overrides before decode, operation replacement, selected-object omissions and source admission before corrupt oversized PNG decoding.

## Evidence and limits

[Planar Water Ripple](../../../../docs/water-ripple.md) passes an installed-shader
WebGL oracle on WARP and hardware across 15 cases, within two channel code values.
Checks cover normal combination, strength, animation/scroll, aspect, masks,
transparency, default/overridden textures and rewind. Actual `3137398793` passes
Playwright/DevTools controls and cleanup. The required software/headless gate
passes with native windows skipped. Five layers across five folders are added;
previous effects remain intact. Perspective and specular remain omitted. Engine/runtime agreement and displayed cadence remain open.

[UV Foliage Sway](../../../../docs/foliage-sway.md) is verified through generated
GPU tests and headless Chrome. The installed-shader WebGL oracle matches WARP
and hardware output within two channel code values, including masks, noise,
direction/aspect, power, timing, transparency and rewind. The actual
`3309706653` browser route passes Playwright/DevTools controls and cleanup.
The required software/headless gate passes with native-window checks skipped.
Eight layers across seven folders are added while previous effects are retained.
Vertex mode and legacy profiles remain omitted.
Complete engine/runtime fidelity and displayed cadence remain unverified.

Inspect `textures/composition.png`, target composition captures, `textures/report.json` and `native-layers/report.json` plus captures/transcript. Source payloads are capped separately from output and total process memory. Cardinal pixel fixtures avoid exact rasterization ties. Target snapshots apply the default unmasked precise-blur and cloud-motion profiles and omit other effects, particles, parallax and general settings; static crop metadata preserves authored placement; nonzero animated crop offsets fail. Capture correctness does not establish animation, scene fidelity, GPU duration or consuming-app latency. Doctor with `-ScenePath` still rejects full scene readiness.

## Selected image playback

[Selected image playback](../../../../docs/image-playback.md) combines static layers and unique animated sources under a library clock. The native `--image-fixture` route checks independent generated pixels. Paired scene/engine roots also drive all seven target image layers, including the cropped wires through `scripts/verify-image-playback.ps1`. Inspect source identity, applied precise-blur scales, cloud settings and input identities, and omissions, exact seek replies, live observations, resize, paused idle and owned-process exit. The viewer uses `ImagePlayback::open_scene_images` to discover authored image IDs from either package. Unsupported visible image profiles fail in strict playback; tolerant scene discovery reports optional omissions. Supported composition points process earlier layers as described above. [Gaussian blur](../../../../docs/gaussian-blur.md), [precise blur](../../../../docs/precise-blur.md) and [cloud motion](../../../../docs/cloud-motion.md) define their APIs, bounds and evidence. The target controller checks cloud pixel changes across repeated sprite indices and exact time-zero rewind. Precise-blur masks and nondefault kernels, full scene fidelity and host impact remain unverified.
