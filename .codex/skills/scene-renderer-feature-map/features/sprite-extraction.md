# Sprite extraction

## Status

Verified on 10 October 2026, Windows x64 and Rust 1.91, through the [hardware/native gate](../../../../artifacts/20261010-151159-dd476da1/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-151340-13047ca2/manifest.json). Background sprite preparation is also verified. Async scene loading, prefetch and live sprite playback remain planned.

## Consumer outcome

Select a frame by cumulative authored durations and modulo looping. Retain only its extracted pixels or enclosing BC3 blocks in the returned sample. Repeat the current selection without another copy, decode or upload. Keep one atlas and one extracted sample per stream.

## How to reach it

Use TextureDecoder::sprite_stream on an asset worker or tool, then SpriteStream::sample(time). Submit the returned region Arc through the existing region or ImageLayer API. Read [the extraction guide](../../../../docs/sprite-extraction.md) before changing timing, residency or integration. The stream API is blocking and must stay off the host UI thread. Use [SpriteWorker](../../../../docs/sprite-worker.md) for background asset opening/extraction, nonblocking time requests and result polling. Stop submitting requests to settle and idle; join shutdown outside UI work.

## Drive and observe

Run scripts/verify.ps1; use -Software -SkipNative without an interactive desktop. Public tests check variable durations, exact boundaries, maximum time, loop return, page residency, retained samples, locking and admission before decoding. The texture consumer compares every generated extracted GPU pixel with independent fixture pixels and checks same-Arc upload reuse.

Worker tests hold preparation at a barrier while 1,000 requests coalesce, discard superseded errors, cancel pending work on joined shutdown and report panic. Public tests verify async error recovery, GPU pixels, an undrained capacity-one notification channel and retained samples. The texture consumer also checks worker-prepared generated frames and actual bird/character page-boundary frames. Inspect generated_worker / target_worker reports and worker captures. The separate native --extracted-fixture instance prepares through the worker before window creation and verifies prepared-frame capture, resize, idle, switching to diagnostic motion and owned exit. With paired local scene/engine roots, target checks compare six selected bird/character frames against whole-atlas GPU crops, including page transition and loop boundaries. Inspect target-extracted captures and textures/report.json.

## Evidence and limits

Record source payloads, decode/extract counters, cache bytes and blocking sample time. The recorded target run uploaded 13,824,000 bytes per bird frame and 3,462,912 per character frame. These payloads exclude driver overhead; the CPU still decodes a full atlas.

Fixed-time equality proves extraction equivalence, not Wallpaper Engine fidelity, smooth playback, GPU timing or host responsiveness. Point sampling is checked; linear filtering is unverified. Caller-held samples and independent streams are outside the per-stream bound. The selected-object snapshot route still uses whole atlases. Evidence is ignored; rerun in other checkouts.
