# CPU and GPU optimization, 25 October 2026

This follows the [initial baseline](performance-baseline-2026-10-25.md) and
[sprite memory optimization](performance-climb-2026-10-25.md). It targets Gojo's
particle submission cost and the 4K composition path while preserving existing
pixels, effects, animation timing and frame targets.

## Profile and select changes

Opt-in [execution profiling](performance-benchmarks.md#opt-in-execution-profiling)
separates worker preparation/submission wall time from GPU timestamp intervals.
The initial 4K particle workload renders 1441 quads in its first frame. Its
warmed worker command submission averages about 730 µs per frame, versus a
0.52 ms GPU interval. Composition uses about 1.70 ms on the GPU and 76 µs in
worker submission. These observations identify different costs rather than
treating completed FPS as GPU execution time.

The selected changes are:

- Keep pipeline, target, viewport and texture bindings between consecutive
  plain particles when their source views and blend mode match. Update every
  quad's constants and preserve every draw's order. Composition points, effects,
  refraction, source changes and blend changes invalidate reuse. Unbind retained
  inputs before targets can be reused or the frame ends.
- Compare existing texture allocation identities without constructing temporary
  weak references on each lookup. Retain the original weak ownership of uploads.
- Read and convert calendar time only when the scene contains rendered text.
  Procedural animation continues to use the existing playback clock.
- Capture composition directly from the unbound accumulated scene into its
  separate intermediate. Remove the preceding full-frame snapshot copy and its
  texture. Refraction keeps its snapshot because it reads prior scene pixels
  while writing the scene.

An axis-only blur sampling trial did not improve warmed throughput. It was
rejected. The shipped shaders, image sampling, source resolution, effect passes,
particle counts and GPU completion polling remain unchanged.

## Repeatable comparison

Measurements use an RTX 5080, Ryzen 9 9950X3D with 32 logical processors,
Windows 11 build 26200, driver 32.0.16.1664 and the Balanced power plan.
Output is 3840×2160 with linear sampling, animation/VFX enabled and tolerant
loading. Existing unsupported-feature reports remain part of the workload.

The comparison alternates pinned before/after binaries twice, with both Gojo
workloads in each run. Each case has two-second warmups/idle controls and
20-second paced and uncapped intervals. Profiling is disabled during those
production measurements. Separate eight-second profiles compare the same
workloads with timestamp queries enabled. Builds, profiling and correctness
checks run separately from ordinary performance measurements.

```powershell
pwsh -File scripts/benchmark.ps1 -Workloads 'gojo-particles,gojo-composition' -Resolutions 3840x2160 -Repeat 2 -Seconds 20 -Warmup 2 -Idle 2
pwsh -File scripts/benchmark.ps1 -Workloads 'gojo-particles,gojo-composition' -Resolutions 3840x2160 -Repeat 2 -Seconds 8 -Warmup 2 -Idle 2 -ProfileGpu
```

Use `python -B scripts/benchmark.py --executable <pinned-before.exe>` for the
before build. Its source identity comes from the report that originally pinned
that binary. A new collection report's checkout hashes describe the collecting
checkout, which can differ from an explicitly selected older executable.

CPU percent is Windows job CPU time divided by all 32 logical processors.
GPU utilization is the busiest owned engine and varies with clocks. Compare
utilization at the same 60 fps target. An uncapped renderer can use more CPU/GPU
per second because it completes more frames. GPU timestamps cover the command
interval, including pipeline/scheduling gaps; worker timings are wall time.
Neither establishes monitor cadence or isolated shader cost.

## Results

The [frozen comparison](../benchmarks/results/2026-10-25-cpu-gpu.json) retains
raw-report hashes, source/binary provenance, settings, every repetition and
exploratory profiles. Ordinary results below are medians of two runs. Ranges
are observed minima/maxima rather than confidence intervals.

| Gojo particles, 4K | Before | After |
| --- | ---: | ---: |
| Paced CPU, machine % | 0.309 (0.282–0.336) | 0.119 (0.109–0.129) |
| Paced busiest GPU engine, % | 3.577 (3.553–3.601) | 3.227 (3.221–3.233) |
| Paced completed fps | 60.00 | 60.00 |
| Uncapped completed fps | 385.8 (385.6–385.9) | 530.6 (529.4–531.7) |
| Paced p95 ready gap, ms | 17.31 | 17.26 |
| Paced private commit, MiB | 299.0 | 298.6 |
| Uncapped CPU, machine % | 2.030 | 1.215 |

At the matched 60 fps target, CPU decreases **61.6%** and busiest-engine
utilization decreases **9.8%**. Uncapped throughput increases **37.5%** while
using less CPU per second. Uncapped GPU utilization rises from about 22.1% to
26.9% because more frames complete; it is not compared as a fixed-workload cost.
The matched-rate controls and separate GPU intervals address that distinction.

The final matched eight-second profiling pair uses the same binaries as the
ordinary runs and has zero invalid GPU samples:

| Timestamp/worker mean, Gojo particles | Before | After |
| --- | ---: | ---: |
| Paced GPU interval, ms | 0.533 | 0.468 |
| Uncapped GPU interval, ms | 0.542 | 0.450 |
| Uncapped worker preparation, µs | 28.8 | 25.8 |
| Uncapped worker submission, µs | 755.2 | 133.4 |

The GPU interval falls 12.1% paced and 16.9% uncapped; submission wall time
falls 82.3%. An earlier candidate profile independently showed about 730 to
132 µs submission and 0.52 to 0.45 ms uncapped GPU intervals. These are complete
frame intervals, not a claim that the unchanged particle shader became faster.

For Gojo composition, ordinary uncapped medians are **315.7 → 316.9 fps**.
The final profiled uncapped GPU interval is **1.713 → 1.685 ms**. These small
changes do not establish a substantial composition speedup. Paced CPU ranges
overlap and are wide, from 0.072–0.114% before to 0.002–0.096% after; no stable
composition CPU percentage improvement is claimed. Windows job CPU accounting
can be coarse for very short CPU bursts.

Removing the snapshot lowers composition effect payload from **190.6 to
159.0 MiB**, exactly one 4K RGBA8 texture. Measured dedicated GPU memory drops
from about **369.1 to 337.2 MiB**, and private commit from **413.8 to 382.5 MiB**.
Neither workload adds output allocations or source uploads after warmup. Both
versions skip zero paced slots. Particle deadline misses remain 0–2 per run;
composition misses change from 1 to 1–3. The recorded timing is not proof of
smooth monitor presentation. Initial PNG hashes and complete playback reports
match in all ordinary before/after cases.

## Verification

The [required software/headless gate](../artifacts/20261025-201802-c17f339a/manifest.json)
passes formatting, Clippy, Rustdoc, dependency boundaries, public contracts,
WARP pixels and diagnostic motion. GPU contracts cover particle frame blending,
draw order, composition points, HDR, refraction, text/clocks, resize, resource
release and retained frames. The composition resource assertion now expects
three shared targets rather than four; its pixel expectations are unchanged.
New profiling tests compare profiled/unprofiled pixels, confirm idle counters
stop, and reject invalid timestamp arithmetic. Native-window checks are skipped.

Both actual scenes pass hardware headless Chrome, Playwright and Chrome
DevTools MCP checks: [particles](../artifacts/browser-utilization-particles-20261025/report.json)
and [composition](../artifacts/browser-utilization-composition-20261025/report.json).
Checks cover native/canvas pixel equality, exact seek/rewind, live advancement,
pause/idle, bounded output, resize, errors and owned cleanup. The particle
1500 ms browser capture was inspected.

The [browser responsiveness receipt](../benchmarks/results/2026-10-25-cpu-gpu-browser.json)
delivers all 160 typed inputs across paused/playing/paused/playing intervals.
Event-to-handler p95 is about 0.1 ms, keyboard round-trip p95 is 1.71–1.84 ms,
and no long tasks are observed. Paused publication remains zero. The debug PNG
route completes only four native frames per enabled interval, so this is a
low-rate host proxy rather than production host or monitor-frame-rate acceptance.
It compares disabled/enabled rendering in this build, not old/new host latency.

The [collection sweep](../artifacts/workshop-support-utilization-20261025/report.json)
passes **65/65**. Every fixed-time pixel hash at 0/1500/4000 ms and rewind, plus
every complete playback report, matches the preceding sprite-window build.
Existing feature omissions are unchanged. Three Python benchmark metric tests
also pass. The percentage improvements above describe the measured particle
workload; the collection sweep establishes correctness rather than performance
across every wallpaper.

The remaining consumer sample-selection/request interval is around 0.45 ms per
call in this workload. Profiling particle construction within that interval is
the next CPU target. The composition GPU interval remains around 1.7 ms and
needs finer pass-level profiling; the rejected blur rewrite does not justify a
shader change.
