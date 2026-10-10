# Blank renderer infrastructure

## Status

Verified on 2026-10-09 with Rust 1.91 on Windows x64. Hardware native/headless checks used an NVIDIA GeForce RTX 5080; the software/headless route used WARP. Exact source and executable hashes are in the [hardware manifest](../../../../artifacts/20261009-233823-61aa2b4f/manifest.json) and [software manifest](../../../../artifacts/20261009-233819-96acac2a/manifest.json). These local evidence files are ignored by Git and may be absent in another checkout; rerun the procedure there. Scene playback and production framework adapters remain unavailable.

## Consumer outcome

The public library produces an immutable opaque blank GPU frame on demand. A native Windows consumer presents it through a separate device, and a headless consumer verifies GPU pixels. Resize replaces output while retained frames remain valid. Idle rendering has no recurring timer. Shutdown joins the renderer's worker.

## How to reach it

Read [the blank renderer guide](../../../../docs/blank-renderer.md) for the current API and limits. Run `pwsh -File ./scripts/verify.ps1` from the checkout being verified. Use `-Software -SkipNative` when no interactive desktop is available and report that native scenario as skipped.

## Drive and observe

The script checks format, Clippy, the public-contract test, Rustdoc, dependency boundaries with a negative fixture, and both consumer builds. The headless run captures and checks all pixels, resizes, checks retained frames across resize/shutdown, and rejects invalid dimensions. It checks 32 same-size requests for allocation reuse, saturation at three outputs and resumed rendering after release. The native run checks readiness identity, captures the back buffer, inspects idle behavior, resizes, captures again, requests stop and verifies process exit. Output directories are unique, and cleanup targets only the owned process handle.

## Evidence and limits

Read the run manifest, consumer reports, capture PNGs, source/executable hashes, action transcript and stderr in its `artifacts/` directory. Native capture values must be `[16,24,32,255]` throughout. Captures use diagnostic readback; native captures establish the swap-chain copy path, not final desktop composition. One idle process sample is not a scene-performance benchmark. The [bounded output pool](../../../../docs/frame-pool.md) limits worker-owned textures to three, including retained sizes. Holding all three leases pauses rendering until release. This does not bound scene assets, consumer or driver memory.

The [native report](../../../../artifacts/20261009-233823-61aa2b4f/native/report.json) records GPU-pixel checks, resize, the idle sample and owned-process exit. [Doctor checks](../../../../artifacts/20261009-233823-61aa2b4f/doctor-checks.json) confirm fresh blank readiness, rejection of a temporary source-set change, and rejection of scene verification even when its path exists.

After the Rust guardrail additions, the [hardware run](../../../../artifacts/20261009-235011-ab2313c5/manifest.json) and [WARP/headless run](../../../../artifacts/20261009-235042-24866331/manifest.json) passed again on 2026-10-09. Both retain `boundaries.json` confirming rejection of the forbidden dependency fixture. Native verification passed in the hardware run and was explicitly skipped in the WARP run.

Blank checks passed again with [the motion fixture build](../../../../artifacts/20261010-000732-ac8c713b/manifest.json) and [its WARP/headless route](../../../../artifacts/20261010-000752-c46add9f/manifest.json) on 2026-10-10. See [diagnostic motion](diagnostic-motion.md) for the additional temporal evidence and limits.

Bounded frame exchange was verified on 2026-10-10 by the [hardware/native gate](../../../../artifacts/20261010-010846-85f196dd/manifest.json) and [WARP/headless gate](../../../../artifacts/20261010-011028-f819e26f/manifest.json). Native presentation passed in the hardware run and was explicitly skipped in the WARP run. See [frame exchange verification](../../../../docs/frame-pool.md#verification) for allocation, saturation, queued-copy and timing evidence. These receipts establish diagnostic output behavior, not scene compatibility.
