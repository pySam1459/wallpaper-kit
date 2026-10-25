# UV Foliage Sway

Scene discovery applies the stock `foliagesway` UV profile through the existing
masked image-motion path. The GPU draws displaced source pixels at the playback
clock time. Masks and noise decode once within the existing source budget;
sampling does not allocate effect targets or read back pixels. Pause, rewind,
resizing, disabled VFX and HDR use the existing playback/renderer contracts.

The checked profile accepts version 1 or 2, a single `effects/foliagesway`
material pass, UV `MODE=0`, and an optional `MASK` combination. Vertex mode,
unknown combinations/constants and legacy `strengthuv`/`friction` profiles
remain omitted under tolerant playback. Strict playback rejects unsupported
profile data.

Accepted controls and defaults:

| Control | Default | Accepted range |
| --- | ---: | --- |
| `speeduv` | 5 | -100 to 100 |
| `strength` | 0.4 | 0 to 1 |
| `phase` | 0.5 | 0 to 2 |
| `power` | 1 | 0.01 to 2 |
| `ratio` | 0.3 | 0.01 to 10 |
| `scale` | 0.05 | 0 to 1 |
| `scrolldirection` | 0 radians | -65536 to 65536 |

Corner/direction weights are validated but inactive in UV mode. Texture slot 0
keeps the source, slot 1 supplies optional red-channel opacity, and slot 2 supplies
green-channel noise. Missing/null noise resolves to `materials/util/noise.tex`
through the normal package/engine lookup. `MASK=0` disables the mask even when
a texture is present. Noise repeats; masks and source sampling clamp. Aspect
uses the source region dimensions rather than the output window.

Strength affects squared displacement amplitude. Direction rotates the spatial
phase and aspect weighting. The signed-power waveform combines several
frequencies, so its clock must not wrap at a single sine cycle. The
[official effect guide](https://docs.wallpaperengine.io/en/scene/effects/effect/sway.html)
describes the UV and vertex controls.

[Ordered image effects](image-effect-chains.md) apply sway at its authored
position, including repeated passes. `effect_chains` records every applied pass;
`image_motions` retains the first applied motion profile per object.

## Browser verification

The [software reference check](../artifacts/foliage-browser-reference-20261024b/report.json)
covers 12 generated cases. The [hardware reference check](../artifacts/foliage-browser-reference-final-20261024/report.json)
covers 13, adding transparency. Across eight timestamps per case, the native
renderer matches the installed original effect pass executed in Chrome WebGL
within two code values per channel. Cases exercise defaults, an authored
profile, rotation, aspect, fractional power, reverse/zero speed, zero strength,
phase/scale, soft/disabled/absent masks and source transparency. Adjacent samples
cross the former single-cycle wrap point; a long-running timestamp and exact
rewind also pass. Frozen-motion and corrupt-pixel faults are rejected.

The browser oracle reads the locally installed original GLSL and retains its
hashes. It supplies test uniforms, textures and a minimal GLSL ES compatibility
wrapper. Installed source and Workshop content are not copied into the repo or
bundled with the renderer. This verifies the tested shader pass, not the complete
Wallpaper Engine runtime, arbitrary shader variants or displayed cadence.

Generate original inputs and run the reference comparison:

```powershell
python -B scripts/prepare-foliage-browser.py artifacts/foliage-inputs-new
node tools/browser-test/verify-effect.mjs --effect foliagesway --fixtures artifacts/foliage-inputs-new --engine-assets '<engine assets>' --output artifacts/foliage-reference-new
```

Add `--hardware` for the hardware renderer; the default is WARP. Chrome is
headless. Every output directory must be new. The test closes its renderer
processes and Chrome, retaining captures and cleanup receipts.

The [actual scene browser check](../artifacts/foliage-actual-browser-20261024/report.json)
uses Playwright and Chrome DevTools MCP on folder `3309706653`. It verifies native
RGBA/canvas equality, seek/rewind, play/pause, bounded output, resizing, error
recovery and cleanup, with captures at 0 and 1500 ms. The
[required software/headless gate](../artifacts/20261024-193806-d2c69f4b/manifest.json)
passes with native-window scenarios skipped.

The [collection audit](../artifacts/workshop-fidelity-foliage-20261024/report.md)
retains 65/65 loading. Eight image layers across seven folders now apply sway.
The [delta check](../artifacts/foliage-inventory-20261024/collection-delta.json)
preserves prior objects and effects. Sway omissions fell from 27 instances in
14 folders to 19 in 10 folders, mostly additional passes on mixed layers.
Complete scene fidelity, native display timing and T3 appearance remain open.
