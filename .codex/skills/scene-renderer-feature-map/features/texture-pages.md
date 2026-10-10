# TEX page drawing

## Status

Verified on 2026-10-10 with Rust 1.91 on Windows x64. The [hardware/native receipt](../../../../artifacts/20261010-125557-1446d018/manifest.json) includes real target pages and native texture presentation; the [WARP/headless receipt](../../../../artifacts/20261010-125734-400ed348/manifest.json) explicitly skips native scenarios and proprietary content. These ignored receipts may be absent elsewhere; rerun there. Authored layers, effects and sprite playback remain planned.

## Consumer outcome

A tool/asset worker decodes selected TEX base pages through `TextureDecoder`. A host enqueues an immutable page Arc through `Renderer::render_texture_page` and consumes a completed shared GPU frame. Uploads reuse the same page identity; resize preserves its content and retained frames stay valid.

## How to reach it

Read [the TEX page guide](../../../../docs/texture-pages.md) for blocking boundaries, memory limits and diagnostic color/alpha conventions. Run `pwsh -File ./scripts/verify.ps1`. Pass paired scene/engine roots to add actual target pages, or use `-Software -SkipNative` without an interactive desktop. `cargo run --locked --example native-viewer -- --texture-fixture` shows the generated PNG/LZ4 fixture.

## Drive and observe

The headless consumer checks eight generated raw/PNG/LZ4 RGBA8/R8/BC3 cases, integer pixel-centre sampling, straight-alpha composition, odd-size resize, retained pixels and upload reuse. Native verification captures the independent-device swap-chain path, resizes, checks idle, switches to motion, then verifies owned-process exit. Actual target checks select nine pages, including first/last sprite atlases. They do not play or place those frames in the scene.

## Evidence and limits

Inspect `textures/index.html`, PNGs and `report.json`, plus `native-texture/report.json`, captures and transcript. Record source-page bytes/uploads separately from output counters. The largest real decoded page was 248,832,000 bytes. GPU pixel expectations for generated fixtures are independent of decoded output. Real RGBA draws are checked against decoded page bytes; target R8/BC3 captures have no independent Wallpaper Engine reference yet. No authored scene fidelity, aggregate caller memory budget, live streaming, GPU duration or host latency claim is established.
