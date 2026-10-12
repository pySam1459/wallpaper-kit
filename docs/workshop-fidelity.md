# Workshop fidelity priorities

Audited on 12 October 2026. The collection contains **65 wallpapers: 59 scenes
and six videos**. All 65 load and render with omissions. There are zero
whole-wallpaper load errors. Remaining errors concern optional visual objects,
unsupported effects and incomplete scene behavior.

The [complete baseline catalog](../artifacts/workshop-fidelity-priorities/report.md)
lists every folder, missing named effect and affected IDs for each gap. Its
[JSON evidence](../artifacts/workshop-fidelity-priorities/report.json) retains
every original omission and source identity. Counts below mean distinct
wallpapers, not repeated layer instances. Categories overlap; do not sum them.

The [latest Panda batch audit](../artifacts/workshop-fidelity-panda-final/report.md)
still records 65/65 loading and zero failures. Basic image-layer Water Waves
omissions now affect 13 wallpapers, down from 19 in the baseline; unsupported
variants and effect chains remain reported. Panda Store applies six masked Spin
and two Water Waves profiles. Its fixed-time frames change and rewind exactly,
and native playback, pause and 4K output passed. T3 now requests linear sampling
at display height, capped at 4K. Exact Wallpaper Engine phase/amplitude and the
current T3 visual comparison remain unverified after Computer Use was stopped.

The [Shake batch audit](../artifacts/workshop-fidelity-shake-final/report.md)
retains 65/65 loading. [Periodic Shake](shake.md) is now applied in seven folders:
`2170552790`, `2432061368`, `2581534176`, `2778759553`, `2784322303`,
`2841443347` and `3406396386`. Remaining Shake omissions fell from 18 folders
and 30 instances to 13 folders and 23 instances. Two of the seven still omit
additional Shake passes. Legacy definitions, nondefault curves, noise/audio and
effect chains remain explicit. This is profile progress, not whole-scene fidelity.

The [particle profile batch](particle-profiles.md) restores nine particle objects
across five wallpapers. Finite-scalar preparation failures fell from 13 folders
to zero, and emitter omissions fell from 17 folders to one. The
[updated audit](../artifacts/workshop-fidelity-particle-profiles/report.md)
still records 65/65 loading. Particle operator omissions now lead at 19 folders;
turbulence appears in 16 of those. Newly accepted profiles expose later missing
operators, so fewer emitter notices do not imply complete particle support.
The [particle delta](../artifacts/particle-gaps-after/delta.json) confirms previous
particle objects and effect inputs remain intact. All previously applied
Spin, Water Waves, Precise Blur, Cloud Motion and Water Flow inputs are unchanged
across all 65 folders in the [delta check](../artifacts/shake-native-gojo-final/collection-delta.json).
The table below retains baseline counts so progress can be compared.

## Fix order

Prioritize concrete gaps by distinct wallpapers affected. For equal counts,
restore an absent object before refining an existing effect. Compare a checked
profile with Wallpaper Engine before admitting additional variants.

| Order | Fix | Wallpapers | Baseline / acceptance check |
| --- | --- | ---: | --- |
| 1 | Authored background clear color | 59 authored | Implemented in this batch. Check uncovered/transparent pixels, alpha blending, resize and HDR accumulation. Opaque imagery can hide the difference. |
| 2 | Water Waves | 19 | 69 omitted instances. Compare direction, scale, speed, strength, perspective and masks. |
| 3 | Shake | 18 | 30 omitted instances. Compare timing, direction, friction, masks and time offsets; keep cursor/audio variants explicit until verified. |
| 4 | Particle emitter components | 17 | 26 notices. Inventory actual emitter names/parameters; verify positions, rates and bounds against isolated references. |
| 5 | Particle preparation failures | 14 | Entire optional particle objects fail. 13 wallpapers hit `expected a finite scalar`, with 22 notices. Inspect authored value types before changing parsing; preserve invalid-data rejection. |
| 6 | Foliage Sway | 14 | 27 omitted instances. Compare branch masks and motion over time. |
| 7 | Water Ripple | 13 | 14 omitted instances. Verify displacement, masking, perspective and looping. |
| 8 | Script / user-property updates | 13 | 106 defaults retained instead of updates. Start with property bindings and specific required behavior; arbitrary SceneScript needs a separate design. |
| 9 | Composition layers / effect chains | 12 | 17 skipped composition points. Apply passes at their authored positions without including later foreground layers. |
| 10 | Particle system flags / animation | 12 | 29 notices. Verify each animation profile. |
| 11 | Pulse | 11 | 11 omitted instances. Compare bounds, phase, speed, noise, colors and masks. |
| 12 | Unsupported material combinations | 9 | 11 notices. Inventory actual combinations before defining supported variants. |
| 13 | Particle layer properties / effects | 9 | 32 notices. Restore authored transforms/effects while bounding sampling work. |
| 14 | Particle instance overrides | 8 | 17 notices. Apply verified overrides and retain explicit reports for others. |
| 15 | Clouds, Iris, additional Water Flow profiles, camera shake | 7 each | Separate profiles; existing cloud-motion/water-flow implementations do not establish these variants. |
| 16 | God Rays | 6 | Seven omitted instances. Keep it separate from procedural light-shaft shapes. |
| 17 | Legacy tint/alignment/perspective, enabled mouse parallax, particle operators | 5 each | Split combined notices into actual properties. Use controlled pointer/clock probes for parallax. |
| 18 | Shine, particle control points/renderers, clamp-UV properties | 4 each | Compare actual variants from the complete catalog. |
| 19 | Puppet geometry/animation, particle initializers, text parents, dependency properties | 3 each | Puppet sources are `2366358165`, `2453332944`, `3013169753`. Keep the [puppet plan](scene-rendering-plan.md#next-compatibility-task-puppet-animation); the cropped Gojo atlas is severe but affects fewer wallpapers. |
| 20 | Two-wallpaper gaps | 2 each | Order individual variants using the complete catalog. |
| 21 | Single-wallpaper gaps | 1 each | The catalog preserves these alongside the common gaps. There are 31 missing named effect families in total. |

The final rows group later work for readability; use exact catalog counts to
order individual fixes. Particle component failures overlap other particle
rows, so a root-cause fix can advance multiple entries.

## Avoid misleading counts

- The combined camera/color/postprocessing notice appeared in all 59 scenes.
  It does not establish that every listed setting is active. Sources confirm
  clear color in all 59, camera shake enabled in seven and mouse parallax in five.
- Layer parallax notices appeared in 43 wallpapers, but only five scenes
  enable camera parallax. Disabled settings are not 43 visible parallax defects.
  Wallpaper Engine describes these controls in its
  [parallax guide](https://docs.wallpaperengine.io/en/scene/parallax/introduction.html).
- Extra-model-field notices appeared in eight wallpapers; actual puppet
  references occur in three. Inventory other fields separately.
- Particle randomness/integration differs from the engine in 20 wallpapers.
  This reference-fidelity limit is separate from missing components and failures.
- Invisible objects, muted audio and bounded canvas normalization are separate
  from missing visual effects. Unknown non-image content remains visible.
- An authored `bloom` field alone does not identify an unsupported bloom variant.
  HDR/reference differences need their own probes.

## Reproduce and track progress

Produce a fresh loading report, then audit it against the current originals:

```powershell
pwsh -File scripts/workshop-support.ps1 -Output artifacts/workshop-support-new
```

```powershell
python scripts/workshop-fidelity.py artifacts/workshop-support-new/report.json artifacts/workshop-fidelity-new
python -B -m unittest discover -s scripts -p test_workshop_fidelity.py
```

Use a new output directory each time. The audit reads all numbered folders,
hashes primary media and project files, and rejects a changed folder set or
primary-source mismatch. Unknown notices are preserved. Engine-asset, decoder
or code changes require rerunning the probe; media hashes cannot verify them.

Loading percentage is a regression check. Fewer omissions are not a fidelity
score: deleting a warning or drawing an approximation does not prove support.
Require independent generated pixels, reference comparison and actual-package
playback before closing each feature. Pixel correctness, live cadence and
consuming-host responsiveness remain separate checks.

## First implementation batch

Scene discovery now applies enabled `general.clearcolor` as an opaque SDR
background. Manual compositions, selected-image playback and standalone video
keep the diagnostic background. Disabled clearing remains reported. Malformed,
nonfinite and out-of-range colors fail before image decoding. Output and
floating-point accumulation targets use the same authored color.

Two public GPU contracts cover transparency, partial coverage, alpha blending,
SDR/HDR paths, resize, retained samples and invalid inputs. The
[hardware/native gate](../artifacts/20261012-143444-c036087f/manifest.json)
passed after this renderer change. The updated collection probe still returned
[65/65, `100.0000`](../artifacts/workshop-support-background/report.json).
It checks pixels against the composition background so the clear color alone
cannot count as image output.

The [updated audit](../artifacts/workshop-fidelity-background/report.md)
confirms matching authored/reported background inputs in all 59 scenes. Five
generated audit tests pass, including repeated-warning deduplication, disabled
settings, audio classification and rejection of changed source bytes. The
[latest software/headless gate](../artifacts/20261012-143815-dd1bfa5d/manifest.json)
also passed after the audit-script update; native checks were skipped in that
run. The prior hardware/native receipt covers the same renderer binaries.

`scripts/prepare-background-reference.py` creates local redistributable SDR/HDR
probes for both renderers. The Wallpaper Engine comparison was interrupted when
the user stopped Computer Use with Escape. Exact engine alpha/color agreement
remains unverified; generated GPU correctness is separate evidence. Effects
after row 1 remain unimplemented by this batch.
