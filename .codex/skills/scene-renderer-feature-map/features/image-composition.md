# Image composition

## Status

Implemented and verified through generated public tests and the native composition scenario on Windows x64. Final hardware/native and WARP gate receipts are recorded in [the guide](../../../../docs/image-layers.md). Full Workshop fidelity remains unverified.

## Consumer outcome

A host submits an immutable composition of cropped image layers. Completed GPU frames preserve placement, order, opacity and brightness across resize. Unique page identities share uploads. A diagnostic selected-object snapshot retains supported authored transforms and names omitted features and objects.

## How to reach it

Read [image composition](../../../../docs/image-layers.md) for the public API, blocking preparation boundaries and limits. Run `pwsh -File ./scripts/verify.ps1`; use `-Software -SkipNative` for headless WARP. Native `--layers-fixture` prepares generated sources before window creation. Paired scene/engine roots add real target snapshots through `texture-checks`.

## Drive and observe

Check generated four-layer composition pixels, crops, cardinal rotations, overlap, clipping, opacity/brightness, source reuse and retained output. A reversed-order submission must fail the oracle. Native captures check initial and resized back buffers, then idle, motion switching and owned exit. Public tests check malformed geometry/appearance before decode, selected-object omissions and source admission before corrupt oversized PNG decoding.

## Evidence and limits

Inspect `textures/composition.png`, target composition captures, `textures/report.json` and `native-layers/report.json` plus captures/transcript. Source payloads are capped separately from output and total process memory. Cardinal pixel fixtures avoid exact rasterization ties. Target snapshots omit effects, particles, parallax and general settings; nonzero crop offsets fail. Capture correctness does not establish animation, scene fidelity, GPU duration or consuming-app latency. Doctor with `-ScenePath` still rejects full scene readiness.

## Selected image playback

[Selected image playback](../../../../docs/image-playback.md) combines static layers and unique animated sources under a library clock. The native `--image-fixture` route checks independent generated pixels. Paired scene/engine roots also drive six supported target images through `scripts/verify-image-playback.ps1`. Inspect source identity and omissions, exact seek replies, live observations, resize, paused idle and owned-process exit. Full scene fidelity and host impact remain unverified.
