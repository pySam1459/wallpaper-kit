# Image composition

## Status

Implemented and verified through generated public tests and the native composition scenario on Windows x64. The latest [hardware/native gate](../../../../artifacts/20261010-192159-423f0c4a/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-192142-e7d47694/manifest.json) passed 65 tests, including mixed Gaussian blur, precise blur and cloud motion. Procedures and limits are recorded in [the guide](../../../../docs/image-layers.md). Full Workshop fidelity remains unverified.

## Consumer outcome

A host submits an immutable composition of cropped image layers with optional masked Gaussian blur, unmasked precise blur or masked cloud motion at an explicit time. Completed GPU frames preserve placement, order, opacity, brightness and filtering across resize. Unique page identities share uploads. A diagnostic selected-object snapshot retains supported authored transforms, checked precise-blur scales and time-zero cloud motion, and names omitted features and objects.

## How to reach it

Read [image composition](../../../../docs/image-layers.md) for the public API, blocking preparation boundaries and limits. Run `pwsh -File ./scripts/verify.ps1`; use `-Software -SkipNative` for headless WARP. Native `--layers-fixture` prepares generated sources before window creation. Paired scene/engine roots add real target snapshots through `texture-checks`.

## Drive and observe

Check generated six-layer composition pixels, masked RGBA and horizontal BC3 Gaussian blur, precise blur, cloud motion, crops, cardinal rotations, overlap, clipping, opacity/brightness, source reuse and retained output. The Gaussian oracle uses independent direct two-dimensional convolution; the precise oracle expands paired samples into integer CPU taps with horizontal byte quantization. Check scratch payload accounting and release when switching to motion. Reversed-order and missing-blur submissions must fail the oracle. Native captures check initial and resized back buffers, then idle, motion switching and owned exit. Public tests check cloud masks, crops, time progression, frozen-motion rejection, malformed geometry/appearance and unsupported effect overrides before decode, operation replacement, selected-object omissions and source admission before corrupt oversized PNG decoding.

## Evidence and limits

Inspect `textures/composition.png`, target composition captures, `textures/report.json` and `native-layers/report.json` plus captures/transcript. Source payloads are capped separately from output and total process memory. Cardinal pixel fixtures avoid exact rasterization ties. Target snapshots apply the default unmasked precise-blur and cloud-motion profiles and omit other effects, particles, parallax and general settings; static crop metadata preserves authored placement; nonzero animated crop offsets fail. Capture correctness does not establish animation, scene fidelity, GPU duration or consuming-app latency. Doctor with `-ScenePath` still rejects full scene readiness.

## Selected image playback

[Selected image playback](../../../../docs/image-playback.md) combines static layers and unique animated sources under a library clock. The native `--image-fixture` route checks independent generated pixels. Paired scene/engine roots also drive all seven target image layers, including the cropped wires through `scripts/verify-image-playback.ps1`. Inspect source identity, applied precise-blur scales, cloud settings and input identities, and omissions, exact seek replies, live observations, resize, paused idle and owned-process exit. The viewer uses `ImagePlayback::open_scene_images` to discover authored image IDs from either package. Unsupported visible image profiles fail; non-image objects and composition points are reported as omitted. [Gaussian blur](../../../../docs/gaussian-blur.md), [precise blur](../../../../docs/precise-blur.md) and [cloud motion](../../../../docs/cloud-motion.md) define their APIs, bounds and evidence. The target controller checks cloud pixel changes across repeated sprite indices and exact time-zero rewind. Precise-blur masks and nondefault kernels, full scene fidelity and host impact remain unverified.
