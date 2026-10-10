# Bounded sprite prefetch

`SpritePager` prepares the current and next contiguous authored page runs on one asset thread. Each ready batch contains all extracted frames for that run. Playback can select those frames while the worker decodes the next atlas. The worker releases each atlas before starting another batch.

This stage-4 component extends [fixed-time extraction](sprite-extraction.md) and [background preparation](sprite-worker.md). The renderer still lacks a full-scene clock, shared asset coordinator, scene replacement and effects playback.

## Consumer contract

Create `SpritePager::new(source, asset)` outside repeated updates. It starts background opening, hashing, metadata validation and initial preparation without file I/O on the caller. The capacity-one notification receiver coalesces wakeups. Ignoring or dropping it cannot block the worker.

Call `try_sample_at(time)` from the host's own clock. It selects by cumulative authored durations and modulo looping, replaces time demand and returns a ready `SpriteSample` through nonblocking metadata access. `None` means unavailable or contended; keep the previous completed GPU frame and retry. A ready selection clones an Arc without copying pixels. Pass the region to the existing region or `ImageLayer` API. Repeated resident selections share the Arc.

The asset worker prepares current demand first, then the next page run. A seek can interrupt extraction between frames. An active codec may finish, but relevance is checked under the metadata lock before publishing either its batch or its error. Obsolete results and errors are discarded. Errors for needed work or asset initialization stop the pager. The first sample poll returns the contextual error; later polls report a stopped worker. Recreate the pager to retry.

Hold time to pause. Outstanding current/next preparation settles, then the worker waits on a condition variable without periodic work. `shutdown()` and Drop cancel demand and join. Active file/codec work may finish first. Join outside latency-critical updates. Retained samples survive eviction and shutdown. Explicit shutdown reports a worker panic.

## Admission and ownership

Metadata admission checks every authored run before decoding pixels:

| Payload | Limit |
| --- | --- |
| One extracted frame | 64 MiB |
| One decoded atlas | 256 MiB |
| One complete batch | 256 MiB |
| Ready batches | Two, at most 512 MiB |
| Ready payloads plus reserved preparation | 768 MiB |

Preparation reserves the larger of atlas plus complete batch or atlas plus encoded inputs and codec workspace ceilings. PNG reserves three 64 MiB ceilings for stored input, optional LZ4 expansion and decoder workspace. PNG channel expansion uses one final RGBA allocation and releases decoder workspace before extraction. Raw LZ4 reserves its stored-input ceiling plus the final page. Obsolete batches drop outside metadata locks and before new decoding.

`try_stats()` reports decodes, extractions, prepared/discarded batches, ready bytes, work reservation and peak admitted bytes. `preparing` includes initialization, queued demand and active preparation. `unavailable_polls` excludes metadata contention and is not a displayed-frame miss count.

This cap covers one pager's admitted pixel/input/workspace buffers. Metadata, allocator/codec overhead beyond explicit quotas, separate pagers, caller-retained samples and GPU resources are outside it. It is not the combined scene memory budget. Birds require 248,832,000 bytes per complete batch. Character batches can require 55,406,592 bytes because enclosing BC3 blocks overlap across extracted frames. The combined scene needs a shared asset coordinator before two birds, character, effects and host resource usage can meet the scene budget.

## Verification

```powershell
pwsh -File ./scripts/verify.ps1
pwsh -File ./scripts/verify.ps1 -Software -SkipNative
cargo run --locked --example native-viewer -- --paged-fixture
pwsh -File ./scripts/verify.ps1 -ScenePath 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg' -EngineAssets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'
```

Public contracts check all eight generated storage/format cases against independent GPU pixel expectations, variable durations, page transitions, maximum time, looping, batch reuse, eviction, retained samples and pre-decode budget rejection. A deterministic completion-state regression checks obsolete and needed decode errors under changed demand, plus shutdown cancellation.

`textures/report.json` records `generated_paged` and, with paired local asset roots, `target_paged`. Controlled target captures check first, page-boundary, middle, last and loop samples against earlier whole-atlas GPU crops. These comparisons establish extraction equivalence. Wallpaper Engine fidelity and linear filtering remain unverified.

The target route also runs each inspected sprite for two authored loops at a requested 60 Hz, separately from captures. It uploads changing frames and waits for completed headless GPU output without readback. It records sample availability, skipped authored frames, maximum sample-poll time, GPU-ready wait and consumer tick gap. These debug observations include scheduler and diagnostic waiting overhead. They do not measure GPU duration, native display cadence or host input latency. Zero unavailable polls alone cannot establish smoothness; inspect skipped frames and tick gaps separately. Holding final demand checks unchanged preparation counters over a 100 ms pause interval.

`native-paged` prepares a generated batch before window creation, joins its pager, then verifies retained-frame native capture, resize, idle, diagnostic motion and owned-process exit. Interactive sprite loading and native live sprite playback remain open. Evidence is ignored and must be regenerated in other checkouts.

## Recorded results

The [hardware/native gate](../artifacts/20261010-154436-fa213d88/manifest.json) and [WARP/headless gate](../artifacts/20261010-154911-c76293fe/manifest.json) passed on 10 October 2026, Windows 11 x64, Rust 1.91. All 52 tests, formatting, Clippy, Rustdoc, dependency boundaries and consumer checks passed. Hardware used a Ryzen 9 9950X3D and NVIDIA GeForce RTX 5080. Native pager capture, resize, idle and owned exit passed; WARP skipped native presentation and proprietary target assets. Target and native captures were inspected. Doctor accepted the matching diagnostic receipt and continued to reject full scene readiness.

The separate [release report](../artifacts/20261010-154945-prefetch-release-f2e651b4/report.json) and [build identity](../artifacts/20261010-154945-prefetch-release-f2e651b4/run-identity.json) record the same source, assets and hardware with optimized compilation. Both profiles ran each sprite separately for 57.6 seconds at 960×540, point sampling, straight alpha and a requested 60 Hz consumer tick. Asset animation advances about 16.67 authored frames per second. Timing excludes readback.

| Profile / asset | Rendered frames | Unavailable polls | Skipped authored frames | Maximum sample poll | Maximum GPU-ready wait |
| --- | ---: | ---: | ---: | ---: | ---: |
| Debug / birds | 957 | 12 | 3 | 0.0674 ms | 30.39 ms |
| Debug / character | 960 | 0 | 0 | 0.0816 ms | 27.27 ms |
| Release / birds | 960 | 0 | 0 | 0.0330 ms | 12.56 ms |
| Release / character | 960 | 0 | 0 | 0.0399 ms | 6.08 ms |

Release initialization through two ready batches took about 357 ms for birds and 208 ms for character, including package opening and hashing. Maximum consumer tick gaps were 17.89 and 18.04 ms. Pausing left preparation counters unchanged. Peak admitted buffers were 746,496,000 bytes for birds and 177,775,072 bytes for character in both profiles. These are payload/reservation values, not measured process working sets.

This single release run establishes two-loop headless readiness on the recorded machine. Debug bird preparation had misses. Native sprite cadence, slower hardware, CPU/GPU workload totals, combined scene residency, host responsiveness and Wallpaper Engine fidelity still require evidence. The next scene scheduling slice must share asset preparation between bird instances and coordinate admission across all animated assets.
