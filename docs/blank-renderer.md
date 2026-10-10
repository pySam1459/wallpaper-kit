# Blank renderer infrastructure

This milestone provides a Rust library, real consumers, and repeatable blank-output verification. [Diagnostic motion verification](motion-verification.md) extends those consumers with controlled-time animation and separate live telemetry. Neither loads the Workshop scene. The planned scene architecture remains in the [specification](spec/scene-renderer.md).

## Ownership and dependencies

The library owns a worker, its Direct3D 11 immediate context, one pending request, and the latest immutable GPU frame. Rendering happens on demand. Resize requests coalesce before execution; obsolete results are discarded. Completed frames lease resources from a [bounded output pool](frame-pool.md), retaining their texture, shared NT handle and adapter reference across renderer resize and shutdown.

`Renderer::new(Size, DeviceMode)` returns the renderer and a capacity-one wakeup receiver. `resize` returns a request identity. `try_frame` retrieves the latest completed `Arc<Frame>` without blocking on rendering or a GPU wait. Notifications are hints to inspect that latest state, not a queue of frames. `shutdown` joins the worker and belongs outside a latency-critical UI operation.

`Size::new` validates physical dimensions and a 64 MiB texture limit. `DeviceMode::Hardware` has no automatic software fallback; `Software` explicitly selects WARP. The blank output is opaque RGBA8 UNORM with channel values `[16, 24, 32, 255]`.

`Frame::create_consumer_device` creates a separate device on the same adapter. `Frame::copy_to` imports the shared texture and uses a zero-timeout keyed-mutex acquisition for a GPU copy. It is unsafe because the caller must serialize its context and supply matching resources. The native example demonstrates those requirements. Producer and consumer immediate contexts are never shared.

`Frame::readback` is an explicit blocking diagnostic operation. It is absent from ordinary native presentation. No raw shared handle is exposed as a public API.

The core uses Windows bindings, JSON parsing and hashing for scene inspection, plus PNG/LZ4 for [TEX decoding](texture-pages.md). Winit remains an example dependency. `scripts/check-boundaries.ps1` traverses production/build dependency edges and rejects host packages entering the core. Its negative fixture changes the real Winit edge to a production edge and confirms rejection.

This is an experimental API. It reuses at most three output textures, pauses when retained frames exhaust the pool and resumes after release. `Frame::sequence()` identifies each accepted publication; `Renderer::try_output_stats()` exposes nonblocking output counters. Scene residency budgets, production animation, device-loss recovery and production host adapters remain future work.

## Example consumers

From the repository root:

```powershell
cargo run --locked --example native-viewer
cargo run --locked --example headless
```

The native viewer owns a Winit window, a consumer device, and a swap chain. It imports the library frame through the public API, copies on the GPU, and presents without routine CPU readback. Blank output uses a waiting event loop after becoming ready. It retries pending startup/resize work with a short timer. Explicit fixture playback adds a consumer-owned timer; pause stops scheduling. Window redraw events can legitimately present an existing frame.

The headless runner waits for actual output, checks every pixel through diagnostic readback, captures initial and resized images, checks idle behavior and invalid dimensions, retains old frames across resize, shuts down, and checks retained frames again. It verifies allocation reuse across 32 same-size requests, saturation at three outputs and progress after release. Its nonaligned 317 × 193 capture exercises row-pitch handling. Public-contract tests additionally exercise request bursts, undrained notifications, repeated motion/resize and concurrent readers after shutdown.

Both examples accept `--output <empty-directory>` and `--software`. Without an output argument, they create a unique directory beneath `artifacts/`. Existing nonempty output directories are rejected.

## Repeatable verification

Requires PowerShell 7 and the Windows MSVC build toolchain:

```powershell
pwsh -File ./scripts/verify.ps1
```

The command runs format and Clippy checks, tests, Rustdoc, example builds, the dependency checker and its negative fixture, headless blank/motion/page/crop consumers, and owned native-viewer scenarios. Each run retains the dependency result in `boundaries.json`. Native operations wait for replies and verify process identity. Separate instances check blank, full-page and cropped capture/idle/resize, then motion capture/play/pause and process exit. Failure cleanup targets only the process handle created for that run.

```powershell
pwsh -File ./scripts/verify.ps1 -Software -SkipNative
```

This route explicitly records native verification as skipped. It supports CI without an interactive desktop. The GitHub Actions workflow is configured but has not been executed remotely.

Each run retains a manifest, source and executable hashes, working-tree status and revision where available, hardware/driver/OS/toolchain information, action transcripts, stderr, PNGs, and consumer reports. The native report contains one measured idle interval with process CPU time, working set and handle count. This is a diagnostic baseline that may include startup/driver activity, not a benchmark or proof of negligible host impact. GPU frame time and input latency are not measured.

Native captures come from the consumer swap-chain back buffer after the real GPU import/copy path. Their uniform RGBA values are checked against the blank contract. They do not capture final desktop composition, window chrome, or overlays. Headless captures come from the producer frame. Both capture paths use readback and must be excluded from normal rendering-cost claims.

## Controller protocol

The native viewer reads one JSON object per line on stdin and writes replies on stdout. Human diagnostics belong on stderr. After startup it emits `event: ready` with process and frame identity. Commands have a caller-supplied `id`, which is echoed in their reply:

```json
{"id":1,"command":"inspect"}
{"id":2,"command":"resize","width":317,"height":193}
{"id":3,"command":"capture","name":"resized.png"}
{"id":4,"command":"stop"}
```

Resize replies after a frame matching the actual window size has been presented. Only one controlled frame command is pending at a time. Capture names are PNG basenames inside that run's directory. Unsupported commands and invalid input return an explicit error. The [motion controls](motion-verification.md#native-controls) add diagnostic fixture playback; load, pointer-driving and scene capture remain unavailable.

`scripts/verify-native.ps1` is the reusable launch/control/capture/cleanup operation. It uses redirected streams and its own process handle instead of attaching to a named executable or global endpoint.

## Doctor

```powershell
pwsh -File ./.codex/skills/control-scene-renderer/scripts/doctor.ps1
```

Doctor remains read-only. It reports diagnostic readiness only when a successful blank/motion/page/crop/native receipt from this checkout matches the current runtime source set, source hashes and five executable hashes. A build without a matching receipt is unverified. Exit codes are 0 for ready, 2 for blocked/unverified, and 1 for a doctor failure.

Supplying `-ScenePath` checks file existence and reports scene verification as blocked even when the blank runtime has been verified. It does not open or modify the package. A prior receipt describes its recorded build/configuration; it is not proof of general hardware support.

## Current limitations

Only Windows x64 has been exercised. The prototype requires Direct3D feature level 11.0. The bounded diagnostic output pool does not establish scene animation, alpha composition, visual effects, host scrolling/input latency, minimized lifecycle or GPUI/Electron/WebView2/CEF compatibility. Those need their own fixtures and real consumers.
