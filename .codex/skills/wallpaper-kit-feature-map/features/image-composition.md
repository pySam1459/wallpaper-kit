# Image composition

## Status

[Periodic Shake](../../../../docs/shake.md) is implemented in SDR and HDR,
including painted direction, time offset and effect opacity. The independent
GPU pixel tests and [engine probe comparison](../../../../artifacts/shake-reference-v8/comparison.json)
passed. Actual Gojo [native playback](../../../../artifacts/shake-native-gojo-final/report.json)
and [T3 addon lifecycle](../../../../artifacts/shake-native-gojo-final/t3-native.json)
passed. Nondefault curves, noise/audio and ordered chains remain omitted.
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
was skipped in that run. Mixed layers preserve previously supported effects and
explicitly omit the additional motion until ordered chains are implemented.
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

A host submits an immutable composition of cropped image layers with optional masked Gaussian blur, unmasked precise blur or masked cloud motion at an explicit time. Completed GPU frames preserve placement, order, opacity, brightness and filtering across resize. Unique page identities share uploads. A diagnostic selected-object snapshot retains supported authored transforms, checked precise-blur scales and time-zero cloud motion, and names omitted features and objects.

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
previous effects remain intact. Perspective, specular and additional passes
remain omitted. Engine/runtime agreement and displayed cadence remain open.

[UV Foliage Sway](../../../../docs/foliage-sway.md) is verified through generated
GPU tests and headless Chrome. The installed-shader WebGL oracle matches WARP
and hardware output within two channel code values, including masks, noise,
direction/aspect, power, timing, transparency and rewind. The actual
`3309706653` browser route passes Playwright/DevTools controls and cleanup.
The required software/headless gate passes with native-window checks skipped.
Eight layers across seven folders are added while previous effects are retained.
Vertex mode, legacy profiles and additional effect passes remain omitted.
Complete engine/runtime fidelity and displayed cadence remain unverified.

Inspect `textures/composition.png`, target composition captures, `textures/report.json` and `native-layers/report.json` plus captures/transcript. Source payloads are capped separately from output and total process memory. Cardinal pixel fixtures avoid exact rasterization ties. Target snapshots apply the default unmasked precise-blur and cloud-motion profiles and omit other effects, particles, parallax and general settings; static crop metadata preserves authored placement; nonzero animated crop offsets fail. Capture correctness does not establish animation, scene fidelity, GPU duration or consuming-app latency. Doctor with `-ScenePath` still rejects full scene readiness.

## Selected image playback

[Selected image playback](../../../../docs/image-playback.md) combines static layers and unique animated sources under a library clock. The native `--image-fixture` route checks independent generated pixels. Paired scene/engine roots also drive all seven target image layers, including the cropped wires through `scripts/verify-image-playback.ps1`. Inspect source identity, applied precise-blur scales, cloud settings and input identities, and omissions, exact seek replies, live observations, resize, paused idle and owned-process exit. The viewer uses `ImagePlayback::open_scene_images` to discover authored image IDs from either package. Unsupported visible image profiles fail; non-image objects and composition points are reported as omitted. [Gaussian blur](../../../../docs/gaussian-blur.md), [precise blur](../../../../docs/precise-blur.md) and [cloud motion](../../../../docs/cloud-motion.md) define their APIs, bounds and evidence. The target controller checks cloud pixel changes across repeated sprite indices and exact time-zero rewind. Precise-blur masks and nondefault kernels, full scene fidelity and host impact remain unverified.
