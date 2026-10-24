# Planar Water Ripple

Scene discovery applies the stock planar `waterripple` profile using its normal
map and optional painted opacity mask. Two repeating normal samples animate in
opposite directions while sharing the authored scroll. Their combined normal
displaces source UVs by squared ripple strength and mask red. Source/mask
sampling clamps; the normal map repeats. Aspect comes from the source region,
and ratio scales the normal-map Y coordinates.

The profile accepts version 1 or 2, one `effects/waterripple` material pass,
absent/zero `PERSPECTIVE` and `SPECULAR`, and optional `MASK=0/1`. Perspective,
specular controls and unknown constants/combinations remain explicit omissions
under tolerant playback. Existing image effects retain priority in mixed layers;
additional Ripple passes remain omitted until ordered effect chains are implemented.

| Control | Default | Accepted range |
| --- | ---: | --- |
| `animationspeed` | 0.15 | -10 to 10 |
| `scrollspeed` | 0 | -10 to 10 |
| `ripplestrength` | 0.1 | 0 to 1 |
| `scale` | 1 | 0 to 10 |
| `ratio` | 1 | 0 to 10 |
| `scrolldirection` | 0 radians | -65536 to 65536 |

Both speed controls are squared, so negative values have the same magnitude
behavior as positive values. Zero scale/ratio remain valid. Time is continuous;
it is not wrapped at a sine-cycle boundary. The
[official effect guide](https://docs.wallpaperengine.io/en/scene/effects/effect/waterripple.html)
describes the authored controls and use for water, heat, clothing and hair.

Slot 0 keeps the source, slot 1 supplies mask red, and slot 2 supplies the normal
map. Missing/null slot 2 inherits the material's normal texture, falling back
to `materials/effects/waterripplenormal.tex`. That asset must resolve as a
decodable texture. Missing normal assets omit the effect while retaining the
base image under tolerant playback; strict playback returns the asset error.
`image_motions[object_id] = "waterripple"` records an applied pass. VFX-disabled
playback skips the effect and its inputs.

## Browser verification

Generate original test inputs and compare the native renderer with the locally
installed shader in headless Chrome:

```powershell
python -B scripts/prepare-ripple-browser.py artifacts/ripple-inputs-new
node tools/browser-test/verify-effect.mjs --effect waterripple --fixtures artifacts/ripple-inputs-new --engine-assets '<engine assets>' --output artifacts/ripple-reference-new
```

Add `--hardware` for the hardware renderer; WARP is the default. Use new output
directories. Inputs are generated data, and the reference reads the installed
GLSL without bundling it. The shared runner also checks Foliage Sway, as described
in [its procedure](foliage-sway.md#browser-verification).

The [WARP reference](../artifacts/ripple-browser-reference-20261024/report.json)
and [hardware reference](../artifacts/ripple-browser-reference-hardware-20261024/report.json)
pass 15 cases at eight timestamps each, with a maximum two-code-value difference
per channel. Cases cover defaults, an authored profile, direction/aspect,
animation/scroll, negative/zero speeds, zero strength/scale/ratio, soft and
disabled/absent masks, transparency, normal overrides and inherited/default
normal assets. Long-running time and exact rewind pass. Frozen-motion and
corrupt-pixel faults are rejected. The migrated shared runner also passes all
[13 Foliage Sway cases](../artifacts/foliage-shared-reference-20261024/report.json).

Public GPU contracts independently check normal combination and squared
strength against literal pixels, masks, HDR/control behavior, malformed data,
missing-normal fallback and preservation of previous effects in either order.
The [actual `3137398793` scene](../artifacts/ripple-actual-browser-20261024/report.json)
passes Playwright and Chrome DevTools MCP checks for native/canvas equality,
seek/rewind, play/pause, output bounds, resize/error recovery and cleanup. The
[required software/headless gate](../artifacts/20261024-195701-71a8c92f/manifest.json)
passes with native-window scenarios skipped.

The [collection audit](../artifacts/workshop-fidelity-ripple-20261024/report.md)
retains 65/65 loading. Five image layers across five folders now apply Ripple.
The [delta check](../artifacts/ripple-inventory-20261024/collection-delta.json)
preserves every previously reported object/effect input. Ripple omissions fall
from 14 instances in 13 folders to nine in eight folders. Additional image
effect passes affect 22 folders overall, so the audit now ranks that shared
cause separately while retaining the named-effect counts. These overlapping
categories must not be summed.

The browser oracle verifies the tested installed shader pass, not the complete
Wallpaper Engine runtime. Complete scene fidelity, displayed cadence and T3
application appearance remain unverified.
