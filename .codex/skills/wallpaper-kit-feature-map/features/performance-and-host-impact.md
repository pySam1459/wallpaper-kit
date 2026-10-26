# Performance and host impact

## Status

The [optimization log](../../../../docs/performance-progress.md) records each
step and the completed 26 October original-settings comparison: 28/28 cases,
unchanged initial pixels and playback reports, and zero owned processes after
cleanup. CPU/GPU observations include regressions and ranges. The
[idle comparison](../../../../docs/idle-performance-2026-10-25.md) verifies two
warm/trimmed repetitions of three 4K workloads: private memory falls 61–77%,
and dedicated GPU memory settles near 77 MiB. CPU/GPU idle activity remains
near the floor. Supported-feature ablations rank composition blur chains and
video decoding without changing quality defaults. The final
[software gate](../../../../artifacts/20261026-000945-f4bc23d0/manifest.json)
passes; native windows remain skipped. The browser input proxy delivers all
160 events with no observed long tasks, but only three PNG-delivered native
frames per enabled interval. This is separate from production host acceptance.

The [CPU/GPU comparison](../../../../docs/cpu-gpu-climb-2026-10-25.md)
is verified on 25 October 2026. Two alternating ordinary 20-second runs per
version measure Gojo particles at 4K: paced CPU falls 61.6%, busiest-engine
utilization falls 9.8%, and uncapped completed throughput rises 37.5%. Both
versions hold 60 paced fps, with unchanged initial pixels and playback reports.
Separate opt-in GPU timestamps and worker timings show lower frame intervals
and command submission cost. Profiling is disabled by default; run
`scripts/benchmark.ps1 -ProfileGpu` for execution diagnostics. Invalid timestamps
remain explicit, and queries never introduce a completion wait. Composition
throughput remains similar while one 4K snapshot allocation/copy is removed.
The required [software gate](../../../../artifacts/20261025-201802-c17f339a/manifest.json)
and both actual-scene hardware browser runs pass. The 65-wallpaper sweep keeps
every fixed-time pixel hash and complete playback report unchanged. The browser
proxy delivers 160/160 inputs with no observed long tasks, but only four native
frames per enabled interval. Native windows and production display/host latency
are unverified. Percentages apply to the measured particle workload.

The [sprite residency comparison](../../../../docs/performance-climb-2026-10-25.md)
verifies the first memory optimization on 25 October 2026. Two alternating
35-second Yuta 4K runs per version reduce average paced private commit from
912.4 to 733.2 MiB and peak commit from 1184.8 to 853.9 MiB. Both versions hold
60 completed fps; uncapped medians are 612.0 and 608.2 fps. Pixel hashes, playback
reports and upload counts match. CPU ranges overlap, paced GPU counters rise,
and no CPU/GPU speedup is claimed. The frozen comparison retains ranges,
exploratory failures and executable/source provenance. The required
[WARP/headless gate](../../../../artifacts/20261025-193849-43de7d6b/manifest.json)
passes; native-window verification is skipped.
The [hardware browser run](../../../../artifacts/browser-sprite-windows-20261025/report.json)
passes Playwright/Chrome DevTools checks and owned cleanup. The paused/playing
Yuta proxy delivers all 160 typed inputs with no observed long tasks, but only
three PNG-delivered native frames per enabled interval. The
[collection sweep](../../../../artifacts/workshop-support-sprite-windows-20261025/report.json)
loads 65/65 with every fixed-time pixel hash and playback report unchanged.
The memory reduction is Yuta-specific; the collection sweep is a correctness
check, not a new collection-wide performance matrix.

Release native renderer benchmarks are verified on 25 October 2026. The
[benchmark guide](../../../../docs/performance-benchmarks.md) defines the
seven-workload, 1080p/4K, two-repetition matrix and metric units. Windows job
accounting includes renderer/decoder CPU and exited children; PDH counters
measure owned GPU engines and GPU memory. Disabled, paused, paced, uncapped and
released controls retain raw timing/resource samples and source/build identity.
The [initial baseline](../../../../benchmarks/baselines/2026-10-25-rtx5080.md)
records repetition medians; its JSON retains ranges, configuration and hashes.
All 28 cases pass with zero active owned processes after shutdown. One blank
control was interrupted by an OS/GPU-access transition and passed an explicit
retry; its failure history remains recorded. The controller prevents idle
system sleep without changing the power plan or keeping the display on.
GPU utilization and submit-to-ready time are separate from GPU timestamp-query
duration. Native presentation and production host input latency remain open.

The [baseline findings](../../../../docs/performance-baseline-2026-10-25.md)
include a 35-second Yuta run through its full sprite loop, with no new output
allocations after warmup. A separate headless browser proxy delivers all 160
input events in alternating paused/playing intervals and records keyboard,
event-handler, animation-frame and long-task observations. Its debug PNG route
updates only four native frames per enabled interval; this is not production
host frame-rate acceptance. Owned cleanup passes. Four Python metric/job tests,
the WARP benchmark smoke route and the required software/headless gate pass.
Native-window checks are skipped.

T3 delivery now follows `requestAnimationFrame`; its native worker uses absolute
frame deadlines and a scoped Windows timer request. Source videos retain their
rational average rate instead of being converted to 30 fps. The local Panda
export benchmark improved from 21.43 fps with simulated old T3 polling to 59.41
fps at a 60 fps target and 119.99 fps at a 120 fps target. Read
[playback cadence](../../../../docs/smooth-playback.md) for commands, raw receipts,
CPU/memory samples and the distinction between exports and display timing.
Actual T3 and Wallpaper Engine captures establish visible motion; smooth monitor
presentation and exact engine fidelity remain unmeasured.

Blank verification reports one measured native idle interval with process CPU time, working set and handles. [Diagnostic motion](diagnostic-motion.md) records a separate warmed run's readiness percentiles, missed deadlines, skipped slots and acquisition duration, plus native presentation-call return samples. [TEX page checks](texture-pages.md) record source payload bytes, cumulative uploads and diagnostic decode times, with repeated draws/resize verifying upload reuse. Per-pass GPU timestamp duration, production host input latency, broad scene coverage and steady-state acceptance remain open; proposed scene budgets are in the specification.

## Consumer outcome

Rendering stays within measured resource limits and preserves consuming-app responsiveness. Under pressure, background quality or animation frequency can decrease instead of delaying the host UI.

## How to reach it

Run `pwsh -File scripts/benchmark.ps1` for the release resource/throughput
matrix. Use `python -B scripts/benchmark.py --help` for source roots and selected
binaries. The [guide](../../../../docs/performance-benchmarks.md) defines phase
controls, raw artifacts, retries and the separate browser responsiveness probe.
Finish all builds and verification before collecting performance samples.
Use an actual product host for production input/scrolling acceptance.

## Drive and observe

Measure matching runs with rendering disabled and enabled. Record startup separately from steady playback. Collect CPU usage, GPU time, memory, frame timing, and variability. Exercise contention and lifecycle transitions, including hidden/minimized behavior when supported. For changes intended to improve performance, compare the same scene, hardware, and configuration before and after.

## Evidence and limits

Keep raw measurements, sampling methods, hardware and driver, build identity, and settings. Define metric units and acceptance budgets from real workloads. Record capture/overlay overhead and distinguish diagnostic runs from normal playback. Include integrated graphics in the test set when available. Good FPS in the viewer does not prove low host latency or zero impact on shared hardware.
