# Periodic Shake

Scene discovery supports balanced periodic Shake in SDR and HDR scenes under
the library playback clock. Masks decode once within the existing source-memory
budget. Seek, rewind and pause apply without per-frame readback.

The profile accepts `effects/shake/effect.json`, version 1 or 2, one
`effects/shake` material pass, and zero or absent `AUDIOPROCESSING`, `DIRECTION`
and `NOISE` combinations. Defaults are speed 1, strength 0.1, bounds `[0, 1]`
and friction `[1, 1]`. Nondefault bounds/friction, audio, noise, one-direction
movement and unknown constants/combinations remain explicitly reported.
Legacy definitions without a replacement key also remain unsupported.
Tolerant playback retains the image when a profile fails.

Slot zero preserves the source. Slot one supplies a required color direction
mask, slot two an optional time offset and slot three optional effect opacity.
Direction uses red/green; blue and alpha do not control opacity. Offset and
opacity use red and accept grayscale. The independent model is:

```text
direction = (painted_rg * 255 - 127) / 128
phase = time_seconds * speed + time_offset_red * 2π
displaced_uv = uv + direction * strength² * sin(phase)
color = lerp(original_color, displaced_color, effect_opacity_red)
```

Missing offset means zero and missing opacity means one. Opacity blends sampled
colors rather than scaling UV displacement. Sampling clamps to the crop.
Existing Spin, Water Waves, Precise Blur, Cloud Motion and Water Flow retain
priority on mixed layers. Additional Shake passes remain reported until ordered
chains are implemented.

## Evidence

The [Wallpaper Engine reference](../artifacts/shake-reference-v8/comparison.json)
checks squared strength, defaults, neutral direction, unused mask channels,
quarter/half-cycle offset and color opacity. Six JPEG captures span 2.1 seconds.
Fitting only the unknown start phase, with speed fixed at one radian per second,
gives maximum error 1.85 source texels within a four-texel JPEG/capture tolerance.
A frozen capture sequence is rejected. A separate 8.5-second periodic probe
matched the same speed.

Public GPU tests check an independent pixel oracle, including a striped channel
that distinguishes color blending from UV scaling, exact rewind, invalid
profiles, disabled VFX, HDR and mixed-effect preservation in both orders. The
[software gate](../artifacts/20261012-192858-5655d787/manifest.json) passed with
diagnostic native checks skipped.

Actual Gojo `2778759553` applies Shake to object 14 in the
[native check](../artifacts/shake-native-gojo-final/report.json). Changed captures,
exact rewind, live rendering, pause, 4K and owned cleanup passed. The
[rebuilt T3 addon check](../artifacts/shake-native-gojo-final/t3-native.json) passes
frame leases, release, hidden pause, reduced motion and remount. Frame counts
do not establish displayed cadence.

The [actual Gojo comparison](../artifacts/gojo-engine-comparison-20261012/comparison.json)
observed Wallpaper Engine and the native viewer at 1280×720. Both render the full
character, chair, city backdrop and animated particles. Three engine captures
and two native live captures retain window identities, timestamps and hashes.
The repeated native motion check passed seek, rewind, pause, resize and owned
cleanup; both temporary windows were confirmed absent afterward.

Foreground placement and particle arrangement differ. The engine's changing
foreground position is consistent with omitted camera/parallax behavior, but
the cause was not isolated. Different clocks and particle seeds prevent an
exact frame comparison. The scene captures do not isolate Shake amplitude or
phase; the controlled probes above provide that evidence. HDR Water Waves,
Pulse and parallax remain omitted, and exact bloom filtering remains unverified.
Complete scene agreement, displayed cadence and T3 application appearance remain
unverified.

## Reproduce the reference

```powershell
python scripts/prepare-shake-reference.py artifacts/shake-reference-new '<engine assets>'
```

The generator refuses destinations outside ignored `artifacts/`. Installed
reference assets are used solely by Wallpaper Engine and kept out of commits.
A second run reproduced 138 identical files. The library neither compiles nor
translates proprietary shaders.

Open `relationships/project.json` using the official
[window command](https://help.wallpaperengine.io/en/functionality/cli.html), a
unique name and a 768×1024 client. Observe through Computer Use and save at least
six original JPEGs over two seconds. Record `path`, `start_ms` and `end_ms` in
`timeline.json`. Determine the client origin from that window, then run:

```powershell
python scripts/verify-shake-reference.py artifacts/shake-reference-new artifacts/shake-reference-new/timeline.json --client-x 1 --client-y 31 --output artifacts/shake-reference-new/comparison.json
```

The shown origin belongs to the recorded window. Pillow is required. Close only
the named reference and retain hashes. The [official description](https://docs.wallpaperengine.io/en/scene/effects/effect/shake.html)
defines the controls; numerical relationships come from the local probes.
