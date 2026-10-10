---
name: control-scene-renderer
description: Inspect readiness and verify scene-renderer through its minimal Windows example viewer. Use when testing rendered output, playback, lifecycle behavior, or performance after a renderer change.
---

# Control scene-renderer

Give agents a repeatable way to observe the rendering library through a real consumer. The example viewer uses the public library API. Keep scene behavior in the library and control mechanics in a reusable tool.

## Current capability

Blank rendering, controlled motion, bounded output reuse, TEX page/region decoding/drawing, sprite extraction/background preparation/prefetch, diagnostic image composition, native/headless consumers, and repeatable launch/control/capture/cleanup are implemented. Runtime scene loading and production playback remain unavailable. For read-only package/dependency inspection, use [the inspection guide](../../../docs/scene-inspection.md). Doctor with `-ScenePath` still rejects scene rendering readiness; inspection evidence is separate. Read [the blank renderer guide](../../../docs/blank-renderer.md) for ownership and [motion verification](../../../docs/motion-verification.md) for temporal checks and metric definitions. The [frame exchange guide](../../../docs/frame-pool.md) defines retained-frame saturation and output counters; viewer `inspect` replies include `output_pool`. For page decoding, shader sampling and the native `--texture-fixture` / `--region-fixture` routes, read [TEX page verification](../../../docs/texture-pages.md). For fixed-time sampling and the native --extracted-fixture route, read [sprite extraction](../../../docs/sprite-extraction.md) and [background preparation](../../../docs/sprite-worker.md). For current/next ready batches and the native --paged-fixture route, read [sprite prefetch](../../../docs/sprite-prefetch.md). For ordered placed images and the native `--layers-fixture` route, read [image composition](../../../docs/image-layers.md). For a common image clock, native `--image-fixture` or target `--scene` startup, and exact seek/play/pause controls, read [selected image playback](../../../docs/image-playback.md).

From the repository root, run:

```powershell
pwsh -File ./.codex/skills/control-scene-renderer/scripts/doctor.ps1
```

To check a locally available scene path, supply it explicitly:

```powershell
pwsh -File ./.codex/skills/control-scene-renderer/scripts/doctor.ps1 -ScenePath 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg'
```

The example package was inspected at that path during planning. Doctor only checks its current existence; scene verification still exits with code `2`. Without a scene argument, doctor reports readiness only when a successful blank/motion/page/crop/extraction/asset-worker/prefetch/composition/image-playback/native verification receipt matches current source and executable hashes. Exit code `1` means doctor failed. Read [the control contract](references/control-contract.md) when interpreting evidence or extending control operations.

Run the complete diagnostic scenarios with:

```powershell
pwsh -File ./scripts/verify.ps1
```

Use `-Software -SkipNative` without an interactive desktop. Report the native path as skipped. Every run creates a unique `artifacts/` directory. The controller operates only on the process it launched and verifies exit. Capture readback and one idle CPU sample do not establish scene or host performance.

## Verify a change

1. Read [the feature map](../scene-renderer-feature-map/SKILL.md) and the relevant feature entry. Establish what the caller must observe before changing code.
2. Run doctor in the checkout containing the change. Missing or stale evidence is a reason to run verification; missing platform prerequisites are a blocker to report. Do not substitute build success for runtime proof.
3. Run the mapped scenario from that checkout. `scripts/verify.ps1` checks blank, motion and texture pixels, verifier fault cases and live readiness timing, then drives resize/capture/play/pause in separate blank, page, crop, extracted-frame, prefetched-frame and composition native instances and stops them. The gate also drives generated combined image playback; paired scene/engine roots drive six supported target images. Full scene and adapter scenarios remain planned.
4. Capture the mapped output and measurements. For a bug, establish the failure before the fix where practical. For performance, compare the same workload and hardware before and after.
5. Stop only the instance created for this run and retain evidence. Report what passed, failed, or remains unverified, with artifact paths and measurement limits.

When a feature or control path changes, update its map entry with the verified procedure. Mark a procedure verified only after executing it end to end, including cleanup. Use bounded waits for observed states instead of guessed delays.

## Observation boundaries

- Use stable controller operations instead of coordinates where possible. A screenshot proves appearance; multiple frames or a recording are needed to establish animation.
- For fixture motion, inspect `motion/index.html`, checkpoint results, self-test faults and the separate live timing report. A controlled contact sheet does not establish real-time cadence. Readiness and presentation-call return timestamps do not establish GPU duration or display timing.
- Use the finished implementation's supported time and seed controls for repeatable fixtures. Record nondeterminism where those controls are absent.
- A viewer result establishes only that consumer path. Verify a host adapter inside its actual application before claiming integration or host responsiveness.
- Keep downloaded packages and engine assets read-only. Store captures outside source fixtures and do not commit third-party content without redistribution rights.
