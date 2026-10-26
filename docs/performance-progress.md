# Performance optimization log

Keep the initial baseline immutable. Each completed step records what changed,
the workload/settings, before/after evidence, observed ranges, fidelity checks
and rejected experiments. Relative gains from different workloads or run
durations must not be multiplied into a cumulative percentage.

| Step | Change | Observed result | Evidence |
| --- | --- | --- | --- |
| Initial, `c44d6c4` | Add release benchmarks without changing production rendering | 28 cases: seven workloads, 1080p/4K, two repetitions | [Initial baseline](performance-baseline-2026-10-25.md), [snapshot](../benchmarks/baselines/2026-10-25-rtx5080.json) |
| 1, `7cba355` | Bound sprite prefetch windows, balance short tails, reuse an atlas between windows | Yuta 4K paced private commit 912.4 → 733.2 MiB; peak 1184.8 → 853.9 MiB | [Notes](performance-climb-2026-10-25.md), [measurements](../benchmarks/results/2026-10-25-sprite-windows.json) |
| 2, `e470ce3` | Reuse compatible particle bindings, avoid temporary weak references/calendar lookups, remove a composition snapshot copy | Gojo particles 4K: paced CPU 0.309 → 0.119%, GPU 3.577 → 3.227%, uncapped 385.8 → 530.6 fps | [Notes](cpu-gpu-climb-2026-10-25.md), [measurements](../benchmarks/results/2026-10-25-cpu-gpu.json) |
| 3a, rejected pilot | Release idle source/effect buffers without driver trimming | Payload counters fell, but substantial VRAM remained resident; insufficient for the idle-memory goal | [Pilot and decision](idle-performance-2026-10-25.md#recorded-step-3-results) |
| 3b, selected | Freeze source workers, close idle decoders, release spare GPU resources and trim deferred driver allocations | 4K paused private MiB: Yuta 776.0 → 177.3; composition 376.0 → 148.2; video 349.0 → 93.8. Dedicated GPU MiB: 261.2/337.2/126.8 → about 76.8 | [Notes](idle-performance-2026-10-25.md), [paired comparison](../benchmarks/results/2026-10-25-idle-comparison.json) |
| Cumulative, 26 October | Repeat the original 28 cases with matching settings after steps 1–3 | Yuta 4K active private MiB 935.6 → 737.1; Gojo particles CPU 0.384 → 0.176%, uncapped 452.2 → 619.0 fps. Other results are mixed; all values and ranges retained | [Full comparison](../benchmarks/results/2026-10-26-vs-initial.md), [snapshot](../benchmarks/baselines/2026-10-26-after-optimizations.json) |

Step 1 uses two alternating 35-second runs per version. A 32 MiB window trial
had four post-warmup readiness misses and was rejected. The selected 64 MiB
profile had none. CPU/GPU speedups were not established in that step.

Step 2 uses two alternating 20-second ordinary runs per version, with separate
eight-second timestamp profiles. An axis-only blur rewrite did not improve
warmed throughput and was rejected. Composition saves one 4K RGBA8 texture;
its small throughput change does not establish a substantial speedup.

The first two completed steps preserve all 65 wallpapers' fixed-time pixel hashes and
complete playback reports. Native-window/display timing and production host
latency remain unverified. Browser checks use the existing headless Chrome
consumer and document its PNG-transport limits.

Step 3 compares the same candidate binary with warm pause and explicit trimmed
still mode, two repetitions each, eight-second active/five-second idle intervals.
It changes idle residency policy, so it remains separate from the cumulative
matrix's original warm-pause settings. Resume may need to prepare resources;
the retained frame remains usable. The [feature-cost audit](idle-performance-2026-10-25.md#feature-cost-audit)
identifies composition blur chains, video decoding and sprite residency as the
next targets. Feature-disabled probes measure different content and are not
fidelity-preserving optimization results.

## Final comparison and limitations

The [original baseline](../benchmarks/baselines/2026-10-25-rtx5080.json) is
unchanged. The [cumulative JSON](../benchmarks/results/2026-10-26-vs-initial.json)
records all 14 workload/resolution groups, two repetitions each, with the
original eight-second intervals, two-second warmups/idles, VFX and linear
sampling. Configuration differences are empty. All 28 initial PNG hashes and
complete playback reports match. All owned processes exit after each case.

Yuta's 4K active private commit falls 21.2%; Gojo particles' CPU falls 54.1% and
completed throughput rises 36.9%. Composition saves about 34 MiB but its paced
GPU utilization rises slightly, 10.40 → 10.61%. Static 4K GPU utilization rises
1.14 → 4.09%, and static CPU rises 0.047 → 0.123% of the 32-thread machine;
Tram CPU rises 2.06 → 2.15% with overlapping repetition ranges. Those are
recorded regressions/observations, not removed from the table. The next GPU
investigation should repeat the forced-static control in alternating order
before attributing that rise to code or power-state behavior. Static consumers
should stop requesting draws; the matrix deliberately forces them.

Paced tests generally complete about 60 fps. A few runs miss one or two
deadlines; this is not evidence of perfect monitor cadence. Low utilization
counters vary considerably across short runs and GPU power states. Use the
longer alternating step-2 tests to assess its targeted CPU/GPU change. Do not
multiply per-step percentages or call uncapped completed frames displayed fps.

An initial final-matrix attempt was invalidated when review found the benchmark
retained its newly added last sample through the released phase. The consumer
now drops it before shutdown. The partial report and reason remain at
`artifacts/performance-final-matrix-20261025/INVALIDATED.md`; only the corrected
`artifacts/performance-final-matrix-v2-20261025/report.json` feeds the final
snapshot. The earlier paired idle measurements are unaffected by that
benchmark-only released-phase bug.

Step 3's final 65-wallpaper sweep preserves every fixed-time pixel hash and
complete playback report. The required format/Clippy/tests/Rustdoc/boundary and
WARP/headless gate passes, including video decoder restart. Hardware browser
sprite/video idle callbacks, still resize, generated Playwright/Chrome DevTools
checks and 160/160 browser inputs pass. The [verification record](../benchmarks/results/2026-10-26-idle-verification.json)
keeps compact evidence and raw receipt hashes. Native windows, actual desktop
focus delivery and production host latency remain unverified.

## Record the next step

1. Profile a named workload; state the bottleneck and intended change.
2. Preserve the before binary/report. Measure comparable sequential runs and
   retain settings, source hashes, hardware, failures and repetition ranges.
3. Freeze the completed report, then create a comparison with a required note:

```powershell
python -B scripts/summarize-benchmark.py artifacts/step-after/report.json benchmarks/results/step-after.json
python -B scripts/compare-benchmarks.py benchmarks/results/step-before.json benchmarks/results/step-after.json benchmarks/results/step-comparison.json --note 'Change, purpose, workload and tradeoff'
```

The tools refuse to overwrite existing snapshots/comparisons. The comparer
rejects incomplete/failed inputs and changed workload assets, preserves
unavailable metrics and zero-baseline percentages, and lists setting changes.
Its JSON retains input hashes/build identities and before/after ranges; its
Markdown provides the compact table. Append the decision here after pixel,
playback and lifecycle checks. Keep rejected experiments too. Finish with the
original matrix when a batch spans multiple different workloads.
