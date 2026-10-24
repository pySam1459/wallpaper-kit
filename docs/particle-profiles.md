# Particle profile compatibility

The 12 October batch supports box emitter `directions`, sphere emitter `sign`,
vector rotation ranges and nullable particle defaults. It restores nine particle
objects across five downloaded wallpapers while retaining every previously
prepared particle object and applied image effect.

## Behavior

Box directions multiply the minimum and maximum distance on each axis. Zero
height produces a horizontal line. Sphere sign accepts exactly `-1`, `0` or `1`
per axis and confines the birth direction to the corresponding side. Unknown
emitter fields remain explicitly reported.

Vector rotation uses the Z component in clockwise radians. Missing minimum is
zero; missing maximum is one turn. The parser converts to the layer API's
counterclockwise degrees. Nonzero X/Y rotation remains unsupported. Existing
numeric rotation retains its earlier degree convention. Null scalar/vector
particle values use their defaults; required references and malformed/nonfinite
numbers still fail. This does not loosen the general image parsers.

The official [emitter documentation](https://docs.wallpaperengine.io/en/scene/particles/component/emitter.html)
describes direction scaling and sign constraints. Units and rotation direction
were established through generated local Wallpaper Engine probes.

## Evidence

The [before/after collection delta](../artifacts/particle-gaps-after/delta.json)
records these additions:

| Folder | Added particle objects |
| --- | --- |
| `2288914800` | `1281`, `1378`, `3107` |
| `2374964197` | `4629`, `396` |
| `2419179377` | `88` |
| `2435600040` | `368`, `376` |
| `2572181262` | `93` |

The [collection audit](../artifacts/workshop-fidelity-particle-profiles/report.md)
retains 65/65 loading. All 22 finite-scalar preparation failures across 13
wallpapers disappeared. Emitter omissions fell from 26 notices in 17 wallpapers
to one notice in one wallpaper. These counts describe preparation progress,
not full-scene fidelity. Newly accepted profiles can still stop at a separately
reported unsupported operator or renderer.

Five new public contracts check the authored formats, independent geometry and
GPU pixels, exact rewind, defaults, sign validation and explicit 3D omissions.
Both the [engine geometry check](../artifacts/particle-compat-reference-before/captures/comparison.json)
and [native geometry check](../artifacts/particle-compat-native-final/comparison.json)
pass the same zero/quarter-turn sprite and box/sign bounds oracle. Random
positions differ, so it checks their supported bounds rather than exact seeds.
The engine fixture includes explicit zero box speed fields; the native fixture
omits those fields because box speed is not admitted. Both are stationary.

The [native probe controller](../artifacts/particle-compat-native-final/report.json)
verified all five particle objects, rewind, pause, live rendering and owned exit.
These probes intentionally remain static over the sampled interval. Presentation
counts do not establish displayed cadence. The [required software gate](../artifacts/20261012-194913-6ebe399a/manifest.json)
passed; its diagnostic native scenarios were skipped after the desktop locked.
The [rebuilt T3 addon check](../artifacts/particle-gaps-after/t3-native.json)
retains Levi objects `85,93` and passes bounded frame leases, release, hidden pause,
reduced motion and remount. T3 application appearance was not checked.

The [live Levi comparison](../artifacts/levi-particle-comparison/comparison.json)
shows both consumers rendering the character and city, with animated fog.
It exposed a red/orange fog error in two-channel particle textures. Rain and ash
remain explicitly omitted. Unsynchronized clocks and different seeds prevent
exact trajectory comparison; presentation counts do not establish display cadence.

Two-channel particle textures now use red as grayscale and green as alpha,
including interpolated sprite frames. The [engine channel probe](../artifacts/particle-channels-reference/comparison.json)
matches an independent grayscale-alpha oracle. The [old native capture](../artifacts/particle-channels-before/comparison.json)
fails that oracle and the [corrected capture](../artifacts/particle-channels-after/comparison.json)
passes it. A public WARP pixel contract checks bright, dark and transparent
samples. The [updated Levi capture](../artifacts/levi-particle-channels-after/live.jpg)
shows pale fog after the fix. Complete particle appearance, turbulence,
engine randomness and full-scene fidelity remain open.

## Velocity correction, 24 October

Turbulent velocity now defaults to 100–250 units per second, an upward forward
direction and an offset in radians. Zero scale produces a straight stream.
Previously the library used 0–1 and a different direction, leaving retained
ember trails nearly invisible. Nonzero scale still uses an independent noise
field and now reports that limitation explicitly.

The [minimum-speed engine check](../artifacts/particle-velocity-minimum-20261024/comparison.json)
and [maximum-speed engine check](../artifacts/particle-velocity-maximum-20261024/comparison.json)
match independent stationary trail bounds. The corresponding
[native minimum](../artifacts/particle-velocity-minimum-native-20261024/comparison.json)
and [native maximum](../artifacts/particle-velocity-maximum-native-20261024/comparison.json)
checks match those same bounds. A wrong-speed reference is rejected. Public
contracts pass for zero scale, offset direction and the default speed range.

The [real ember comparison](../artifacts/particle-velocity-ember-native-20261024/comparison.json)
shows bright rising streaks after the correction. Exact trajectories differ:
the turbulence operator remains omitted, initializer noise is approximate,
and the consumers use different seeds and clocks. Captures, rewind, pause and
owned exit passed. The [hardware/native gate](../artifacts/20261024-175119-c3344866/manifest.json)
passed. The [current audit](../artifacts/workshop-fidelity-particle-velocity-20261024/report.md)
retains 65/65 loading; the [delta check](../artifacts/particle-velocity-ember-native-20261024/collection-delta.json)
retains every previous particle object and image effect input.

Tolerant playback also retains supported components around unknown operators,
with each omission named. The earlier [partial-particle delta](../artifacts/particle-fallback-comparison/delta.json)
adds 29 prepared objects across 18 folders. This is partial rendering, not
support for the skipped operators. Alpha change scales initial opacity over
normalized lifetime, with continuous sampling and exact rewind.

The rebuilt [T3 addon check](../artifacts/particle-velocity-ember-native-20261024/t3-native.json)
passes GPU leases and lifecycle checks. At 1920×1080 with linear sampling and
a 60 fps request, the [timing run](../artifacts/particle-velocity-ember-native-20261024/t3-timing.json)
observes 59.23 exports/s and a 31.35 ms p95 gap. Node polling variability does
not establish displayed cadence or T3 input latency. Hardware/settings are
retained with the receipt. T3 application appearance was not rechecked.

## Browser turbulence approximation, 24 October

Tolerant playback now applies a smooth 2D turbulence approximation to the common
zero-gravity, zero-drag profile in 16 wallpapers. Strict playback continues to
omit this operator. The report says `turbulence uses independent harmonic noise;
exact engine noise is not reproduced`, and the collection audit retains a
separate **Particle turbulence approximation** fidelity category. This batch
restores motion; it does not establish Wallpaper Engine fidelity.

The independent field combines two spatial waves, with per-particle speed and
phase ranges, axis masks, scale and signed noise speed. It integrates the force
analytically along the initial trajectory. Position and trail velocity remain
continuous at arbitrary sample times, including zero noise speed. Seeking does
not replay history or accumulate frame-dependent simulation state. This model's
force units, waveform, defaults and displaced-position feedback have not been
calibrated against Wallpaper Engine. Audio, operator blending, 3D masks, gravity
and drag combinations remain omitted. The official [operator documentation](https://docs.wallpaperengine.io/en/scene/particles/component/operator.html#turbulence)
describes the authored controls; it does not specify the underlying algorithm.

Four public contracts cover independent constant-force GPU pixels, axis masks,
fractional and negative masks, trail velocity, 120 Hz position sampling, exact
rewind, malformed input and explicit omission of other profiles. The
[WARP browser check](../artifacts/particle-turbulence-browser-software-final-20261024/report.json)
and [hardware browser check](../artifacts/particle-turbulence-browser-check-20261024b/report.json)
use Playwright and Chrome DevTools MCP. The browser samples 121 frames over a
one-second interval, verifies their raster positions against `displacement =
4*t*t`, and checks play/pause, resizing, error recovery, resource bounds and owned
process exit. These controlled samples do not measure displayed cadence.

The [real Levi browser check](../artifacts/particle-turbulence-levi-browser-final-20261024/report.json)
passes and retains screenshots at 0 and 1500 ms. All 65 wallpapers still load in
the [collection probe](../artifacts/workshop-support-turbulence-final-20261024/report.json).
The [delta](../artifacts/particle-turbulence-20261024/collection-delta.json) retains
every previous image/particle object and reported effect input. Omitted particle
operators fell from 19 folders to four; the 16 approximate turbulence folders
remain unverified fidelity. The [required software/headless gate](../artifacts/20261024-192121-e4bde8b7/manifest.json)
passes with native-window scenarios skipped. No T3 addon or application visual
verification is claimed for this change.

Generate the redistributable browser-only probe, then run its independent checks:

```powershell
python -B scripts/prepare-particle-motion-reference.py '<engine assets>' artifacts/turbulence-fixture-new --case browser-turbulence
pwsh -File scripts/verify-browser.ps1 -ScenePath artifacts/turbulence-fixture-new/library.pkg -TurbulenceFixture -Output artifacts/turbulence-browser-new
```

Add `-Hardware` for the hardware renderer. The browser-only generator case does
not copy installed shaders or Workshop assets. Every output directory must be
new. Run a separate actual-scene browser check as described in
[browser testing](browser-testing.md).

## Reproduce

```powershell
python scripts/inventory-particle-gaps.py artifacts/workshop-support-particle-profiles/report.json artifacts/particle-gaps-new/inventory.json
python scripts/prepare-particle-motion-reference.py '<engine assets>' artifacts/particle-reference-new --case compatibility
& ./scripts/verify-particle-profiles.ps1 -ScenePath artifacts/particle-reference-new/library.pkg -EngineAssets '<engine assets>' -ExpectedParticles @(2,3,4,5,6) -Width 1024 -Height 512 -Output artifacts/particle-native-new
python scripts/verify-particle-reference.py artifacts/particle-native-new/frame-0-0.png artifacts/particle-native-new/comparison.json
```

For velocity defaults, generate separate `--case velocity-minimum` and
`--case velocity-speed` probes. Run each through the native controller with
objects `@(2,3)` at 1024×512, then use
`scripts/verify-particle-velocity.py <capture> <report> --case minimum` or
`--case maximum`. Engine captures need the observed client origin;
the recorded runs use `--client-x 1 --client-y 31`.

Use the official window command to open the generated project in Wallpaper
Engine with a 1024×512 client. Capture through Computer Use, retain the original
JPEG and window identity/timestamps, then run the same geometry checker with
the observed `--client-x` and `--client-y`. The recorded reference uses `1,31`.
Pillow is required. Generated probes and local engine inputs stay out of commits.

Turbulence fidelity remains open in the 16 approximated wallpapers. Remaining
omitted operators affect four folders. Periodic emitter flags, animation
profiles, control points and material variants remain explicit gaps.
