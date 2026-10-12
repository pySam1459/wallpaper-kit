# Performance and host impact

## Status

T3 delivery now follows `requestAnimationFrame`; its native worker uses absolute
frame deadlines and a scoped Windows timer request. Source videos retain their
rational average rate instead of being converted to 30 fps. The local Panda
export benchmark improved from 21.43 fps with simulated old T3 polling to 59.41
fps at a 60 fps target and 119.99 fps at a 120 fps target. Read
[playback cadence](../../../../docs/smooth-playback.md) for commands, raw receipts,
CPU/memory samples and the distinction between exports and display timing.
Actual T3 and Wallpaper Engine captures establish visible motion; smooth monitor
presentation and exact engine fidelity remain unmeasured.

Blank verification reports one measured native idle interval with process CPU time, working set and handles. [Diagnostic motion](diagnostic-motion.md) records a separate warmed run's readiness percentiles, missed deadlines, skipped slots and acquisition duration, plus native presentation-call return samples. [TEX page checks](texture-pages.md) record source payload bytes, cumulative uploads and diagnostic decode times, with repeated draws/resize verifying upload reuse. Full scene metrics, GPU timing, host input latency and steady-state acceptance remain planned; proposed scene budgets are in the specification.

## Consumer outcome

Rendering stays within measured resource limits and preserves consuming-app responsiveness. Under pressure, background quality or animation frequency can decrease instead of delaying the host UI.

## How to reach it

Use a repeatable workload in the example viewer for renderer measurements. Use an actual host integration for input and scrolling latency. Record concrete commands and metric definitions after implementation.

## Drive and observe

Measure matching runs with rendering disabled and enabled. Record startup separately from steady playback. Collect CPU usage, GPU time, memory, frame timing, and variability. Exercise contention and lifecycle transitions, including hidden/minimized behavior when supported. For changes intended to improve performance, compare the same scene, hardware, and configuration before and after.

## Evidence and limits

Keep raw measurements, sampling methods, hardware and driver, build identity, and settings. Define metric units and acceptance budgets from real workloads. Record capture/overlay overhead and distinguish diagnostic runs from normal playback. Include integrated graphics in the test set when available. Good FPS in the viewer does not prove low host latency or zero impact on shared hardware.
