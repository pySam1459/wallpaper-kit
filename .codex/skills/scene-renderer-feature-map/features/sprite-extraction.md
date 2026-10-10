# Sprite extraction

## Status

Verified on 10 October 2026, Windows x64 and Rust 1.91, through the [hardware/native gate](../../../../artifacts/20261010-145103-eaacbb0e/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-145543-4482d6a8/manifest.json). Async loading, prefetch and live sprite playback remain planned.

## Consumer outcome

Select a frame by cumulative authored durations and modulo looping. Retain only its extracted pixels or enclosing BC3 blocks in the returned sample. Repeat the current selection without another copy, decode or upload. Keep one atlas and one extracted sample per stream.

## How to reach it

Use TextureDecoder::sprite_stream on an asset worker or tool, then SpriteStream::sample(time). Submit the returned region Arc through the existing region or ImageLayer API. Read [the extraction guide](../../../../docs/sprite-extraction.md) before changing timing, residency or integration. The API is blocking and must stay off the host UI thread.

## Drive and observe

Run scripts/verify.ps1; use -Software -SkipNative without an interactive desktop. Public tests check variable durations, exact boundaries, maximum time, loop return, page residency, retained samples, locking and admission before decoding. The texture consumer compares every generated extracted GPU pixel with independent fixture pixels and checks same-Arc upload reuse.

The separate native --extracted-fixture instance verifies prepared-frame capture, resize, idle, switching to diagnostic motion and owned exit. With paired local scene/engine roots, target checks compare six selected bird/character frames against whole-atlas GPU crops, including page transition and loop boundaries. Inspect target-extracted captures and textures/report.json.

## Evidence and limits

Record source payloads, decode/extract counters, cache bytes and blocking sample time. The recorded target run uploaded 13,824,000 bytes per bird frame and 3,462,912 per character frame. These payloads exclude driver overhead; the CPU still decodes a full atlas.

Fixed-time equality proves extraction equivalence, not Wallpaper Engine fidelity, smooth playback, GPU timing or host responsiveness. Point sampling is checked; linear filtering is unverified. Caller-held samples and independent streams are outside the per-stream bound. The selected-object snapshot route still uses whole atlases. Evidence is ignored; rerun in other checkouts.
