# Sprite extraction

## Status

Verified 26 October: freezing playback asynchronously releases extra pager
windows and the decoded atlas while the selected sample remains valid for
still resize. Play/seek resumes preparation. Public contracts observe an
unselected frame's final reference disappearing, preserve frozen pixels,
trim/rebuild GPU resources and redraw at a new size. The
[software gate](../../../../artifacts/20261026-000945-f4bc23d0/manifest.json)
and hardware Yuta browser idle/resume check pass. Warm pause still retains
caches. See [still-mode resources](../../../../docs/idle-performance-2026-10-25.md)
for actual process-memory measurements, not just payload counters.

Bounded prefetch windows are verified on 25 October 2026 through the
[required WARP/headless gate](../../../../artifacts/20261025-193849-43de7d6b/manifest.json)
and [release residency/readiness comparison](../../../../docs/performance-climb-2026-10-25.md).
Each ready window holds at most 64 MiB, with at most two retained. A decoded
atlas remains resident while another window needs it. Generated public tests
check GPU pixels through window boundaries, seek/wrap, one-decode reuse,
payload bounds and retained samples after shutdown. The final 35-second Yuta
profile covers its full loop with zero post-warmup source-unavailable samples
for both sprites. A smaller 32 MiB candidate had four misses and was rejected.
`reserved_work_bytes` includes an idle retained atlas; it does not imply CPU
activity. Read the [prefetch guide](../../../../docs/sprite-prefetch.md) for
ownership, admission and measurement limits.
Hardware browser seek/play/pause/resize and cleanup also pass. The 65-scene
collection sweep preserves every fixed-time pixel hash and complete playback
report; see the comparison for raw receipts. Native-window checks are skipped.

Verified on 10 October 2026, Windows x64 and Rust 1.91, through the [hardware/native gate](../../../../artifacts/20261010-154436-fa213d88/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-154911-c76293fe/manifest.json). Background sprite preparation is also verified. Batch prefetch is verified, including controlled target pixels and separate two-loop headless readiness. The release run delivered 960 frames per asset without unavailable polls or skipped frames; debug birds skipped three frames. See the [recorded prefetch results](../../../../docs/sprite-prefetch.md#recorded-results). Async scene loading and native live sprite playback remain planned.

## Consumer outcome

Select a frame by cumulative authored durations and modulo looping. Retain only its extracted pixels or enclosing BC3 blocks in the returned sample. Repeat the current selection without another copy, decode or upload. Keep one atlas and one extracted sample per stream.

## How to reach it

Use TextureDecoder::sprite_stream on an asset worker or tool, then SpriteStream::sample(time). Submit the returned region Arc through the existing region or ImageLayer API. Read [the extraction guide](../../../../docs/sprite-extraction.md) before changing timing, residency or integration. The stream API is blocking and must stay off the host UI thread. Use [SpriteWorker](../../../../docs/sprite-worker.md) for background asset opening/extraction, nonblocking time requests and result polling. Stop submitting requests to settle and idle; join shutdown outside UI work. Use [SpritePager](../../../../docs/sprite-prefetch.md) to prepare current/next batches while nonblocking time selection reads completed frames.

## Drive and observe

Run scripts/verify.ps1; use -Software -SkipNative without an interactive desktop. Public tests check variable durations, exact boundaries, maximum time, loop return, page residency, retained samples, locking and admission before decoding. The texture consumer compares every generated extracted GPU pixel with independent fixture pixels and checks same-Arc upload reuse.

Worker tests hold preparation at a barrier while 1,000 requests coalesce, discard superseded errors, cancel pending work on joined shutdown and report panic. Public tests verify async error recovery, GPU pixels, an undrained capacity-one notification channel and retained samples. The texture consumer also checks worker-prepared generated frames and actual bird/character page-boundary frames. Inspect generated_worker / target_worker reports and worker captures. The separate native --extracted-fixture instance prepares through the worker before window creation and verifies prepared-frame capture, resize, idle, switching to diagnostic motion and owned exit. With paired local scene/engine roots, target checks compare six selected bird/character frames against whole-atlas GPU crops, including page transition and loop boundaries. Inspect target-extracted captures and textures/report.json. Batch checks add generated_paged / target_paged reports, target-paged captures, separate two-loop headless GPU-readiness telemetry and the native --paged-fixture route. Inspect native-paged/sprite-prefetch.json and its native report; this fixture prepares and joins before window creation.

## Evidence and limits

Record source payloads, decode/extract counters, cache bytes and blocking sample time. The recorded target run uploaded 13,824,000 bytes per bird frame and 3,462,912 per character frame. These payloads exclude driver overhead; the CPU still decodes a full atlas.

Fixed-time equality proves extraction equivalence, not Wallpaper Engine fidelity, smooth playback, GPU timing or host responsiveness. Point sampling is checked; linear filtering is unverified. Caller-held samples and independent streams are outside the per-stream bound. The selected-object snapshot route still uses whole atlases. Evidence is ignored; rerun in other checkouts.
