# Workshop scene MVP

## Status

Read-only package/dependency inspection is verified on the recorded Windows build in [the inspection guide](../../../../docs/scene-inspection.md). [Selected TEX page/region decoding/drawing](../../../../docs/texture-pages.md) is also verified on real target assets, including ten individual sprite-frame captures. [Diagnostic image composition](../../../../docs/image-layers.md) adds selected target snapshots with explicit omissions. The [latest native target report](../../../../artifacts/20261010-180918-03edde55/native-target-images/report.json) verifies seven-image playback with authored precise blur. Complete authored scene rendering remains planned. The [initial package inventory](../../../../docs/research/example-scene-inventory.md) supplies independent expected metadata. Target: [Workshop item 3780699842](https://steamcommunity.com/sharedfiles/filedetails/?id=3780699842).

## Consumer outcome

A consuming application loads the actual scene and displays its visible layers, effects, and animation on Windows. A preview image alone does not meet the MVP.

## How to reach it

For metadata, use the public `inspect_scene(&SceneSource)` API or `scene-inspect` example described in [the guide](../../../../docs/scene-inspection.md). The required gate accepts paired `-ScenePath` and `-EngineAssets` arguments to run the actual target inspection. Inspect counts, object order, hashes, effect bindings, sprite pages/durations and diagnostics. Generated public tests also check conditional dependency upgrades, normalized references and changed profiles. This produces no rendered scene. [Selected image playback](../../../../docs/image-playback.md) adds a native startup route for all seven image objects, including the cropped wire layer, and both sprite sequences, with exact seek/play/pause controls and explicit omissions. The viewer calls `ImagePlayback::open_scene_images` without target object IDs. Image discovery preserves authored order and reports omitted composition/non-image content. The [checked precise-blur profile](../../../../docs/precise-blur.md) applies the wire and first bird's authored scales. Other effect dispatch and complete scene loading remain planned.

## Drive and observe

Inventory required features and dependencies first. Load the package through the public API used by the viewer. Observe explicit load completion or actionable errors, including unsupported features. Compare layers, composition, effects, and animation over multiple times against Wallpaper Engine with matching settings. Exercise interaction if the actual scene requires it.

## Evidence and limits

Retain the package inventory, source identity, settings, reference output, rendered captures or recording, and logs. Account for timing and randomness instead of asserting identical frames without controlled inputs. Use small redistributable fixtures to isolate failures. Keep third-party assets out of commits unless redistribution is permitted.
