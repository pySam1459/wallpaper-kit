# Motion verification

Agents can test motion through explicit playback times, independent pixel expectations, and timestamped live observations. A model can inspect a contact sheet for appearance, while executable checks establish the fixture's positions, phase, and loop boundary. This fixture does not implement Workshop scenes or video decoding.

## Run the checks

From the checkout containing the change:

```powershell
pwsh -File ./scripts/verify.ps1
```

This runs the source checks, blank consumer, `motion-checks`, and an isolated native viewer. It records source/executable hashes, hardware and driver information in the successful run's manifest. Each run gets a unique ignored directory under `artifacts/`. Failures retain available captures, transcripts, and `failure.json`. The controller stops only the process it launched.

Without an interactive desktop, use `-Software -SkipNative`. WARP results establish software rendering, and the report marks native presentation as skipped. To run just the headless motion scenario:

```powershell
cargo run --locked --example motion-checks
cargo run --locked --example motion-checks -- --software
```

These standalone commands retain evidence but do not create the full gate's build receipt. A build or an old receipt alone is insufficient proof of a changed runtime.

## Fixture contract

`Renderer::render_fixture_at(Duration)` schedules an immutable frame through the public worker API and returns its request ID. It installs no playback timer. `Frame::fixture_time()` records the requested time. Resize preserves content and time. The consumer owns the live clock and consumes the latest completed frame.

The output uses opaque SDR RGBA8 pixels. The background is `[16,24,32,255]`; the bar is `[224,160,64,255]`. For physical width `w` and height `h`:

```text
bar_width = max(floor(w / 8), 1)
step = floor((time_ms % 2000) / 50)
left = floor((w - bar_width) * step / 39)
top = floor(h / 4)
right = left + bar_width
bottom = max(h - top, top + 1)
```

The bar occupies `[left,right) × [top,bottom)`. It reaches the right edge at 1950 ms and wraps to the left at 2000 ms. The deliberately discrete 50 ms steps give exact pixel expectations; they are not a visual smoothness benchmark. At 320 × 180 and 1000 ms, the expected bar is x=143..182, y=45..134.

The verifier computes expectations in example-only code from this contract, without calling the library's position helper. A literal midpoint unit test checks that oracle and proves pixel corruption fails. Update expectations only when the fixture contract intentionally changes, and inspect any changed baseline.

## Evidence to inspect

| Artifact | What it establishes |
| --- | --- |
| `motion/checkpoints.json` and ten `frame-*.png` files | Exact GPU pixels at 0, 49, 50, 500, 1000, 1950, 1999, 2000, 2050 and 4000 ms, with request/time metadata |
| `motion/index.html` and `contact-sheet.png` | Labeled checkpoint inspection and a compact visual comparison |
| `motion/resized.png` | Nonaligned 317 × 193 output preserves time and composition |
| `motion/verifier-self-tests.json` | Captured start/midpoint/end frames presented to the oracle at incorrect times must fail for frozen motion, wrong speed and a broken loop; a synthetic 120 ms stall must register |
| `motion/timeline.json` and `report.json` | Separate warmed live run with 125 absolute slots at 30 requests/second, with no capture or readback |
| `native/motion-*.png`, `transcript.json` and `report.json` | Controlled fixture pixels copied through a separate consumer device into its swap-chain back buffer |
| `native/motion-timeline.json` | Live playback spanning at least two loops, request times and presentation-call return times; history is capped at 2048 entries |

Headless timing records request-to-ready p50/p95/p99/max, skipped slots, missed deadlines, longest ready gap, and maximum `try_frame` call duration. A deadline is the end of that scheduled 33.33 ms slot. Late slots are skipped instead of slowing the authored clock. All measurements are milliseconds. Capture and warmup time are excluded.

`status: passed` means the correctness/control scenario passed. Timing numbers remain diagnostic, even when deadlines are missed. Inspect those numbers and report regressions; do not turn an aggregate success status into a production performance claim. The verifier fault cases test the checks themselves, not fault recovery in the renderer.

## Native controls

After the viewer's `ready` event, send line-delimited JSON over stdin and wait for the matching reply before sending another controlled frame command:

```json
{"id":1,"command":"fixture_at","time_ms":1000}
{"id":2,"command":"capture","name":"midpoint.png"}
{"id":3,"command":"play_fixture"}
{"id":4,"command":"pause_fixture"}
{"id":5,"command":"stop"}
```

`fixture_at` pauses and replies after presenting the requested time. `play_fixture` resumes from the controlled or paused clock time; repeated play is idempotent. Pause stops scheduling, while an in-flight frame may still complete. The automated check settles that work with a controlled step and checks that the frame ID stays unchanged while paused. The live interval has no explicit readback. Commands do not mutate private renderer state.

## Limits and next measurements

Ready time is when a consumer observes the completed frame. It includes scheduling and synchronization, so it is not GPU duration. Presentation-call return time is not display time. Captures check GPU content and the native copy path, not final desktop composition or tearing.

The first hardware run exposed approximately 390 ms completion waits and failed the native two-loop check. Allowing `GetData` to submit work while polling reduced measured request-to-ready latency to around 2 ms on that adapter. Keep the failing evidence when comparing runs. Microsoft's [GetData flags documentation](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_async_getdata_flag) describes the submission risk with `DONOTFLUSH`; the explicit completion wait remains outside UI acquisition.

The next measurement layers are D3D11 timestamp/disjoint queries for GPU duration, a Windows presentation trace for displayed cadence, and a real host with input/scroll latency compared with rendering disabled. Video needs known frame timestamps, seek/loop fixtures and audio/video synchronization tests once decoding exists. Scene verification needs reference output from Wallpaper Engine, with fixed time/seed where supported and tolerances for nondeterminism.

These layers are planned. Current checks do not measure sustained memory growth, integrated-GPU contention, minimized lifecycle, production frame pooling, host input latency, scene fidelity or video playback. Immutable allocation per frame remains experimental. Compare matching workloads and configurations over repeated runs before claiming an improvement; keep correctness gates separate from performance scores.
