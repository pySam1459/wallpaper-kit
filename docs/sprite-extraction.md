# Deterministic sprite extraction

`SpriteStream` selects a TEX sprite frame at a caller-supplied time and extracts its payload for the existing Direct3D 11 region or image API. This is the first stage-4 component. [Background sprite preparation](sprite-worker.md) now runs this blocking work on an asset thread. Scene scheduling, prefetch and live sprite playback remain open.

## Consumer path

Create a stream with `TextureDecoder::sprite_stream(asset)`. Opening validates and indexes the texture once without decoding pixels. The stream owns an independent read-locked asset handle; it remains usable after the decoder is dropped. Use it on an asset worker or diagnostic tool: opening and `sample(time)` perform blocking I/O and decoding.

`frames()` exposes checked authored metadata. `duration()` sums each frame's duration rounded to nanoseconds. `frame_index_at(time)` selects by cumulative durations without I/O: an exact frame boundary selects the next frame, and an exact total duration loops to frame zero. Arbitrarily large `Duration` values use modulo arithmetic. Durations need not be uniform.

`sample(time)` returns a `SpriteSample` containing the authored frame, index and extracted region Arc. Pass `sample.region().clone()` to `Renderer::render_texture_region` or use it to construct an `ImageLayer`. The existing completed-frame ownership and nonblocking consumption contract still applies. Repeated selection of the current frame returns the same region Arc without copying or decoding. Shared instances can use that Arc to reuse its GPU upload. Caller-retained samples remain valid after another sample or stream destruction.

RGBA8 and R8 copy exactly the rectangle's rows. BC3 copies the enclosing compressed blocks and retains a residual crop, including unaligned rectangles and partial blocks at atlas edges. It does not decompress BC3. The current point-sampling shader draws the authored rectangle from those blocks; linear filtering and edge behavior have not been verified.

## Residency and work

Each stream keeps one decoded atlas page, capped at 256 MiB, and one extracted frame, capped at 64 MiB. Frame size admission precedes atlas decoding. Page changes discard the previous cached atlas before decoding the next. Returning to an evicted page, including a loop, decodes it again. Repeated frames and other frames on the resident page avoid that decode.

`stats()` reports successful `page_decodes`, `frame_extracts`, and current `cached_page_bytes` / `cached_frame_bytes`. These are payload counters, not total memory measurements. Caller-retained samples, separate streams, encoded inputs, codec workspace, GPU resources and driver overhead are outside this per-stream bound. No global memory budget, queue or prefetch exists yet. Full PNG/LZ4 atlases still need CPU decoding; extraction reduces uploaded source payload and the data retained by a sample.

## Verification

```powershell
cargo run --locked --example native-viewer -- --extracted-fixture
pwsh -File ./scripts/verify.ps1
pwsh -File ./scripts/verify.ps1 -Software -SkipNative
```

The required gate runs public-contract tests for variable durations, exact boundaries, `Duration::MAX`, page eviction, retained samples, same-Arc reuse, independent asset ownership and early oversized-frame rejection. Independent generated RGBA8/R8/BC3 pixel oracles include unaligned BC3 rectangles at partial atlas edges. `texture-checks` captures start, boundary, last, loop and second-loop checkpoints for eight formats/storage cases. The native extracted fixture verifies initial/resized back-buffer pixels through the independent consumer device, idle behavior, switching to diagnostic motion and owned-process cleanup. It displays one prepared sprite frame; its motion controls still drive the diagnostic fixture.

Add the paired local scene and engine roots from [TEX verification](texture-pages.md) to compare selected actual bird and character frames. Six checkpoints per asset include the first page transition, middle/last frames and loop return. `textures/report.json` records authored and extracted extents, residual crop, timing, cache counters and GPU source payload. Captured GPU pixels must exactly match the earlier whole-atlas crop captures. This establishes extraction equivalence, not Wallpaper Engine fidelity or smooth animation.

The [hardware/native receipt](../artifacts/20261010-145103-eaacbb0e/manifest.json) and [WARP/headless receipt](../artifacts/20261010-145543-4482d6a8/manifest.json) passed on 10 October 2026, Windows x64 and Rust 1.91. Hardware used an NVIDIA GeForce RTX 5080; WARP skipped native presentation and proprietary assets. All 44 tests, formatting, Clippy, Rustdoc, dependency and consumer checks passed. Native extracted capture, resize and owned exit passed. Captures were inspected. Evidence is ignored and must be regenerated in other checkouts.

At 960x540, actual bird samples uploaded 13,824,000 payload bytes instead of a 248,832,000-byte whole atlas (18 times smaller). Character samples uploaded 3,462,912 bytes instead of 55,259,616 (about 16 times smaller). Debug blocking page-change samples took approximately 0.33–1.06 seconds for birds and 0.21–0.23 seconds for the character; resident-page frame copies took about 1.54 ms and 0.45 ms. These single-run preparation observations exclude upload/capture, use diagnostic readback and do not establish steady playback or host responsiveness. They motivate worker scheduling and prefetch before live acceptance.

The selected-object snapshot loader still uses whole atlas pages and retains its existing source budget rejection. It does not automatically use this stream. Full two-loop sprite playback, async scene loading, aggregate residency, Wallpaper Engine reference comparison, effects and host-impact measurements remain stage-4 or later work.
