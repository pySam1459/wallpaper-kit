---
name: wallpaper-kit-feature-map
description: Find and maintain wallpaper-kit's consumer-facing feature verification map. Use before reproducing a renderer defect, checking a changed capability, or documenting a newly verified viewer or adapter workflow.
---

# wallpaper-kit feature map

Map behavior from the library consumer's point of view: what the capability does, how to reach it, which actions exercise it, and which observations prove it. Keep implementation details in source and architecture documentation.

## Capability index

| Entry | Scope | Status |
| --- | --- | --- |
| [Browser tests](features/browser-tests.md) | Native pixels, playback controls, resize/errors, Playwright and Chrome DevTools MCP | Fixture, actual scene and video verified in isolated headless Chrome |
| [Blank renderer infrastructure](features/blank-renderer.md) | Native/headless blank GPU frames, bounded output reuse, resize, idle, capture and cleanup | Verified on recorded Windows build |
| [Diagnostic motion](features/diagnostic-motion.md) | Fixed-time GPU animation, independent pixel checks, live readiness and native play/pause | Verified on recorded Windows build |
| [TEX page and crop drawing](features/texture-pages.md) | Selected raw/PNG/LZ4 pages, authored integer sprite crops, RGBA8/R8/BC3 sampling, reuse and native capture | Verified on recorded Windows build |
| [Image composition](features/image-composition.md) | Ordered placed crops, source reuse, selected target snapshots and combined image playback | Verified on recorded Windows build |
| [Sprite extraction](features/sprite-extraction.md) | Fixed-time selection, bounded extraction/background preparation/prefetch, stale-result rejection and native prepared-frame capture | Verified on recorded Windows build |
| [Workshop scene MVP](features/workshop-scene-mvp.md) | Inspect and later render Workshop item `3780699842`, including animation | Inspection and seven-image native playback verified; complete scene rendering planned |
| [Scene particles](features/scene-particles.md) | Gojo stars, shooting-star trails and fireworks death/follow children | Generated WARP and native Gojo checks verified; engine fidelity unverified |
| [Playback and lifecycle](features/playback-and-lifecycle.md) | Pause/resume plus production lifecycle | Fixture and selected-image play/pause/seek, frame lifetime and pool backpressure verified; full scene lifecycle planned |
| [Performance and host impact](features/performance-and-host-impact.md) | Playback resource usage and consuming-app responsiveness | Release scene resource/throughput baseline verified; production host latency remains open |
| [Library and adapter integration](features/library-and-adapter-integration.md) | Public consumer API and individual framework adapters | Native/headless consumers and local Windows Zed GPUI diagnostic composition verified; full host acceptance planned |

Photos, general video playback, and additional scene compatibility remain project requirements. Add their procedures as concrete implementations arrive rather than inventing navigation or commands now.

## Use the map

Read only the entries affected by the task. Use [control-wallpaper-kit](../control-wallpaper-kit/SKILL.md) for readiness, instance control, and evidence collection. Each entry's status is authoritative for that procedure; planned entries supply acceptance goals, not proof of support.

## Maintain an entry

Keep one file per consumer capability, with these sections:

- Status: planned, implemented but unverified, or verified. Attach the last verification's build/configuration and evidence reference when verified.
- Consumer outcome: the behavior callers depend on.
- How to reach it: actual API calls and viewer controls once implemented.
- Drive and observe: actions, observable completion conditions, and expected results.
- Evidence and limits: artifacts, unsupported cases, nondeterminism, and relevant prerequisites.

When code changes a route or behavior, update the affected entry in the same change. Execute the procedure before marking it verified. A prior verification record describes that recorded build, not every future revision. Keep unavailable checks visible rather than silently excluding them from the result.
