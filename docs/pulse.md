# Periodic Pulse

Scene playback applies time-driven Pulse to image layers and composition
captures. The checked profile supports additive and hard-light tint blending,
optional alpha pulsing, painted opacity, bounds, power and scrolling noise.
It participates in authored [effect chains](image-effect-chains.md).

The default additive mode brightens even when both tints are white. Tint colors
multiply the source before blending. A bounded sine signal passes through smooth
thresholding, amount, optional noise and power. Phase is in radians in the
installed fragment profile. The renderer reduces periodic inputs in double
precision before uploading floats, so time does not advance in authored frame
steps. This does not establish smooth native presentation.

## Checked inputs

| Input | Default | Accepted profile |
| --- | --- | --- |
| `speed` | 3 | 0 through 10 radians/second |
| `phase` | 0 | Finite radians, magnitude at most 65,536 |
| `amount` | 1 | 0 through 2 |
| `bounds` | `0 1` | Two values within 0 through 1, strictly increasing |
| `noiseamount` | 0 | 0 through 2 |
| `noisespeed` | 0.5 | 0 through 1 |
| `power` | 1 | 0.01 through 4 |
| `tintlow`, `tinthigh` | `1 1 1` | Three finite channels within 0 through 1 |
| `BLENDMODE` | 9 | Additive 9 or hard-light 13 |
| `PULSECOLOR`, `PULSEALPHA` | 1, 0 | Each 0 or 1 |
| `MASK` | Inferred from texture | 0 disables the painted mask; 1 permits it |
| `AUDIOPROCESSING` | 0 | Time-driven mode only |

Noise uses texture slot 1, falling back to `materials/util/noise.tex`; painted
opacity uses slot 2. Both sample normalized coordinates with bilinear filtering.
Noise repeats; the mask clamps and reads red. Zero noise amount skips its texture
entirely, including missing inactive noise inputs. RGBA and grayscale source
pages retain tint and alpha behavior. SDR and HDR accumulation are admitted.

Audio-driven variants, other blend modes, zero power and unknown parameters
remain unsupported. Zero power can reach undefined `0^0` in the installed shader
at a pulse trough; the generated browser reference exposed divergent output.
Tolerant playback reports and omits unsupported profiles while preserving other
passes and base imagery. Strict playback rejects them.

Consumer `visual_effects: false` and `reduce_flashing: true` both skip Pulse
before preparing its assets. Reduced flashing also continues to skip particles.
Other image effects and sprites retain their existing policy.

## Ownership and resource use

Pulse reuses the image-motion draw path and existing alternating effect targets.
A shared 96-byte motion constant buffer carries timing, flags, extents and tints;
it is separate from the smaller cloud/flow buffer. Inputs share existing source
page caches and budgets. No Pulse-specific render target or background thread
is created. Reports list `pulse` in `effect_chains`, `composition_layers`, and
the first-motion compatibility field when applicable.

## Reproduce browser verification

The fixture generator writes original metadata and textures under ignored
`artifacts/`. The verifier reads installed effect and blending shader text at
runtime; it does not bundle those sources or modify the engine installation.

```powershell
cargo build --locked --example browser-worker
python -B scripts/prepare-pulse-browser.py artifacts/pulse-fixtures-new
node tools/browser-test/verify-effect.mjs --effect pulse --fixtures artifacts/pulse-fixtures-new --engine-assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets' --output artifacts/pulse-reference-new
```

Add `--hardware` for hardware D3D11. Use new output directories. Reports retain
hashes of the installed vertex, fragment and shared blending sources, native
renderer identity, per-time hashes, pixel differences and fault-check results.

On 25 October 2026, [WARP](../artifacts/pulse-browser-software-recorded-20261025/report.json)
and [RTX 5080](../artifacts/pulse-browser-hardware-recorded-20261025/report.json)
pass 18 cases at eight timestamps, including exact rewind. Cases cover defaults,
both blend modes, tint, bounds, phase, noise, soft/disabled/absent masks,
transparency, color/alpha combinations, overbright alpha, low power, stationary
settings, an inactive missing texture, HDR and composition capture. A resized
case also passes. All comparisons stay within two channel code values. Frozen
motion and corrupt pixels fail the oracle.

Public GPU contracts independently check the signal, blend modes, noise, masks,
alpha over the diagnostic background, grayscale tint, rewind, malformed inputs
and consumer policy. The
[required software/headless gate](../artifacts/20261025-115400-644adb10/manifest.json)
passes, including 73 image contracts, Clippy, Rustdoc, dependency boundaries and
consumer checks. Native-window scenarios are skipped.

Actual [hard-light scene `2374964197`](../artifacts/pulse-hard-light-actual-browser-20261025/report.json)
and [noisy composition scene `2288914800`](../artifacts/pulse-composition-actual-browser-20261025/report.json)
pass Playwright and Chrome DevTools MCP checks for native pixels, rewind,
advancement, pause, replacement, resizing, errors, bounds and owned cleanup.
Their captures were inspected. Installed-shader comparisons do not establish
complete Wallpaper Engine runtime agreement, native display cadence or host
responsiveness.

The [collection audit](../artifacts/workshop-fidelity-pulse-final-20261025/report.md)
retains 65/65 loading. The
[delta](../artifacts/workshop-fidelity-pulse-final-20261025/collection-delta.json)
adds nine Pulse passes across nine wallpapers, including one composition point,
while retaining previous objects and effect chains. Remaining Pulse omissions
fall from 12 folders to three, all authored with audio response. Optional-object
preparation failures and particle system flags now lead at 12 folders each.
