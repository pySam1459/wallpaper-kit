# Performance benchmarks

The [25 October initial baseline](performance-baseline-2026-10-25.md) includes
the completed matrix, a longer sprite-loop run and browser responsiveness data.
The [first optimization](performance-climb-2026-10-25.md) profiles sprite
residency and compares bounded prefetch windows against the pinned baseline.

Run the release renderer through its public API and collect Windows process-tree
and GPU counters:

```powershell
pwsh -File scripts/benchmark.ps1
```

The default matrix covers seven workloads at 1920×1080 and 3840×2160, twice.
It uses hardware D3D11, linear image sampling, animation and VFX, and tolerant
scene loading. The five downloaded workloads retain their omission reports;
their results measure currently rendered content, not full Wallpaper Engine
fidelity. The other workloads are a blank control and an original static image.
No third-party artwork is committed.

Each run uses a new process and pinned executable. The controller creates a
Windows job before the worker starts loading. CPU accounting includes decoder
children that exit between samples. Closing the owned job also contains cleanup
after a failure or timeout. No desktop window or browser is opened.
The controller temporarily prevents idle system sleep. It permits the display
to turn off and leaves the power plan unchanged; the request ends on exit.

## Workload phases

| Phase | Behavior |
| --- | --- |
| `disabled` | Process exists, renderer and scene have not been created |
| Startup | Create the device, open assets, prepare and complete the first content frame |
| Warmup | Render continuously; excluded from measurements |
| `paused` | Scene and GPU resources remain loaded, without new requests |
| `paced` | Request against absolute 60 fps deadlines; skip expired slots |
| `uncapped` | Request the next frame immediately after observing completion |
| `paused_after` | Stop requests and check that the retained frame remains unchanged |
| `released` | Join and drop renderer/playback, then observe remaining process resources |

There are two-second idle controls, two-second warmups before active intervals,
and eight-second measured active intervals by default. The library installs no
autonomous render timer. Blank and static active intervals deliberately force
draws to measure their cost; real static consumers should stop requesting frames.
The consumer does not request a different Windows timer resolution.

Startup timing distinguishes device initialization, asset opening and first
content readiness. These are fresh-process measurements with ordinary OS file
and driver caches, not cold-disk tests. The initial capture validates the output
route and is retained as `initial.png`. Readback and PNG encoding finish before
warmup. Neither occurs during measured playback.

## Metrics and units

| Metric | Definition |
| --- | --- |
| Completed fps | Completed native frames divided by the measured wall interval |
| Uncapped fps | Observed public-consumer throughput with one render request in flight |
| Ready gaps | Time between observed completed frames; mean, p50, p95, p99 and maximum |
| Sample-to-ready | Source selection through completed-frame observation |
| Sample wait | Time spent selecting/preparing a ready composition, including asynchronous source waits and submission |
| Submit-to-ready | Time after submission returns until completed-frame observation |
| Source age | Playback clock minus selected sample time, useful for delayed decoding |
| Skipped slots / deadline misses | Requests skipped when late / completed frames past their slot's end |
| Changed source-index selections | Selected sprite/video indices that differ from the preceding observation |
| CPU cores | Job kernel + user CPU seconds divided by measured wall seconds |
| CPU machine % | CPU cores divided by logical processor count, multiplied by 100 |
| Working set | Sum of sampled resident working sets for live owned processes |
| Private bytes | Sum of private committed memory for live owned processes |
| Peak job private bytes | OS job peak committed process memory, including setup and child peaks |
| GPU engine % | Busiest owned engine after summing owned processes on each adapter/engine |
| Dedicated / shared GPU bytes | Windows GPU Process Memory counters for owned PIDs |
| Renderer payload bytes | Public output/source/effect texture counters; excludes driver/consumer allocations |
| Uploads / allocations | Public cumulative texture-upload and output-allocation counts before/after each active interval |
| Resource growth | Difference between last and first in-phase private-byte samples, plus an extrapolated rate |
| Handles, I/O, processes | Owned handles, logical process I/O bytes, active and lifetime process counts |

CPU uses [Windows job accounting](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information).
Private commit and resident working set are distinct
[process-memory measures](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex).
Summed working sets can count shared pages more than once. Live-process memory
samples can miss short-lived children, so the job peak is recorded separately.
CPU accounting can be quantized on Windows; a short interval reported as zero
is not proof of no CPU work.

GPU counters use English-name PDH queries with wildcard instance arrays,
filtered to the owned process tree. The busiest-engine definition follows
[Microsoft's Task Manager explanation](https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/).
It is not a sum across unrelated GPU engines. Unavailable counters remain null
with errors, rather than being reported as zero. Counters are sampled every
250 ms, with sampling cost retained. Phase summaries exclude intervals crossing
a phase boundary and report their actual coverage.

When `nvidia-smi` is installed, separate one-second logs record whole-adapter
utilization, memory activity, allocated memory, power, temperature, clocks and
P-state. These include unrelated apps. NVIDIA's
[utilization definition](https://docs.nvidia.com/deploy/nvidia-smi/index.html#utilization)
describes activity over its sampling window; it is not per-frame GPU duration.
The controller's CPU and sampling cost are outside the worker job and recorded
separately. External telemetry and other desktop applications can still affect
the workload.

## Reproduce a smaller run

```powershell
pwsh -File scripts/benchmark.ps1 -Workloads 'panda,gojo-composition' -Resolutions 1920x1080 -Repeat 2 -Seconds 30 -Warmup 3 -Idle 3
```

`benchmarks/workloads.json` defines the names and source folder IDs. To choose
different local roots or a particular binary, call `python -B
scripts/benchmark.py --help`. The PowerShell wrapper builds with `--release
--locked`; `-SkipBuild` reuses the existing release executable, whose hash is
still recorded. `-Software` explicitly selects WARP and cannot establish
hardware performance. Output directories must be new and under ignored
`artifacts/`.

Use longer intervals to investigate memory growth. Both timed modes use the
requested duration. The per-frame telemetry bound is 120,000 frames per interval;
shorten an interval if it reaches this bound. The telemetry buffer and example
consumer are included in process memory. This is not isolated library heap
profiling.

## Evidence and limitations

Each output retains:

- `report.json` and `report.md`: machine, driver, power plan, source hashes,
  executable identity, settings, phase summaries and all completed/failed cases.
- Per-case `resources.json`: raw job, process and PDH samples with timestamps.
- `worker/*-frames.csv`: every observed frame's timing, written after measurement.
- `worker/report.json`, `events.json`, `config.json` and `stderr.txt`: native
  results, phase boundaries, exact workload configuration and errors.
- `adapter-nvidia.json`: separate whole-adapter telemetry, when available.

Freeze a completed matrix into a compact, shareable baseline with repetition
medians and ranges:

```powershell
python -B scripts/summarize-benchmark.py artifacts/benchmark-new/report.json benchmarks/baselines/new-baseline.json
```

The summarizer refuses incomplete matrices and existing output files. Its JSON
records metric definitions, primary-source identities, configuration, hardware,
source hashes and the raw report hash. The adjacent Markdown table reports the
median of each repetition's metric. It does not pool frame-time percentiles.

Run workloads sequentially, with builds, verification and other benchmark
instances finished. Keep hardware, power settings, source packages, renderer
settings and omissions comparable. Repeat runs; report spread as well as a
central value. The controller continues after a failed case, retains its reason,
and exits unsuccessfully if the matrix is incomplete.

Retry only failed cases with the pinned original binary:

```powershell
python -B scripts/benchmark.py --output artifacts/benchmark-new --retry-failed
```

Each retry appends a new case directory and preserves the preceding report and
failure history. The compact baseline retains that history too. A resource
sampling gap above two seconds invalidates a run; interrupted timing cannot be
silently averaged into the baseline.

Uncapped completion rate includes public sampling, worker synchronization,
driver submission, completion polling and this consumer's wakeups. It is not a
theoretical GPU limit, a monitor refresh measurement, or the rate of distinct
video frames. Low-rate source animation can repeat while procedural effects
continue to move. Submit-to-ready is not a GPU timestamp query.

These initial runs do not measure swap-chain presentation, texture export or
browser transport, host input/scroll latency, integrated-GPU performance,
thermal equilibrium, or long-term leaks. Paused/released comparisons and short
growth samples are diagnostics, not proof of a memory plateau. The production
renderer and its resource/queue limits are unchanged by the benchmark harness.

## Browser consumer responsiveness

After native benchmark processes finish, compare the existing browser consumer
paused and playing:

```powershell
node tools/browser-test/benchmark-responsiveness.mjs --scene 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3255418892/scene.pkg' --engine-assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets' --output artifacts/browser-responsiveness-new
```

This uses the existing debug browser worker and an isolated headless Chrome.
Four alternating intervals each type 40 characters into a focused input at
200 ms intervals. They record Playwright/CDP keyboard round-trip time,
event-to-handler delay, animation-frame gaps and long tasks. Both paused
intervals must keep the native publication count unchanged, both playing
intervals must advance, and all 160 input events must be delivered. Raw samples
and owned cleanup are retained in `responsiveness.json` and per-interval files.

The test measures this lightweight browser host with debug PNG transport, not
T3, a production GPU-sharing adapter, device-to-display latency or native
presentation. Its numbers are separate from release native throughput and
resource costs. Unavailable long-task support remains explicit.
