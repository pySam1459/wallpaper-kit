# Composition layers

Scene playback applies supported composition effects to the scene drawn before
that object's authored position. Later foreground images, text and particles
are drawn afterward. This follows Wallpaper Engine's description of composition
layers as a camera capturing lower layers in its
[composition guide](https://docs.wallpaperengine.io/en/scene/rgb/introduction.html).

Full-screen and project layers capture at native output resolution. Adjustable
layers capture their transformed rectangle at the authored `size`, rounded up
to whole pixels. Output resizing and layer scale change placement without
reducing that source resolution. This avoids adding blur when an adjustable
layer is displayed smaller than its authored size.

## Supported profile

Scene discovery recognizes full-screen/project models and adjustable models
using `materials/util/composelayer.json`. It reuses the checked Precise Blur,
Cloud Motion, Water Flow, Spin, Water Waves, Shake, UV Foliage Sway and planar
Water Ripple profiles from [ordered image effects](image-effect-chains.md).
[Periodic Pulse](pulse.md) also applies to composition captures.
Existing parameter, mask and SDR/HDR restrictions still apply.

Adjustable layers support 2D translation, rotation, positive scale, opacity and
brightness. The checked profile requires centered alignment, white tint,
ordinary color blending, background copying and passthrough propagation.
Perspective, parents, shadows, disabled propagation and transformed full-screen
layers remain unsupported. Tolerant playback reports unsupported geometry and
omits that point; unsupported optional effects are named and omitted separately.
Turning off consumer VFX skips composition preparation and its effect assets.

`ImagePlaybackReport::composition_layers` maps admitted object IDs to applied
effect names in authored order, including repetitions. An empty list means no
supported effect profiles; brightness can still apply. It does not establish
that every authored effect is supported. Identity points perform no GPU work.
`ImageComposition::layers()` continues to describe source-image draws; captured
backgrounds do not create synthetic CPU images or texture pages. Explicit
selected-image snapshots remain image-only.

## GPU ownership and limits

The renderer unbinds the accumulated scene and captures the requested rectangle
directly into a separate shared intermediate.
Effects alternate between two shared targets. The final pass applies geometry,
brightness and opacity while blending into the scene. No pass samples its own
destination. Refraction retains its own background snapshot because it reads
the previous scene while drawing back into it.

There are at most 32 admitted points, with up to 16 supported effects per point.
Adjustable captures pass the existing `Size` bound of 64 MiB at RGBA8. Shared
intermediate dimensions cover the largest admitted image crop or composition
capture; their merged extent must also pass `Size`. This bounds their combined
payload to 128 MiB in SDR or 256 MiB in HDR. Targets use RGBA8 in SDR and RGBA16F
in HDR. Blur scratch is shared too; there are no per-point GPU targets.

Active points require an output-sized scene target. A snapshot target is only
needed for refraction. These, the
shared intermediates, blur scratch and bloom contribute to
`OutputStats::effect_texture_bytes`. Masks, noise and normal pages share the
existing 256 MiB unique source-page budget. Output frames retain their separate
three-frame pool. These bounds do not constitute a total process-memory budget.
Switching away from composition content releases unused targets after preceding
GPU work completes.

## Reproduce browser verification

Inventory the local collection without modifying Workshop originals:

```powershell
python -B scripts/inventory-composition.py 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960' artifacts/composition-inventory-new
```

Generate original textures and metadata, then compare native pixels with an
independent scene-capture/placement oracle and installed GLSL effect shaders
executed in headless Chrome:

```powershell
cargo build --locked --example browser-worker
python -B scripts/prepare-composition-browser.py artifacts/composition-fixtures-new
node tools/browser-test/verify-effect.mjs --effect composition --fixtures artifacts/composition-fixtures-new --engine-assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets' --output artifacts/composition-reference-new
```

Add `--hardware` for hardware D3D11. Use new output directories. The generator
uses original test assets; installed shader text stays local and reports retain
its hashes. No Wallpaper Engine or Workshop assets belong in commits.

## Evidence and limits

On 25 October 2026, the
[WARP reference](../artifacts/composition-browser-authored-software-20261025/report.json)
and [RTX 5080 reference](../artifacts/composition-browser-authored-hardware-20261025/report.json)
pass nine cases at eight timestamps plus resize. Cases cover full-screen and
rotated adjustable capture, two to four passes, clipping, masks, transparency,
brightness, HDR, optional-pass failures and rewind. Native-size comparisons
allow two channel code values; resize comparisons allow three because the CPU
capture oracle adds an intermediate UNORM rounding step. WARP stays within two;
hardware reaches three on resized captures. Reports retain counts above two
separately. Reversed passes, missing passes, frozen motion and corrupt pixels
are rejected. A 4K check verifies dimensions, a later foreground pixel and
bounded resources; it is not a full 4K pixel reference.

Public GPU contracts independently check capture order, two composition points,
opacity, authored capture resolution across resize, retained frames, target
accounting/release, HDR overbright preservation, VFX policy and invalid profiles.
The [required software/headless gate](../artifacts/20261025-113728-c1ff6e90/manifest.json)
passes format, Clippy, public contracts, Rustdoc, dependency boundaries and
consumer checks. Native-window scenarios are skipped.

Actual [Gojo](../artifacts/composition-gojo-browser-20261025/report.json) and
[adjustable-region](../artifacts/composition-adjustable-authored-browser-20261025/report.json)
scenes pass Playwright and Chrome DevTools MCP checks for pixels, rewind,
advancement, pause, replacement, resizing, bounds, errors and owned cleanup.
Their captures were inspected. Complete Wallpaper Engine runtime agreement,
native display cadence and host responsiveness remain unverified.

The [collection audit](../artifacts/workshop-fidelity-composition-final-20261025/report.md)
retains 65/65 loading. All 17 composition points across 12 folders are recognized;
14 effect passes now apply at six points across five folders. The
[delta](../artifacts/workshop-fidelity-composition-final-20261025/collection-delta.json)
confirms previous objects and effect inputs remain intact. Eleven points have
no supported effect profiles. Previously hidden effect families are now named
individually; this exposes Pulse in 12 folders as a leading remaining gap.
