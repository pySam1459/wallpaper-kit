# Blank renderer infrastructure

This milestone provides a Rust library, two real consumers, and repeatable blank-output verification. It does not load the Workshop scene. The planned scene architecture remains in the [specification](spec/scene-renderer.md).

## Ownership and dependencies

The library owns a worker, its Direct3D 11 immediate context, one pending request, and the latest immutable GPU frame. Rendering happens on demand. Resize requests coalesce before execution; obsolete results are discarded. Completed frames retain their own texture, shared NT handle, and adapter reference, so consumers can keep them across renderer resize and shutdown.

`Renderer::new(Size, DeviceMode)` returns the renderer and a capacity-one wakeup receiver. `resize` returns a request identity. `try_frame` retrieves the latest completed `Arc<Frame>` without blocking on rendering or a GPU wait. Notifications are hints to inspect that latest state, not a queue of frames. `shutdown` joins the worker and belongs outside a latency-critical UI operation.

`Size::new` validates physical dimensions and a 64 MiB texture limit. `DeviceMode::Hardware` has no automatic software fallback; `Software` explicitly selects WARP. The blank output is opaque RGBA8 UNORM with channel values `[16, 24, 32, 255]`.

`Frame::create_consumer_device` creates a separate device on the same adapter. `Frame::copy_to` imports the shared texture and uses a zero-timeout keyed-mutex acquisition for a GPU copy. It is unsafe because the caller must serialize its context and supply matching resources. The native example demonstrates those requirements. Producer and consumer immediate contexts are never shared.

`Frame::readback` is an explicit blocking diagnostic operation. It is absent from ordinary native presentation. No raw shared handle is exposed as a public API.

The core has only the Windows bindings dependency. Winit, PNG encoding, and JSON reporting are development dependencies used by the examples. `scripts/check-boundaries.ps1` traverses production/build dependency edges and rejects host packages entering the core. Its negative fixture changes the real Winit edge to a production edge and confirms rejection.

This is an experimental API. It allocates a new immutable texture per completed request, not a reusable animation frame pool. Consumers can increase memory use by retaining old frames. Global residency budgets, continuous animation, device-loss recovery, and production host adapters remain future work.

## Example consumers

From the repository root:

```powershell
cargo run --locked --example native-viewer
cargo run --locked --example headless
```

The native viewer owns a Winit window, a consumer device, and a swap chain. It imports the library frame through the public API, copies on the GPU, and presents without routine CPU readback. It uses a waiting event loop after output becomes ready. It retries pending startup/resize work with a short timer; no continuous background rendering loop exists. Window redraw events can legitimately present an existing frame.

The headless runner waits for actual output, checks every pixel through diagnostic readback, captures initial and resized images, checks idle behavior and invalid dimensions, retains an old frame across resize, shuts down, and checks retained frames again. Its nonaligned 317 × 193 capture exercises row-pitch handling. The public-contract test additionally exercises request bursts, undrained notifications, and concurrent readers after shutdown.

Both examples accept `--output <empty-directory>` and `--software`. Without an output argument, they create a unique directory beneath `artifacts/`. Existing nonempty output directories are rejected.

## Repeatable verification

Requires PowerShell 7 and the Windows MSVC build toolchain:

```powershell
pwsh -File ./scripts/verify.ps1
```

The command runs format and Clippy checks, the public-contract test, example builds, the dependency checker and its negative fixture, the headless consumer, and an owned native-viewer scenario. Native operations wait for replies and verify process identity. The scenario captures initial pixels, inspects idle behavior, resizes, captures again, requests stop, and verifies process exit. Failure cleanup targets only the process handle created for that run.

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

Resize replies after a frame matching the actual window size has been presented. Only one controlled resize is pending at a time. Capture names are PNG basenames inside that run's directory. Unsupported commands and invalid input return an explicit error. There is no load, playback, pointer-driving, or scene-capture operation yet.

`scripts/verify-native.ps1` is the reusable launch/control/capture/cleanup operation. It uses redirected streams and its own process handle instead of attaching to a named executable or global endpoint.

## Doctor

```powershell
pwsh -File ./.codex/skills/control-scene-renderer/scripts/doctor.ps1
```

Doctor remains read-only. It reports blank-runtime readiness only when a successful native verification receipt from this checkout matches the current runtime source set, source hashes and both executable hashes. A build without a matching receipt is unverified. Exit codes are 0 for ready, 2 for blocked/unverified, and 1 for a doctor failure.

Supplying `-ScenePath` checks file existence and reports scene verification as blocked even when the blank runtime has been verified. It does not open or modify the package. A prior receipt describes its recorded build/configuration; it is not proof of general hardware support.

## Current limitations

Only Windows x64 has been exercised. The prototype requires Direct3D feature level 11.0. Blank pixels and one resize do not establish scene animation, alpha composition, visual effects, host scrolling/input latency, minimized lifecycle, GPUI/Electron/WebView2/CEF compatibility, or production frame-pool behavior. Those need their own fixtures and real consumers.
