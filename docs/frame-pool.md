# Bounded frame exchange

The diagnostic renderer reuses at most three shared output textures, including retained textures from earlier sizes. Each output is limited to 64 MiB by `Size`, so worker-owned RGBA8 payloads total at most 192 MiB. Scene assets, consumer resources, readback buffers and driver overhead are outside this bound.

## Consumer contract

`Renderer::try_frame()` retrieves the newest completed `Arc<Frame>` without waiting for GPU completion. Requests coalesce; stale results are discarded. `Frame::id()` remains the request identity. `Frame::sequence()` counts accepted publications, and `fixture_time()` retains the diagnostic playback time.

A retained frame leases its output. If all three outputs are leased, rendering stops until an older frame is released. The renderer itself retains the latest frame: dropping only the consumer's reference to that latest frame does not free its slot. Pending resize and motion requests still coalesce during saturation, and shutdown remains possible. Retained frames stay readable after resize and healthy-device shutdown.

`Renderer::try_output_stats()` provides a nonblocking metadata snapshot. It returns `None` when that metadata is locked. Reports expose owned output count/payload bytes, cumulative allocations/submissions/publications and lease saturation. Source-page bytes and uploads are recorded separately for [TEX drawing](texture-pages.md). These counters do not measure total process or physical GPU memory.

## Worker and GPU ownership

An output transitions between available, submitted and leased states. The worker owns one immediate context, permits one submitted render and keeps one pending request. It polls the completion query before publication and checks controls between polls. The first poll is immediate; pending GPU work uses a requested 1 ms worker sleep. Idle and lease-saturated workers wait for a notification without a recurring timer. Frame release wakes the worker; replaced frames are dropped outside the metadata lock.

Before writing or retiring an output, the producer acquires its keyed mutex with zero timeout. An expired frame lease alone does not establish GPU safety. `Frame::copy_to()` releases the mutex and flushes the consumer context after submitting its copy; reuse acquires that mutex again. A busy mutex defers work, and an abandoned mutex fails the worker. See Microsoft's [AcquireSync contract](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgikeyedmutex-acquiresync) and [GetData completion contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-getdata). Diagnostic readback remains blocking.

## Verification

Run `pwsh -File ./scripts/verify.ps1`. The public tests retain three frames, establish saturation, release one and verify progress and unchanged pixels. They also check repeated motion/resize, allocation reuse and shutdown while saturated. The independent-device test submits 90 WARP consumer copies, drops source leases immediately, requests subsequent renders, then verifies the copied pixels.

The headless runner records 32 same-size requests with allocations plateauing at three. It captures saturation and resumed progress in `headless/report.json`. The native viewer's `inspect` reply includes `output_pool`; motion reports also include these counters.

On 10 October 2026, the [hardware/native gate](../artifacts/20261010-010846-85f196dd/manifest.json) and [WARP/headless gate](../artifacts/20261010-011028-f819e26f/manifest.json) passed. Both completed 125 live diagnostic slots with zero skipped slots or missed deadlines. Hardware request-to-ready p95 was 1.782 ms; WARP p95 was 1.919 ms with a 17.628 ms maximum. These ignored receipts may be absent in another checkout; rerun there.

An initial polling implementation regressed hardware timing because a requested 1 ms condition-variable timeout waited about 16 ms on this runtime. A local timer probe distinguished that behavior from short worker sleeps. The final hardware sample had a 2.160 ms maximum request-to-ready time. These are scheduling diagnostics, not GPU durations, display cadence or proof of host responsiveness.

Workshop rendering, device-loss recovery, sustained total-memory measurement and framework adapters remain unverified. Controlled pixels and separate live timing are described in [motion verification](motion-verification.md).
