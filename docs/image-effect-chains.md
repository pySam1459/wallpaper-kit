# Ordered image effects

Scene snapshots and playback apply supported image effects in authored order.
Each pass reads the previous pass's pixels. Supported profiles include Precise
Blur, Cloud Motion, Water Flow, Spin, Water Waves, Shake, UV Foliage Sway,
planar Water Ripple and [periodic Pulse](pulse.md). Each profile retains its
existing parameter restrictions.
[Composition layers](composition-layers.md) reuse these profiles to process the
accumulated scene at their authored draw positions.

`ImagePlaybackReport::effect_chains` records every applied pass by object ID,
including repetitions. The older effect-specific fields record the first pass
of that kind. Tolerant playback omits an unsupported or failed optional pass,
reports its name and keeps the remaining passes. Strict playback rejects invalid
supported profiles. Consumer VFX policy disables passes before asset loading.

## GPU ownership and limits

An image admits up to 16 supported effects. Strict playback rejects a longer
chain; tolerant playback reports and omits excess passes. Manual public
`ImageLayer::with_*` builders retain their documented replacement behavior.

Layers with multiple effects use two reusable source-crop-sized GPU targets.
Their dimensions cover the largest admitted crop across chained layers and
active composition captures.
They alternate as destinations, so a pass never reads its own destination.
Intermediate draws preserve straight RGBA without blending or layer appearance.
The last pass applies placement, brightness and opacity, then blends into the
scene. Masks retain their own normalized coordinates and texture identities.

SDR intermediates use RGBA8; HDR intermediates use RGBA16F. Their combined payload
is capped at 128 MiB for SDR or 256 MiB for HDR. Blur scratch, HDR/refraction,
bloom and the three-output pool have separate allocations. All effect targets
contribute to `OutputStats::effect_texture_bytes`. Unique source pages still
share the 256 MiB source budget. Switching away from chained content releases
the intermediates after preceding GPU work completes.

## Browser verification

Generate original inputs and compare the real native renderer with the installed
Foliage Sway and Water Ripple shaders executed in headless Chrome:

```powershell
cargo build --locked --example browser-worker
python -B scripts/prepare-effect-chain-browser.py artifacts/chain-fixtures-new
node tools/browser-test/verify-effect.mjs --effect chain --fixtures artifacts/chain-fixtures-new --engine-assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets' --output artifacts/chain-reference-new
```

Add `--hardware` for the hardware renderer. Output directories must be new.
The generator uses its own textures and metadata. Installed shader text stays
local; reports retain shader hashes.

The [WARP comparison](../artifacts/effect-chain-reference-software-final-20261025/report.json)
and [hardware comparison](../artifacts/effect-chain-reference-hardware-20261025/report.json)
pass eight cases at eight timestamps, within two channel code values. Cases
cover two to four passes, masks, transparency, appearance, rewind, an unknown
middle effect and a missing normal map. The oracle rejects reversed order,
missing passes, frozen animation and corrupt pixels. A separate exact-pixel
public GPU test checks two, three and sixteen cardinal-rotation passes, HDR,
scratch accounting/release and retained frames.
The [required software/headless gate](../artifacts/20261025-101250-27955532/manifest.json)
passes, including format, Clippy, public contracts, Rustdoc, dependency boundaries
and consumer checks. Native-window verification is skipped.

The [actual autumn-temple scene](../artifacts/effect-chain-actual-browser-20261025/report.json),
`3565747436`, passes Playwright and Chrome DevTools checks for pixels,
play/pause/seek, resizing, errors, resource bounds and owned-process cleanup.
The [collection audit](../artifacts/workshop-fidelity-chains-final-20261025/report.md)
retains 65/65 loading. The [delta](../artifacts/workshop-fidelity-chains-final-20261025/collection-delta.json)
records 70 additional passes across 21 wallpapers and retains every previously
rendered object and effect family.

These checks establish the tested pass composition and shader profiles.
Complete Wallpaper Engine runtime agreement, native display cadence and host
responsiveness remain unverified.
