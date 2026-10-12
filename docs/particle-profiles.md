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

## Reproduce

```powershell
python scripts/inventory-particle-gaps.py artifacts/workshop-support-particle-profiles/report.json artifacts/particle-gaps-new/inventory.json
python scripts/prepare-particle-motion-reference.py '<engine assets>' artifacts/particle-reference-new --case compatibility
& ./scripts/verify-particle-profiles.ps1 -ScenePath artifacts/particle-reference-new/library.pkg -EngineAssets '<engine assets>' -ExpectedParticles @(2,3,4,5,6) -Width 1024 -Height 512 -Output artifacts/particle-native-new
python scripts/verify-particle-reference.py artifacts/particle-native-new/frame-0-0.png artifacts/particle-native-new/comparison.json
```

Use the official window command to open the generated project in Wallpaper
Engine with a 1024×512 client. Capture through Computer Use, retain the original
JPEG and window identity/timestamps, then run the same geometry checker with
the observed `--client-x` and `--client-y`. The recorded reference uses `1,31`.
Pillow is required. Generated probes and local engine inputs stay out of commits.

The next particle priority is the operator omission in 19 wallpapers, including
turbulence in 16. Periodic emitter flags, other operators, animation profiles,
control points and material variants remain explicit gaps.
