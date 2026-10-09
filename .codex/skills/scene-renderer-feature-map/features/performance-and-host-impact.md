# Performance and host impact

## Status

Blank verification reports one measured native idle interval with process CPU time, working set and handles. [Diagnostic motion](diagnostic-motion.md) records a separate warmed run's readiness percentiles, missed deadlines, skipped slots and acquisition duration, plus native presentation-call return samples. Full scene metrics, GPU timing, host input latency and steady-state acceptance remain planned; proposed scene budgets are in the specification.

## Consumer outcome

Rendering stays within measured resource limits and preserves consuming-app responsiveness. Under pressure, background quality or animation frequency can decrease instead of delaying the host UI.

## How to reach it

Use a repeatable workload in the example viewer for renderer measurements. Use an actual host integration for input and scrolling latency. Record concrete commands and metric definitions after implementation.

## Drive and observe

Measure matching runs with rendering disabled and enabled. Record startup separately from steady playback. Collect CPU usage, GPU time, memory, frame timing, and variability. Exercise contention and lifecycle transitions, including hidden/minimized behavior when supported. For changes intended to improve performance, compare the same scene, hardware, and configuration before and after.

## Evidence and limits

Keep raw measurements, sampling methods, hardware and driver, build identity, and settings. Define metric units and acceptance budgets from real workloads. Record capture/overlay overhead and distinguish diagnostic runs from normal playback. Include integrated graphics in the test set when available. Good FPS in the viewer does not prove low host latency or zero impact on shared hardware.
