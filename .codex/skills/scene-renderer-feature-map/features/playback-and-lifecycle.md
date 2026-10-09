# Playback and lifecycle

## Status

Blank resize, retained-frame lifetime and shutdown are implemented through the [blank infrastructure](blank-renderer.md). [Diagnostic motion](diagnostic-motion.md) adds consumer-owned play/pause and controlled stepping. Animated frame-pool backpressure, device-loss recovery and scene playback remain planned.

## Consumer outcome

The host can control playback and output size, consume completed frames with defined ownership, and close the renderer without stranded workers or resources. Background work does not require the host UI to wait.

## How to reach it

Use the example viewer's public library path. Fixture controls and pause semantics are in [motion verification](../../../../docs/motion-verification.md#native-controls). Production playback controls remain planned.

## Drive and observe

Observe animation, pause it, then resume it. Check rendered behavior as well as reported state. Resize during playback and verify dimensions and composition. Observe frame delivery under a slow consumer, bounded resource use, and the documented drop or backpressure behavior. Release frames according to the API contract. Close the viewer and establish that its owned process and workers exit.

## Evidence and limits

Retain actions, timestamps, captures, state observations, and lifecycle diagnostics. Check GPU allocations over repeated load/close cycles once measurement exists. Define pause semantics and allowed in-flight work before setting expectations. Process exit alone does not prove resource behavior in a long-lived host.
