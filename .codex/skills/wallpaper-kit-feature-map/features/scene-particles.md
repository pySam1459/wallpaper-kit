# Scene particles

## Status

The [browser turbulence batch](../../../../docs/particle-profiles.md#browser-turbulence-approximation-24-october)
adds an explicitly reported harmonic approximation in 16 folders under tolerant
playback. Strict playback omits it. Independent constant-force GPU pixels, axis
masks, trail velocity, 120 Hz sampled continuity and rewind pass. Playwright and
Chrome DevTools MCP verify the generated fixture on WARP/hardware and the actual
Levi scene, including controls, errors and cleanup. The required
[software/headless gate](../../../../artifacts/20261024-192121-e4bde8b7/manifest.json)
passes with native-window checks skipped. Engine noise/force fidelity, native
display cadence and T3 appearance remain unverified. Omitted operators now
affect four folders; the audit retains 16 unverified turbulence approximations.

The 24 October [velocity correction](../../../../docs/particle-profiles.md#velocity-correction-24-october)
calibrates turbulent-velocity defaults and zero-scale direction. Independent
engine/native trail bounds and the [hardware/native gate](../../../../artifacts/20261024-175119-c3344866/manifest.json)
pass. Real ember playback shows bright rising streaks; exact noise trajectories
remain unverified; that batch omitted the turbulence operator. The collection
retains 65/65 loading and prior prepared objects/effects. T3 addon lifecycle
passes; application appearance and displayed cadence were not rechecked.

[Particle profile compatibility](../../../../docs/particle-profiles.md) supports
box direction scaling, sphere sign, vector rotation and nullable defaults.
Controlled engine/native geometry checks, five public contracts and the
[software gate](../../../../artifacts/20261012-194913-6ebe399a/manifest.json) passed.
The collection adds nine particle objects across five wallpapers and preserves
previous objects/effects. The [Levi comparison](../../../../artifacts/levi-particle-comparison/comparison.json)
exposed a two-channel fog colour error, corrected with an engine channel probe
and public GPU pixels. Rain and ash remain omitted; T3 application appearance
remains unverified. That batch recorded operator omissions in 19 wallpapers,
including turbulence in 16.

A bounded 2D particle profile is implemented through scene discovery. Generated WARP contracts and the [HDR/refraction Gojo native receipt](../../../../artifacts/gojo-hdr-refraction/report.json) verify the recorded Windows build, including normal-map ring admission and HDR bloom. Exact engine fidelity and complete particle compatibility remain unverified. The original Yuta particle profile remains omitted.

The [4K disabled-bloom receipt](../../../../artifacts/gojo-fireworks-after-4k/report.json) verifies that Gojo's authored `bloom=false` retains float HDR accumulation and the ring while removing the added halo and bloom allocations. The [required gate](../../../../artifacts/20261011-233510-3a0742cd/manifest.json) passed. Public WARP contracts independently check disabled-bloom pixels and effect-target bytes.

## Consumer outcome

Gojo's stars, shooting-star trail and fireworks rockets/bursts animate behind its foreground image. Disabled scene bloom no longer washes the burst into a large white disk. Seeking backward restores the same output. Hidden visibility defaults omit objects. Unsupported components are named in the report.

## How to reach it

Use `ImagePlayback::open_scene_images`, `try_sample`, and `Renderer::render_images`. `ImagePlaybackReport::particle_objects` records prepared particle object IDs; `hdr_bloom` records authored HDR activation. Composition draws include particles in authored order. Opening decodes inputs; sampling and GPU presentation follow existing worker boundaries. See [the particle guide](../../../../docs/scene-particles.md) for refraction/HDR behavior and resource bounds.

## Drive and observe

Run `scripts/verify-gojo-particles.ps1` with explicit installed Gojo package and engine roots as documented in the guide. Add `-Width 3840 -Height 2160` for 4K; the controller checks completed-frame dimensions. Inspect captures, rewind, paused idle, source upload reuse, live observations and cleanup. Run `scripts/verify.ps1` with the original Yuta roots to check seven-image behavior. Generated public contracts provide independent additive-pixel and event/geometry checks.

## Evidence and limits

Retain package/build hashes, adapter/resolution, captures, timeline and omissions. Gojo captures have no synchronized engine pixel oracle. Refraction/HDR are rendered, with independent filtering and displacement models; engine calibration remains open. Parallax and image effects remain omitted; fixed seeds, analytic integration and rope strips approximate engine behavior. A full engine reference capture succeeded, but controlled material probes crashed and the dialog named DiscordHook.dll as a likely cause. Live calls do not establish display timing or host impact. Third-party assets and captures stay in ignored local evidence.
