# Workshop scene MVP

## Status

Read-only package/dependency inspection is verified on the recorded Windows build in [the inspection guide](../../../../docs/scene-inspection.md). [Selected TEX page/region decoding/drawing](../../../../docs/texture-pages.md) is also verified on real target assets, including ten individual sprite-frame captures. [Diagnostic image composition](../../../../docs/image-layers.md) adds selected target snapshots with explicit omissions. The [latest native target report](../../../../artifacts/20261010-192159-423f0c4a/native-target-images/report.json) verifies seven-image playback with authored precise blur and cloud motion, including changed pixels at repeated sprite indices and exact time-zero rewind. Complete authored scene rendering remains planned. The [initial package inventory](../../../../docs/research/example-scene-inventory.md) supplies independent expected metadata. Target: [Workshop item 3780699842](https://steamcommunity.com/sharedfiles/filedetails/?id=3780699842).

## Consumer outcome

A consuming application loads the actual scene and displays its visible layers, effects, and animation on Windows. A preview image alone does not meet the MVP.

## How to reach it

For metadata, use the public `inspect_scene(&SceneSource)` API or `scene-inspect` example described in [the guide](../../../../docs/scene-inspection.md). The required gate accepts paired `-ScenePath` and `-EngineAssets` arguments to run the actual target inspection. Inspect counts, object order, hashes, effect bindings, sprite pages/durations and diagnostics. Generated public tests also check conditional dependency upgrades, normalized references and changed profiles. This produces no rendered scene. [Selected image playback](../../../../docs/image-playback.md) adds a native startup route for all seven image objects, including the cropped wire layer, and both sprite sequences, with exact seek/play/pause controls and explicit omissions. The viewer calls `ImagePlayback::open_scene_images` without target object IDs. Image discovery preserves authored order and reports omitted composition/non-image content. The [checked precise-blur profile](../../../../docs/precise-blur.md) applies the wire and first bird's authored scales. The [cloud-motion profile](../../../../docs/cloud-motion.md) also applies both instances and the painted mask. Light shafts, chromatic aberration, particles, parallax and complete scene loading remain planned. Approximate component references do not establish full scene fidelity.

## Drive and observe

The requested next compatibility task is [puppet animation for Workshop item 3013169753](../../../../docs/scene-rendering-plan.md#next-compatibility-task-puppet-animation). Its `PKGV0018` package uses the checked index layout, but image preparation rejects object `101`'s `animationlayers` and its puppet model remains unsupported. The plan records the observed asset/layer identities and requires generated deformation tests, a Wallpaper Engine motion comparison and the actual Codex URL-loading route. This is planned compatibility work, not another verified scene.

[Light-shafts reference groundwork](../../../../docs/research/light-shafts.md) adds generated local Wallpaper Engine probes and a measured receipt for geometry, noise channels, gradient RGB/alpha, masks and isolated target motion. These are reference observations; the library still omits the light-shaft shape. Ray/noise equations, general perspective interpolation and blend semantics remain unresolved.

Inventory required features and dependencies first. Load the package through the public API used by the viewer. Observe explicit load completion or actionable errors, including unsupported features. Compare layers, composition, effects, and animation over multiple times against Wallpaper Engine with matching settings. Exercise interaction if the actual scene requires it.

## Evidence and limits

Package indexing accepts `PKGV0016`, `PKGV0018`, `PKGV0022` and `PKGV0024`. The `PKGV0022` route was exercised through `scene-inspect` against Workshop item `3521344055`; [the inspection guide](../../../../docs/scene-inspection.md) records its receipt and separate texture/effect/text findings. Generated contracts retain malformed-index rejection for each added signature. Package acceptance does not establish scene rendering compatibility.

Retain the package inventory, source identity, settings, reference output, rendered captures or recording, and logs. Account for timing and randomness instead of asserting identical frames without controlled inputs. Use small redistributable fixtures to isolate failures. Keep third-party assets out of commits unless redistribution is permitted.
