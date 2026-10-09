# Workshop scene MVP

## Status

Scene rendering is planned. The [initial package inventory](../../../../docs/research/example-scene-inventory.md) is complete, and blank consumers exist; they do not load this scene. Target: [Workshop item 3780699842](https://steamcommunity.com/sharedfiles/filedetails/?id=3780699842).

## Consumer outcome

A consuming application loads the actual scene and displays its visible layers, effects, and animation on Windows. A preview image alone does not meet the MVP.

## How to reach it

Use a local copy of the target package and its required assets. Discover the input path without changing the originals. The loader API and viewer controls will be recorded after implementation.

## Drive and observe

Inventory required features and dependencies first. Load the package through the public API used by the viewer. Observe explicit load completion or actionable errors, including unsupported features. Compare layers, composition, effects, and animation over multiple times against Wallpaper Engine with matching settings. Exercise interaction if the actual scene requires it.

## Evidence and limits

Retain the package inventory, source identity, settings, reference output, rendered captures or recording, and logs. Account for timing and randomness instead of asserting identical frames without controlled inputs. Use small redistributable fixtures to isolate failures. Keep third-party assets out of commits unless redistribution is permitted.
