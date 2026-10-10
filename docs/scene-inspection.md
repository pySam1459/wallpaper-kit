# Scene inspection

The library's `inspect_scene(&SceneSource)` reads a local package and explicit engine asset root without extracting files or creating a graphics device. Run it on a tool or worker thread: indexing, full-content SHA-256 hashing and JSON parsing perform blocking I/O. Scene rendering remains unavailable.

## Run the public consumer

From the repository root:

```powershell
cargo run --locked --example scene-inspect -- `
  --package 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg' `
  --assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'
```

Use `--output <empty directory>` to choose an evidence directory. The default is a unique directory under `artifacts/`. `report.json` contains the typed library report; `summary.json` records elapsed inspection time and counts. Fatal inspection failures produce `failure.json`. Argument errors return before inspection. Exit codes are `0` for inspected metadata with no blocking findings, `2` for a report containing errors or unsupported profiles, and `1` for a fatal or command error.

`rendering_supported` is always false. `has_blocking_findings()` reports inspection problems, not render readiness. Informational muted-audio findings and warnings for unresolved conditional references are distinct from required missing assets.

## What is checked

The package reader accepts `PKGV0024`, validates UTF-8 relative paths, duplicate paths, entry bounds and overlapping payloads, and prefers package entries over engine assets. It resolves engine references within the canonical root and rejects resolved paths escaping it. Source handles permit read sharing and exclude writes while open; the package handle remains open throughout traversal. Keep the engine installation stable during a run, since this is not an atomic snapshot of all external assets.

The scene reader requires version 5 and a nonzero orthographic canvas. It validates object identities, content selectors, vector fields, basic visual flags and finite shader constants. It normalizes asset references, preserves object order and recognizes project-layer composition. Models, materials, particle definitions and effects introduce graph edges. Shader files contribute include names and sampler defaults. Unrecognized general and definition fields, object behavior, material shaders/states/combinations, effect instance overrides and particle component names produce diagnostics. Known effect profiles also check shaders, material pass counts, blur directions, targets and bindings.

Dependency discovery is conservative. The report includes declared dependencies, shader headers and potentially inactive sampler defaults. Required reachability is resolved after discovery: conditional ancestry propagates through included files, and a later required reference upgrades shared descendants. Binary references cannot suppress later typed validation; conflicting typed uses produce a diagnostic. The inspector does not evaluate the entire GLSL preprocessor, select final sampler bindings or prove shader-source equivalence. Recorded hashes provide inputs for later compatibility profiles. A clean report establishes this metadata inspection only; complete parameter semantics and nested particle/gizmo validation remain part of runtime compilation.

TEX indexing accepts `TEXV0005`/`TEXI0001`, required RGBA/BC3/R8 layouts, `TEXB0001` through `TEXB0004` without embedded video, raw/PNG/LZ4 payload metadata, and `TEXS0003` sprite records. It checks mip dimensions/lengths, page identities, finite positive frame durations and atlas rectangle bounds. Payload bytes are skipped during indexing and streamed during hashing. PNG/LZ4 data is not decoded, so corruption within a payload can still require a decoding-stage diagnostic.

Limits bound metadata work: 65,536 package entries, a 16 MiB index, 4 MiB JSON documents, 1 MiB shader metadata, 4,096 scene objects/assets, 65,536 graph edges, bounded graph depth, 256 texture pages, 16 mip levels and 65,536 sprite frames. Source texture dimensions have their own 16,384 bound; the output `Size` limit is unchanged. Hashing uses a 64 KiB buffer. These are inspection limits, not production playback memory budgets.

## Verify the actual target

The required gate includes generated public-API tests. Supply both local roots to add the real target inspection:

```powershell
pwsh -File ./scripts/verify.ps1 `
  -ScenePath 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842/scene.pkg' `
  -EngineAssets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'
```

Without the roots, the proprietary-content scenario is recorded as skipped. CI still runs generated fixture tests. WARP and `-SkipNative` remain explicit choices for the diagnostic GPU scenarios; inspection itself uses no GPU.

The target verification checks the independently inventoried package hash, 42 entries, 3840 x 2160 canvas, authored object order, ten visual objects, four effects, both 480-frame sequences, 27/30 sprite pages and 28.8-second durations. It checks the blur's retained `previous` input, reports muted audio, rechecks package identity after inspection, and independently hashes resolved external assets. The receipt binds evidence to source and executable hashes. A changed Workshop package requires re-inventory before changing the acceptance fixture.

Generated tests cover package/path/range failures, malformed scene metadata, shared sprite assets, binary-first references, conditional dependency upgrades, include cycles, normalized composition/effect references, unknown fields, changed effect profiles, shader combinations, sprite timing/bounds, package precedence and ordered composition/blur bindings. Fixtures contain synthetic bytes only. Keep proprietary assets and reports with third-party metadata under ignored evidence directories.

The hardware gate passed on 10 October 2026, including 17 public inspection tests, real-package inspection and native/headless diagnostic regressions. The local [manifest](../artifacts/20261010-004928-00600186/manifest.json) and [inspection verification](../artifacts/20261010-004928-00600186/scene-inspection/verification.json) record exact source/binary identities and limits. This run resolved 63 assets, including 21 engine assets, and indexed 12 textures. These ignored evidence files may be absent in another checkout; rerun the command there.

The binary layout interpretation was checked against the MIT-licensed [RePKG format readers](https://github.com/notscuffed/repkg/tree/master/RePKG.Application). The Rust readers are independently implemented. `sha2` uses its MIT-or-Apache-2.0 license; pinned dependency identities are in `Cargo.lock`.

## Next boundary

With [bounded frame exchange](frame-pool.md) and [TEX page decoding/drawing](texture-pages.md) implemented, the [implementation plan](scene-rendering-plan.md) continues with runtime scene compilation, authored layers, playback streaming and effects. Inspection itself remains metadata-only; the separate decoder does not yet produce a complete runtime scene or establish scene fidelity. Native reference comparisons and performance acceptance remain separate gates.
