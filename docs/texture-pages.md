# TEX decoding and page drawing

The library decodes selected TEX base-mip pages and draws pages or checked regions on Direct3D 11 through its public worker/frame API. It reads authored sprite frame rectangles without decoding their payloads. [Diagnostic image composition](image-layers.md) adds placement and order; sprite playback remains planned.

## Public contract

`TextureDecoder::open(&SceneSource)` checks the package/root and hashes the package once. The session keeps the package open read-only, excluding writes and replacement. `decode_page(asset, page_index)` validates all TEX metadata, seeks to that page's base mip and allocates only its payload. Package-local assets take precedence over engine assets. The returned `TexturePage` owns checked dimensions, storage format and immutable bytes. `package_sha256()` records the session's input identity.

Supported storage is raw or LZ4 RGBA8, R8 and BC3, plus PNG with optional outer LZ4. PNG expands to RGBA8 without applying gamma/color profiles; 16-bit channels are reduced to eight bits. BC3 blocks remain compressed for GPU sampling. Missing assets/pages, invalid ranges/codecs, corrupt PNG/LZ4 and allocation limits return asset-named errors.

Opening, hashing and decoding block. Use a tool or asset worker. `Renderer::render_texture_page(Arc<TexturePage>)` only enqueues work. The render worker compiles our HLSL on first use and uploads an immutable source texture. Repeated use of the same Arc reuses that upload; resize preserves the selected page. Source replacement follows completion of preceding rendering. The shader unbinds its source after drawing, and retained output frames remain independent of source replacement.

The diagnostic draw stretches the page or selected rectangle to the output. It selects source texels using integer pixel-centre coordinates. RGBA/BC3 use straight alpha over `[16,24,32,255]`; R8 displays opaque grayscale. Output is opaque SDR UNORM. These are explicit diagnostic conventions. [Image composition](image-layers.md) adds placement and order. Filtering, scene color/alpha fidelity, effects and sprite timing remain open.

### Sprite rectangles

`TextureDecoder::sprite_frames(asset)` reads the TEXS table in authored order. Each `SpriteFrame` reports its page index, positive duration, top-left pixel origin and extent. The component supports integer, positive, axis-aligned rectangles. Rotated, mirrored or fractional bases fail explicitly; static textures have no frame table. Durations below nanosecond resolution fail rather than rounding to zero. Inspection still accepts valid general TEXS bases without claiming they can be drawn. Metadata is bounded to 65,536 records and reading it allocates no pixel payload.

Decode the chosen page, then pass `Arc::new(TextureRegion::new(page, origin, extent)?)` to `Renderer::render_texture_region`. A region must be nonempty and entirely inside its page; arithmetic overflow fails construction. It shares the page bytes. GPU sampling adds the crop origin after integer texel-centre selection, so no neighbouring atlas frame is sampled and BC3 needs neither CPU decompression nor block-aligned crop coordinates. Selecting another region of the same page Arc reuses its upload. Resize preserves the crop. `Frame::texture_region` reports the submitted origin/extent for page and region draws, without retaining CPU pixels.

This region path crops individual frames and stretches them to output. The separate [sprite extraction path](sprite-extraction.md) adds time-based selection, one cached atlas and extracted payloads. Neither path implements interpolation, prefetch or live scene playback. In the target package, birds change pages every 18 frames and the character every 16; each contains 480 frames. Do not assume these page capacities for other textures.

## Resource limits

Decoded storage is capped at 256 MiB per page. Encoded PNG/LZ4 inputs and LZ4-expanded PNG streams are capped at 64 MiB. PNG decoder workspace has a 64 MiB limit. Checked lengths and extents precede payload/output allocation.

Page/region draws keep one uploaded source page; [image compositions](image-layers.md) admit up to 256 MiB of unique sources. `source_texture_bytes` reports resident source payloads and `texture_uploads` counts uploads. This is outside the [three-output pool](frame-pool.md). Caller-held pages and concurrent decoder calls have no aggregate budget. Driver allocation overhead and decode/upload transients require separate measurement before scene playback acceptance. Whole-page decoding here does not implement the planned bounded sprite streaming strategy.

## Run and inspect

```powershell
cargo run --locked --example texture-checks
cargo run --locked --example native-viewer -- --texture-fixture
cargo run --locked --example native-viewer -- --region-fixture
pwsh -File ./scripts/verify.ps1
```

`texture-checks` generates eight redistributable cases, verifies every GPU pixel, checks odd-dimension resize and retained output, and proves repeated draws/resize do not reupload the selected page. It also checks two authored crops per case, including unaligned BC3 crops, source reuse across crop changes and retained cropped frames. It writes PNGs, `index.html` and `report.json`. The native flags prepare a generated PNG/LZ4 page and optional crop before window creation, present through the independent consumer device and check captured back-buffer pixels. Existing motion controls can switch to the motion fixture. `verify-native.ps1 -TextureFixture` and `-RegionFixture` drive capture, resize, idle, motion and owned-process exit. The full gate runs separate blank, page and region native instances.

To add local target pages and inspection:

```powershell
pwsh -File ./scripts/verify.ps1 `
  -ScenePath 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg' `
  -EngineAssets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'
```

The [hardware/native receipt](../artifacts/20261010-131236-180dff12/manifest.json) and [WARP/headless receipt](../artifacts/20261010-131400-ac7c716b/manifest.json) passed on 10 October 2026. WARP explicitly skipped native scenarios and proprietary content. Hardware decoded/drew nine real pages and ten authored sprite frames. Captures include frame 0, both sides of the first page transition, frame 239 and frame 479 for each sequence. The largest decoded page was 248,832,000 bytes. Real RGBA captures were checked against decoded bytes or independently copied CPU crop rows; R8/BC3 target captures were inspected, with format correctness established by generated known-pixel tests. Native cropped back-buffer checks, resize, idle and owned exit passed. Captures are component diagnostics, not scene fidelity proof. These ignored receipts may be absent elsewhere; rerun there.

The hardware debug sample opened/hashed the package in 4.19 seconds, then decoded individual pages in approximately 5–794 ms. Decode timings exclude GPU upload/capture. They establish this diagnostic route, not release-build throughput, live streaming or host responsiveness. The first odd-height hardware test caught a floating UV boundary mismatch; integer texel selection fixed it, and WARP/hardware resize checks pass.

`png` remains pinned at 0.18.1 and now serves library decoding. `lz4_flex` 0.12.1 is MIT-licensed, requires Rust 1.81 and enables safe block encoding/decoding without the frame dependency. It includes the malformed-offset fix in [RUSTSEC-2026-0041](https://rustsec.org/advisories/RUSTSEC-2026-0041.html). Shader compilation uses the existing Windows dependency's [D3DCompile binding](https://learn.microsoft.com/en-us/windows/win32/api/d3dcompiler/nf-d3dcompiler-d3dcompile), never proprietary shader blobs.
