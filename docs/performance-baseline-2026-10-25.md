# Initial performance baseline, 25 October 2026

The release renderer completed 28 measured cases: seven workloads at 1080p and
4K, with two repetitions. A separate 35-second Yuta run covers its full sprite
loop. Production rendering code is unchanged. The new example uses the public
API; browser transport is excluded from these native measurements.

Hardware: Ryzen 9 9950X3D, 16 cores / 32 logical processors, NVIDIA RTX 5080,
Windows driver 32.0.16.1664, Windows 11 build 26200, Balanced power plan. Settings
are linear image sampling, animation/VFX enabled and tolerant scene discovery.
These workloads retain their existing unsupported-feature omissions.

The [complete table](../benchmarks/baselines/2026-10-25-rtx5080.md) and
[machine-readable baseline](../benchmarks/baselines/2026-10-25-rtx5080.json)
retain medians and repetition ranges. The
[raw matrix](../artifacts/performance-baseline-clean-20261025/report.json)
contains frame-time distributions, GPU memory, startup/shutdown, source/build
identities, resources, disabled/paused controls and cleanup results.

## Selected 4K results

These are medians of two runs. Resource values describe paced playback at about
60 completed frames/s. CPU % uses all 32 logical processors; RAM is process-tree
private commit, including decoder children. GPU % is the busiest owned engine.

| Workload | Paced fps | Uncapped fps | CPU % | RAM MiB | GPU % | Dedicated GPU MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Panda Store | 60.00 | 606 | 0.13 | 376 | 4.5 | 278 |
| Gojo particles | 60.00 | 452 | 0.38 | 304 | 7.0 | 237 |
| Gojo composition / blur | 60.00 | 315 | 0.15 | 420 | 10.4 | 369 |
| Yuta | 60.00 | 603 | 0.51 | 936 | 6.5 | 261 |
| Tram embedded video | 60.00 | 439 | 2.06 | 357 | 8.6 | 127 |

Gojo's composition workload falls from about 610 uncapped fps at 1080p to 315
at 4K. Video decoding has the highest paced CPU cost, about 0.66 CPU cores at
4K. Yuta has the largest process-memory footprint. GPU utilization varies across
repetitions and clocks; the JSON retains that spread rather than treating a
single percentage as a fixed cost.

All scene runs stay near 60 fps at the paced target. A static 1080p control
records 59.12 and 59.99 fps across the two runs; its timing and skipped slots
remain in the baseline. A passing run establishes completed measurement and
resource/control checks, not a universal frame-time acceptance threshold.

## Longer Yuta observation

The [35-second 4K run](../artifacts/performance-yuta-loop-20261025/report.json)
records 60.00 paced fps and 609.94 uncapped fps. Average private commit is about
918 MiB paced and 917 MiB uncapped. First-to-last private commit changes by
+1.07 MiB and -0.25 MiB respectively. Peak job private commit, including setup,
is 1,187 MiB, about 1.16 GiB.

Neither interval creates another output texture after warmup. Each records
1,168 source texture uploads across the full sprite loop. That upload rate and
the CPU page footprint are useful targets for later profiling. The
[frozen long-run data](../benchmarks/baselines/2026-10-25-rtx5080-yuta-loop.json)
is separate from the shorter matrix; the durations are not pooled.

After joined renderer/playback shutdown, process private commit remains above
the roughly 2.6 MiB disabled control. At 4K, released private commit ranges from
about 22 MiB for blank to 237 MiB for Gojo composition, with around 32 MiB still
reported by GPU dedicated-memory counters. These observations need allocator,
driver-cache and repeated-lifecycle profiling before attributing them to a leak.
Every completed run reaches zero active owned processes after exit.

## Browser consumer probe

The separate [responsiveness receipt](../artifacts/performance-browser-responsiveness-20261025/responsiveness.json)
and [frozen summary](../benchmarks/baselines/2026-10-25-browser.json) compare two
paused and two playing intervals in the existing headless Chrome consumer.
All 160 typed characters arrive. Paused intervals produce zero new native
frames. Event-to-handler p95 is 0.1–0.2 ms; Playwright/CDP keyboard round-trip
p95 is 1.7–2.1 ms. No long tasks are recorded.

The debug PNG transport delivers only four native frames per playing interval,
roughly 0.5 fps. This is a limited browser input-isolation probe, not a production
frame-rate host test or T3 latency measurement. It is excluded from the release
throughput numbers. Native presentation, GPU sharing and actual product-host
input/scrolling acceptance remain unmeasured.

## Reliability and reproduction

One blank 4K control is interrupted before renderer initialization. NVIDIA
reports GPU access blocked by Windows, followed by the controller timeout. An
explicit retry with the same pinned executable passes. The original failed
case, preceding report and retry history remain retained. A preliminary matrix
started alongside validation was excluded entirely. The controller now prevents
idle system sleep temporarily and rejects long resource-sampling gaps.

Run `pwsh -File scripts/benchmark.ps1` to reproduce the matrix. See
[benchmark methods and metric definitions](performance-benchmarks.md) for
custom durations, raw files, retry behavior and comparison limits.

The [required software/headless gate](../artifacts/20261025-185940-a38a76d6/manifest.json)
passes format, Clippy, public contracts, Rustdoc, dependency boundaries and
consumer checks. Four Python metric/job tests pass, including exited-child CPU
accounting and job-close cleanup. The
[software benchmark smoke run](../artifacts/performance-software-smoke-20261025/report.json)
also passes. Native-window scenarios are skipped. Initial Yuta and Gojo 4K
captures were inspected.

This baseline does not establish theoretical GPU throughput, smooth monitor
presentation, thermal equilibrium, performance on integrated GPUs, complete
Wallpaper Engine fidelity or absence of long-term leaks.
