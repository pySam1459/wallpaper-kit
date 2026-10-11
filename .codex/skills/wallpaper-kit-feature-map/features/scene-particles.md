# Scene particles

## Status

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
