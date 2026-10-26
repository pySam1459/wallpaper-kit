# Still-mode resources and expensive features

This step follows the [optimization log](performance-progress.md). The consumer
owns the focus/chat policy. The renderer has no dependency on window or browser
events and cannot infer whether its output is visible.

## Idle behavior and implementation

Stopping render requests already leaves the core near zero measured CPU/GPU
usage. Warm pause retains substantial memory for fast reuse. Long still/hidden
states can now release extra resources while preserving the displayed image:

```rust
playback.freeze(&last_presented_sample);
renderer.trim_memory()?;
// Keep the displayed Frame or copied host texture. Stop requesting new frames.
```

`freeze` holds the exact sample and asks source workers to discard extra sprite
windows/decoded atlases and close owned video decoders. Active codec work may
finish before cleanup. Metadata and source identity remain available; workers
sleep until sampling resumes. Large buffers and decoder cleanup are released
on their workers rather than the caller.

`Renderer::trim_memory` is a coalesced asynchronous request. It waits for pending
GPU work, releases source/effect targets and unleased spare outputs, and keeps
the latest frame plus every consumer-held frame. New render requests supersede
a pending trim. Shader objects remain available for reuse. The worker unbinds
state, releases resources, flushes and calls
[IDXGIDevice3::Trim](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_3/nf-dxgi1_3-idxgidevice3-trim)
to retire deferred driver allocations. Driver trimming is reserved for idle
transitions rather than repeated active-frame work.

Resume with `play`, then poll for ready samples as usual. Keep showing the
retained GPU frame while sources and GPU resources prepare again. A still resize
can redraw from the retained sample without changing its time. Cold decoder,
atlas and GPU resource preparation is the tradeoff for lower idle memory;
short pauses can use the existing warm `pause` route instead.

## Recorded step 3 results

The [warm snapshot](../benchmarks/results/2026-10-25-idle-warm.json),
[trimmed snapshot](../benchmarks/results/2026-10-25-idle-trimmed.json) and
[comparison](../benchmarks/results/2026-10-25-idle-comparison.json) retain two
4K repetitions per mode, five-second idle intervals, eight-second active
intervals and two-second warmups. Both modes use the same pinned candidate
binary. `trim_paused` is the intentional policy difference and is recorded in
the comparison. Active rendering keeps linear sampling, animation and VFX.

Hardware is the Ryzen 9 9950X3D, 32 logical processors, RTX 5080, Windows 11
build 26200, driver 32.0.16.1664 and Balanced power plan used by the baseline.
Values below are medians of the post-playback `paused_after` observations:

| Workload, 3840×2160 | Warm private MiB | Trimmed private MiB | Warm dedicated GPU MiB | Trimmed dedicated GPU MiB |
| --- | ---: | ---: | ---: | ---: |
| Yuta sprites | 776.0 | 177.3 | 261.2 | 76.8 |
| Gojo composition | 376.0 | 148.2 | 337.2 | 76.9 |
| Tram video | 349.0 | 93.8 | 126.8 | 76.8 |

This reduces post-playback private commit by about 77%, 61% and 73%
respectively. Settled idle CPU/GPU activity remains near the measurement floor;
there is no claimed speedup below zero. Renderer counters show one retained
output and zero GPU source/effect payload after trimming. Consumer-held frames,
CPU pages needed for still resize, device overhead and driver allocations are
outside those payload counters. Dedicated GPU memory is measured separately.

Step 3a's first prototype released core buffers but left substantial driver
memory resident. Its [pilot report](../artifacts/idle-trim-pilot-20261025/report.json)
is preserved. Step 3b adds explicit state clearing and driver trimming; the
paired snapshots above are the selected result. Do not substitute payload-byte
counters for actual process/VRAM measurements.

## Feature-cost audit

The [feature-cost record](../benchmarks/results/2026-10-25-feature-cost.json)
compares five full workloads against the existing VFX-disabled consumer policy.
Each mode has an eight-second 4K profile. These are deliberately different
rendered contents, used to identify cost rather than claim a quality-preserving
optimization. GPU timestamps measure the whole frame interval, including
pipeline/scheduling gaps, rather than isolated effects.

| Workload | Full GPU interval, ms | VFX-off interval, ms | Full CPU, machine % | VFX-off CPU, machine % |
| --- | ---: | ---: | ---: | ---: |
| Panda | 0.220 | 0.127 | below measurement floor | 0.082 |
| Gojo particles | 0.470 | 0.069 | 0.195 | 0.050 |
| Gojo composition | 1.691 | 0.130 | 0.095 | 0.025 |
| Yuta sprites/effects | 0.478 | 0.221 | 0.592 | 0.435 |
| Tram video | 0.094 | 0.089 | 2.274 | 2.117 |

Gojo's full-screen composition chain contains Water Ripple and five precise
blurs. It is the clearest GPU target in this sample. Video remains the largest
CPU cost even with VFX disabled, making decoder/transfer profiling more relevant
than removing its small GPU effect cost. Sprite residency remains the largest
active-memory target. Large particle scenes still spend time constructing sample
geometry even after the earlier state-binding optimization.

Short CPU observations near zero are coarse and noisy. Panda's CPU pair is not
evidence that removing VFX increases CPU cost. These single-pair profiles rank
large workload differences; repeat targeted measurements before changing an
individual feature. No effects, particle counts, resolution or quality settings
are automatically reduced by this step.

## Verification and consumer adoption

Public contracts verify retained GPU pixels, late frame leases, pending trim,
still resize, resource reconstruction, sprite-frame eviction, video-frame
eviction and decoder restart. The required software/headless gate passes at
[this receipt](../artifacts/20261026-000945-f4bc23d0/manifest.json); native windows
are skipped.

The browser example stops polling on blur/hidden callbacks, freezes its last
sample and trims resources. Focus resumes only requested playback. Explicit
pause survives focus changes. Hardware checks pass for
[Yuta](../artifacts/browser-idle-yuta-v2-20261025/idle.json) and
[Tram](../artifacts/browser-idle-tram-20261025/idle.json), including still resize
and owned cleanup. Focus callbacks are synthesized in headless Chrome; actual
desktop focus delivery is unverified. Activating another headless tab did not
deliver the expected callback in the first
[preserved test](../artifacts/browser-idle-yuta-20261025/idle.json).

The [generated browser route](../artifacts/browser-idle-fixture-20261025/report.json)
passes independent pixels, play/pause/seek/resize, errors and Chrome DevTools MCP
checks. This PNG route does not establish production host input latency,
zero-copy sharing or monitor presentation. A consumer must stop rendering to
benefit; `pause` does not prevent an application from explicitly requesting
more GPU draws. Production adapters should call the same freeze/trim lifecycle
and release unneeded frame leases and their own export caches.

The [original-settings final matrix](../benchmarks/results/2026-10-26-vs-initial.md)
completed after midnight on 26 October. All 28 initial PNG hashes and complete
playback reports match the initial baseline. The campaign-start date is kept
in this step's paired measurement filenames. See the [log](performance-progress.md)
for the invalidated benchmark ownership experiment and the corrected final
report. Its warmup-first-frame metric measures a seek-to-zero first draw after
pause, not production focus-resume latency.

The final [collection sweep](../artifacts/workshop-support-idle-final-20261026/report.json)
loads 65/65 with every fixed-time pixel hash and complete playback report
unchanged from the preceding milestone. Existing omissions remain explicit.
The final [browser responsiveness check](../artifacts/browser-idle-responsiveness-20261026/responsiveness.json)
delivers 160/160 inputs without observed long tasks. Paused intervals publish
no frames; playing intervals deliver three native frames each through debug
PNG transport. This does not establish production host latency. A compact
[verification record](../benchmarks/results/2026-10-26-idle-verification.json)
retains receipt hashes, counters, checks and limitations outside ignored artifacts.
