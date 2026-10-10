# Viewer control and evidence contract

## Doctor

`scripts/doctor.ps1` resolves the repository from its own location, so invoking the copy in a worktree inspects that worktree. It reads filesystem presence and tool versions without building, downloading, launching a viewer, or contacting an existing application.

The JSON report contains checkout and toolchain identity, manifest presence, scene input existence, readiness, blockers, verification scope, and the matching evidence receipt. Scene input is never opened or parsed. Diagnostic runtime readiness requires passed blank, motion, texture, composition and native scenarios with a source-set/hash and five executable-hash matches for this checkout. A build alone is insufficient.

Exit code `0` means the diagnostic runtime has matching verified evidence. Code `2` means blocked or unverified. Code `1` means doctor failed. Supplying a scene path remains blocked because full scene loading is unavailable. Headless-only evidence is identified separately and does not establish native readiness.

## Implemented diagnostic operations

`scripts/verify.ps1` builds and runs the consumers. `scripts/verify-native.ps1` launches a unique native instance, reads its readiness event, sends JSON inspect/resize/capture/stop commands over redirected streams, and verifies exit. It retains only its own process handle for cleanup. [The protocol](../../../../docs/blank-renderer.md#controller-protocol) documents actual commands.

Headless verification checks actual GPU output, idle behavior, resize, input bounds and retained-frame lifetime. Native captures check the imported/copied swap-chain back buffer. One measured native idle interval reports process CPU time, working set and handle count. GPU timings and host input latency remain unavailable.

The [TEX page procedure](../../../../docs/texture-pages.md) adds selected payload decoding, independent format/alpha/sampling checks, upload reuse, authored integer sprite crops and native full-page/crop PNG/LZ4 fixtures. Source-page memory is reported separately from the three-output pool. The separate [image composition procedure](../../../../docs/image-layers.md) adds ordered placed crops and diagnostic selected-object snapshots with named omissions. Neither route implements sprite animation or full scene behavior.

The [motion procedure](../../../../docs/motion-verification.md) adds fixed-time pixel checks, oracle fault cases, separate live readiness telemetry and native fixture step/play/pause controls. Contact sheets establish checkpoint appearance; raw live observations establish readiness or presentation-call cadence. They do not measure displayed frames.

## Remaining capabilities

The following table is the broader controller goal. Scene loading, production playback/input driving and full performance collection remain planned. Diagnostic launch, inspect, resize, capture, fixture play/pause/step and stop are implemented above.

| Operation | Observable result |
| --- | --- |
| Launch | A viewer from the selected checkout with a unique run ID and evidence directory |
| Inspect | Process identity, exact build, backend, scene identity, size, and playback state |
| Load | Load completion or an explicit error and unsupported-feature report |
| Drive | Resize, pause, resume, and input through the viewer's public consumer path |
| Capture | Actual rendered frames with timestamps and settings, plus temporal evidence where needed |
| Measure | Defined CPU, GPU, memory, and frame-timing measurements for a stated workload |
| Stop | The owned instance exits and evidence remains available |

The controller must identify the intended instance explicitly. Avoid a shared default instance, output directory, or endpoint that can attach to another run. Retain the process handle or identity established at launch, and never clean up by executable name alone. Cleanup covers only resources created by the run.

The viewer exercises the same API available to consuming applications. Controller operations must not bypass that contract by directly mutating private renderer state. Any deterministic timing mode must have explicit semantics, and its results must be distinguished from real-time performance measurements.

## Evidence per run

Each run needs its build identity, local source changes, scene identity, hardware and driver, backend, resolution, quality settings, and playback settings. Evidence includes the action sequence, observed results, captures, logs, metrics, and unavailable checks. Use separate directories per run.

Record whether captures used readback or overlays, since observation can affect timing. Define each metric's units, sampling interval, and measurement method. Separate normal playback measurements from diagnostic captures. Compare matching configurations and record variability rather than accepting one favorable sample.

For the Workshop MVP, keep a reference comparison against Wallpaper Engine. Account for differences in timing, random behavior, interaction, and settings. Do not derive expected pixels from the implementation under test.

## Readiness progression

First implement the viewer and its consumer API. Then implement launch, inspect, drive, capture, measure, and cleanup against that real viewer. Exercise the complete loop before updating readiness checks or marking feature procedures verified. Static checks can exist before runtime readiness; they establish a different kind of evidence.
