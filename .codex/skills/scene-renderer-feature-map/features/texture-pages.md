# TEX page and crop drawing

## Status

Verified on 2026-10-10 with Rust 1.91 on Windows x64. The [hardware/native receipt](../../../../artifacts/20261010-131236-180dff12/manifest.json) includes real target pages, ten sprite-frame crops and native full-page/crop presentation; the [WARP/headless receipt](../../../../artifacts/20261010-131400-ac7c716b/manifest.json) explicitly skips native scenarios and proprietary content. These ignored receipts may be absent elsewhere; rerun there. [Diagnostic image composition](image-composition.md) adds ordered placed layers. Effects and sprite playback remain planned.

## Consumer outcome

A tool/asset worker decodes selected TEX base pages through `TextureDecoder`. A host enqueues an immutable page or checked region Arc through `Renderer::render_texture_page` or `render_texture_region` and consumes a completed shared GPU frame. Uploads reuse the same page identity; resize preserves its content and source crop and retained frames stay valid.

## How to reach it

Read [the TEX page guide](../../../../docs/texture-pages.md) for blocking boundaries, memory limits and diagnostic color/alpha conventions. Run `pwsh -File ./scripts/verify.ps1`. Pass paired scene/engine roots to add actual target pages, or use `-Software -SkipNative` without an interactive desktop. `cargo run --locked --example native-viewer -- --texture-fixture` shows the generated PNG/LZ4 fixture; `--region-fixture` shows its authored crop.

## Drive and observe

The headless consumer checks eight generated raw/PNG/LZ4 RGBA8/R8/BC3 cases, integer pixel-centre sampling, straight-alpha composition, odd-size resize, retained pixels and upload reuse. Native verification captures the independent-device swap-chain path, resizes, checks idle, switches to motion, then verifies owned-process exit. Actual target checks select nine pages, including first/last sprite atlases. Ten additional captures select first, page-boundary, middle and last authored frames from both sprite sequences. Sixteen generated crops check nonzero offsets, one-pixel dimensions, odd-size resize, retained frames and source upload reuse. BC3 crops deliberately begin inside blocks. `sprite_frames` reads metadata only and rejects unsupported rectangle bases. Native crop verification uses a separate owned instance. These checks do not play or place frames in the scene.

## Evidence and limits

Inspect `textures/index.html`, PNGs and `report.json`, plus `native-texture/report.json` and `native-region/report.json`, captures and transcript. Record source-page bytes/uploads separately from output counters. The largest real decoded page was 248,832,000 bytes. GPU pixel expectations for generated fixtures are independent of decoded output. Real RGBA draws are checked against decoded page bytes; target R8/BC3 captures have no independent Wallpaper Engine reference yet. No authored scene fidelity, aggregate caller memory budget, live streaming, GPU duration or host latency claim is established.
