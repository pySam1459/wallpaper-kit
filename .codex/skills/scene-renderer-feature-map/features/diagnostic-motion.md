# Diagnostic motion

## Status

Verified on 2026-10-10 on Windows x64 with Rust 1.91. The [hardware/native manifest](../../../../artifacts/20261010-000732-ac8c713b/manifest.json) records the NVIDIA GeForce RTX 5080 build; the [WARP/headless manifest](../../../../artifacts/20261010-000752-c46add9f/manifest.json) records native verification as skipped. These ignored local files may be absent in another checkout; rerun there. Workshop scenes, video and production animation remain unavailable.

## Consumer outcome

Consumers request known playback times through the library, observe actual GPU pixels and exercise the separate native presentation path. Agents get a repeatable way to detect frozen motion, wrong phase/speed, broken loops, and timing stalls without asking a model to infer cadence from screenshots.

## How to reach it

Read [motion verification](../../../../docs/motion-verification.md) for the fixture contract, metric definitions, supported JSON controls and limits. Run `pwsh -File ./scripts/verify.ps1`; use `-Software -SkipNative` without an interactive desktop. `cargo run --locked --example motion-checks` runs only headless motion evidence.

## Drive and observe

The runner checks ten controlled GPU frames, nonaligned resize and independent oracle fault cases. Its separate readback-free live run records scheduled slots, skipped/missed deadlines, request-to-ready percentiles and acquisition call durations. The native controller checks two controlled back-buffer captures, plays across at least two loops, pauses, settles in-flight work, checks paused idle, and verifies process exit.

## Evidence and limits

Inspect labeled `motion/index.html`, captures, checkpoints, self-tests, raw timelines, reports and the build manifest. Success in correctness does not imply timing acceptance. Ready times are consumer observations; native samples end when the presentation call returns. Neither measures GPU duration or displayed cadence. The history is capped; omitted samples are counted. Immutable frames still allocate per request. Real host responsiveness, production pooling, minimized behavior and sustained resource budgets remain unverified.

The [hardware motion report](../../../../artifacts/20261010-000732-ac8c713b/motion/report.json) recorded 125 completed slots, zero skipped/missed deadlines and 2.067 ms maximum request-to-ready latency at 320 × 180. The [native report](../../../../artifacts/20261010-000732-ac8c713b/native/report.json) records 126 live samples, two-loop coverage, paused idle and owned-process exit. The [first failing run](../../../../artifacts/20261009-235938-a2dd5b41/failure.json) retains the completion-query delay that these checks exposed. These are diagnostic samples, not scene or host performance scores.
