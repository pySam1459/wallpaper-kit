# Sprite residency optimization, 25 October 2026

The initial baseline identifies Yuta as the largest memory workload. Its two
animated textures previously retained two complete extracted page runs each.
The bird batches alone held about 475 MiB; character batches added 106 MiB.
These are CPU pixel payloads, before other scene, driver and consumer memory.

## Profile before changing residency

The `sprite-profile` example uses the public texture and pager APIs. It records
frame/page sizes, first decode/extraction duration and 60 Hz samples of ready
bytes, preparation reservations, decode counts and source availability. It
does no GPU work. JSON observations perturb timing, so native resource and
throughput measurements run separately after profiling has exited.

```powershell
cargo build --release --locked --example sprite-profile
@'
{
  "package": "C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg",
  "engine_assets": "C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets",
  "assets": [
    "materials/oiseaux optimized_00000.tex",
    "materials/resize yuta_00000.tex"
  ],
  "seconds": 35
}
'@ | Set-Content artifacts/sprite-profile-config.json
target/release/examples/sprite-profile.exe artifacts/sprite-profile-config.json artifacts/sprite-profile-new
```

The output directory must be new. Each report retains the package hash and raw
observations. Readiness includes startup; compare settled playback separately.
A polling miss means a source sample was unavailable or its metadata was busy;
it does not directly count a missed displayed frame.

## Change and rejected candidate

The pager now divides each authored page run into extracted-frame windows of
at most 64 MiB. It retains the current and next windows, keeping one decoded
atlas while another window needs it. It releases the atlas before moving to a
different page. The original 256 MiB whole-run admission check, 64 MiB single
frame limit and 768 MiB preparation cap remain enforced. Caller-retained
samples remain independent of eviction and shutdown.

A 32 MiB trial reduced memory further but produced four post-warmup source
readiness misses at bird page transitions in its 35-second profile. That trial
was rejected before native benchmarking. A short final batch left too little
lead for occasional slower page decodes. The selected implementation uses
64 MiB windows and redistributes frames into a short tail, without crossing
page boundaries or exceeding the window limit.

The final CPU profile covers the full 28.8-second authored loop and reports
zero post-warmup unavailable samples for both animated textures. Smaller
windows reuse decoded atlases rather than decoding them again. No sampling,
frame duration, image quality, effect or shader setting changes.

## Comparison procedure

Native runs use the same Yuta package, RTX 5080, Ryzen 9 9950X3D, Windows 11
build 26200 and Balanced power plan as the initial baseline. Output is
3840×2160, linear sampling, animation/VFX enabled and tolerant loading. Existing
unsupported-feature omissions remain part of the measured workload.

Each process measures 35 seconds paced at 60 fps and 35 seconds uncapped, with
three-second warmups and idle controls. Runs are sequential; profiling, builds
and correctness checks finish before measurement. The original benchmark
executable is pinned from the initial baseline. Later comparisons alternate
the old and new executables to expose environmental variation.

```powershell
python -B scripts/benchmark.py --executable artifacts/performance-baseline-clean-20261025/benchmark.exe --output artifacts/climb-before-new --workloads yuta --resolutions 3840x2160 --repeat 1 --seconds 35 --warmup 3 --idle 3
pwsh -File scripts/benchmark.ps1 -Output artifacts/climb-after-new -Workloads yuta -Resolutions 3840x2160 -Repeat 1 -Seconds 35 -Warmup 3 -Idle 3
```

Private commit is process-tree memory, distinct from working set and GPU
memory. CPU percent uses all 32 logical processors. GPU utilization is the
busiest owned engine, affected by scheduling and clocks. Completed native FPS
excludes readback and browser transport; it does not measure monitor cadence.

## Measured result

The [frozen comparison](../benchmarks/results/2026-10-25-sprite-windows.json)
retains two alternating repetitions per version, executable/source hashes,
raw-report hashes, settings and ranges. Earlier exploratory runs are retained
separately. Values below are medians of run summaries; parentheses show the
observed minimum and maximum, not confidence intervals.

| Metric, Yuta at 4K | Original | Bounded windows |
| --- | ---: | ---: |
| Paced private commit, MiB | 912.4 (911.8–913.1) | 733.2 (732.0–734.4) |
| Peak job private commit, MiB | 1184.8 (1184.7–1185.0) | 853.9 (853.5–854.3) |
| Paced working set, MiB | 734.4 | 554.5 |
| Paced CPU, machine % | 0.431 (0.407–0.454) | 0.478 (0.420–0.535) |
| Paced busiest GPU engine, % | 5.11 (4.58–5.65) | 6.60 (5.75–7.46) |
| Paced dedicated GPU memory, MiB | 261.2 | 261.2 |
| Paced completed fps | 60.00 | 60.00 |
| Paced p95 ready gap, ms | 18.25 | 18.67 |
| Uncapped completed fps | 612.0 (610.1–614.0) | 608.2 (603.4–613.1) |
| Uncapped CPU, machine % | 0.821 | 0.859 |
| Uncapped busiest GPU engine, % | 26.86 | 26.55 |

Average paced private commit falls **19.6%**, or 179.2 MiB. Peak commit falls
**27.9%**, or 331.0 MiB. Both versions have zero skipped paced slots, 1168 source
uploads per measured interval and zero new output allocations. Each version
still records occasional late completions: original paced deadline misses are
1–2, versus 0–2 after the change. Source preparation counters do not show
repeated same-page decoding. Initial PNG hashes and complete playback reports
match across all four comparison runs.

This is a memory improvement, with no demonstrated CPU or GPU speedup.
Uncapped throughput medians differ by -0.6% and their ranges overlap. CPU
medians increase, with overlapping ranges; the paced difference is about
0.015 CPU cores. Paced GPU utilization increases and p95 completion gaps rise
0.42 ms. GPU clock/power states vary between runs: paced mean graphics clocks
span about 1190–1571 MHz after the change and 1381–1425 MHz before it.
These observations do not isolate GPU execution cost. GPU timestamp profiling
and controlled-clock comparisons remain necessary before claiming a GPU win.

The [final CPU profile](../artifacts/sprite-profile-final64-20261025/report.json)
ends with 33 bird atlas decodes and 38 character decodes, compared with 34 and
38 before. The difference in bird look-ahead explains the single decode saved;
it is not a claim of faster decoding. The rejected 32 MiB profile is retained
in the frozen comparison with its four post-warmup unavailable samples.

The original executable's source identity comes from its initial baseline
receipt, whose binary hash matches each new before-run. Checkout hashes in a
new report describe the checkout collecting the run; they cannot describe an
older explicitly selected executable. The frozen comparison keeps those
provenance roles separate.

## Correctness and consuming-app checks

The [required software/headless gate](../artifacts/20261025-193849-43de7d6b/manifest.json)
passes formatting, Clippy, public contracts, Rustdoc, dependency boundaries,
WARP output, diagnostic motion and texture checks. A generated large-atlas
contract verifies exact GPU pixels through window boundaries, looping and
seeks, payload limits, one-decode reuse and retained samples after shutdown.
Native-window scenarios are skipped.

The [Yuta browser run](../artifacts/browser-sprite-windows-20261025/report.json)
passes on hardware through headless Chrome, Playwright and Chrome DevTools MCP.
It checks native RGBA against canvas pixels, exact rewind, live advancement,
freeze-on-pause, idle, bounded output, resize, error recovery and owned cleanup.
The 4K native initial capture and browser 1500 ms capture were inspected.

The separate [browser responsiveness receipt](../benchmarks/results/2026-10-25-sprite-windows-browser.json)
delivers all 160 typed inputs across paused/playing/paused/playing intervals.
Event-to-handler p95 is 0.1–0.2 ms and Playwright keyboard round-trip p95 is
1.44–1.62 ms. No long tasks are observed. Paused publication stays at zero;
each enabled interval completes three native frames. This debug PNG transport
is a low-rate host proxy, not production 60 fps host acceptance. It compares
disabled/enabled rendering within this build, not old/new host performance.

The [65-wallpaper collection sweep](../artifacts/workshop-support-sprite-windows-20261025/report.json)
passes 65/65. All fixed-time pixel hashes at 0/1500/4000 ms and rewind, plus
complete playback reports, match the preceding collection receipt. Supported
layers and existing unsupported-feature omissions are unchanged. This sweep
checks rendering behavior; the memory percentages above describe Yuta rather
than a new performance matrix across the whole collection.

The next profiling target is 4K composition/blur: the initial Gojo composition
baseline drops from about 610 uncapped fps at 1080p to 315 at 4K. GPU timestamp
measurements per pass would distinguish shader work from completion polling
and clock-state effects before selecting the next optimization.
