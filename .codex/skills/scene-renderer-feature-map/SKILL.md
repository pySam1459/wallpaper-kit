---
name: scene-renderer-feature-map
description: Find and maintain scene-renderer's consumer-facing feature verification map. Use before reproducing a renderer defect, checking a changed capability, or documenting a newly verified viewer or adapter workflow.
---

# Scene-renderer feature map

Map behavior from the library consumer's point of view: what the capability does, how to reach it, which actions exercise it, and which observations prove it. Keep implementation details in source and architecture documentation.

## Capability index

| Entry | Scope | Status |
| --- | --- | --- |
| [Blank renderer infrastructure](features/blank-renderer.md) | Native/headless blank GPU frames, resize, idle, capture and cleanup | Verified on recorded Windows build |
| [Diagnostic motion](features/diagnostic-motion.md) | Fixed-time GPU animation, independent pixel checks, live readiness and native play/pause | Verified on recorded Windows build |
| [Workshop scene MVP](features/workshop-scene-mvp.md) | Inspect and later render Workshop item `3780699842`, including animation | Inspection verified; rendering planned |
| [Playback and lifecycle](features/playback-and-lifecycle.md) | Pause/resume plus production lifecycle | Fixture play/pause and blank lifetime verified; production lifecycle planned |
| [Performance and host impact](features/performance-and-host-impact.md) | Playback resource usage and consuming-app responsiveness | Idle and fixture readiness diagnostics implemented; scene/host performance planned |
| [Library and adapter integration](features/library-and-adapter-integration.md) | Public consumer API and individual framework adapters | Native/headless blank consumers implemented; framework adapters planned |

Photos, general video playback, and additional scene compatibility remain project requirements. Add their procedures as concrete implementations arrive rather than inventing navigation or commands now.

## Use the map

Read only the entries affected by the task. Use [control-scene-renderer](../control-scene-renderer/SKILL.md) for readiness, instance control, and evidence collection. Each entry's status is authoritative for that procedure; planned entries supply acceptance goals, not proof of support.

## Maintain an entry

Keep one file per consumer capability, with these sections:

- Status: planned, implemented but unverified, or verified. Attach the last verification's build/configuration and evidence reference when verified.
- Consumer outcome: the behavior callers depend on.
- How to reach it: actual API calls and viewer controls once implemented.
- Drive and observe: actions, observable completion conditions, and expected results.
- Evidence and limits: artifacts, unsupported cases, nondeterminism, and relevant prerequisites.

When code changes a route or behavior, update the affected entry in the same change. Execute the procedure before marking it verified. A prior verification record describes that recorded build, not every future revision. Keep unavailable checks visible rather than silently excluding them from the result.
