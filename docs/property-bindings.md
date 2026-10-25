# Scene property bindings

Scene playback resolves direct checkbox visibility, combo visibility, text
strings and text colors. Checked clock/calendar profiles keep their script
metadata instead of freezing at the saved text value. Their nested format,
seconds and delimiter options can link to user properties too.

This follows Wallpaper Engine's documented
[visibility and numeric/color links](https://docs.wallpaperengine.io/en/scene/first/properties.html).
The stock text behaviors are independent Rust implementations selected by exact
script identity. The core does not execute JavaScript. Unknown scripts retain
their saved values under tolerant playback and remain named omissions.

## Prepare user options

Call `ImagePlayback::open_scene_images_with_properties(source, settings, &values)`
with a `BTreeMap<String, serde_json::Value>`. Values are raw JSON values rather
than `{ "value": ... }` wrappers:

```rust
let values = std::collections::BTreeMap::from([
    ("clock".to_owned(), serde_json::json!(true)),
    ("_24h".to_owned(), serde_json::json!(false)),
]);
let replacement = wallpaper_kit::ImagePlayback::open_scene_images_with_properties(
    &source, settings, &values,
)?;
```

Each scene defines its property names. Checkbox links require Booleans. Combo
visibility compares a string with the authored condition. Text and color links
require strings; colors contain three normalized RGB components separated by
spaces. Text is limited to 256 ASCII characters, including spaces and newlines.
Checked clock/calendar option types retain their existing restrictions.

Unspecified options use each binding's authored `value`. The library does not
implicitly read adjacent `project.json` or Wallpaper Engine's saved user settings.
A host can pass those values explicitly. At most 128 property keys are admitted;
keys contain 1..=64 ASCII letters, digits or underscores. String inputs are
limited to 256 bytes. Finite numeric values can be retained for unconsumed host
options; this does not implement arbitrary slider-driven scene behavior.

Preparation reads assets, decodes textures and builds font atlases. Prepare a
replacement on a background worker, retaining the current playback and frame
until preparation succeeds. Restore its scene clock with `seek`, then resume
with `play` when appropriate. Separate preparations can temporarily retain both
sets of resources; the documented budgets apply to each playback. Sampling
performs no font, script or asset I/O. Hiding all bound layers yields the scene
background, including when their authored visibility defaults are all false.

`report.user_properties` records the supplied values. `report.resolved_bindings`
records recognized links and checked text profiles. Recognition happens before
visibility and optional-object preparation, so it does not prove that every
referenced object could render. `report.text_objects` records prepared text;
`sample.text_values` records the strings submitted in that sample.

## Browser checks

The debug consumer accepts incremental `window.wallpaperTest.setProperties({...})`
updates. It prepares the replacement in its native process and swaps after
success. A rejected preparation keeps the prior playback. The page also exposes
`setCalendarTime(unixMilliseconds)` for repeatable text captures; ordinary
playback uses the local wall clock independently of scene animation time.

```powershell
python -B scripts/inventory-bindings.py 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960' artifacts/binding-inventory-new
python -B scripts/prepare-binding-browser.py artifacts/binding-fixtures-new
cargo build --locked --example browser-worker
node tools/browser-test/verify-bindings.mjs --fixtures artifacts/binding-fixtures-new --engine-assets 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets' --output artifacts/binding-reference-new
```

Add `--hardware` for the hardware renderer. Output directories must be new and
under ignored `artifacts/`. The generator uses original image/scene data, a local
font and checked installed script/blur identity inputs. Those local inputs stay
out of commits. Only the explicitly checked script identities execute in Chrome
as formatting references. Wallpaper-kit itself uses Rust formatting.

The [WARP reference](../artifacts/bindings-browser-software-verified-20261025/report.json)
and [hardware reference](../artifacts/bindings-browser-hardware-verified-20261025/report.json)
pass nine cases. They compare eight installed clock/calendar profiles at six wall
times, including midnight, noon, a date boundary and leap day. Tests change
nested 12/24-hour and seconds options, check exact option/time rewind, and compare
the native glyph pixels with independently supplied literal text. Color checks
include precise-blurred text. Invalid visibility, color and format changes keep
the previous settings and pixels. Resource limits and owned-process cleanup pass.

The [actual lake scene](../artifacts/bindings-actual-browser-final-20261025/report.json),
`3299011444`, passes Playwright and Chrome DevTools checks. Its fixed capture
submits `13:04`, `S U N D A Y` and `25 OCT 2026`. DevTools also verifies that
preparing the same properties preserves paused pixels.

The [required software/headless gate](../artifacts/20261025-105223-5116e861/manifest.json)
passes format, Clippy, 67 image contracts, the remaining public contracts,
Rustdoc, dependency boundaries and headless consumers. Native-window checks are
skipped. The [generated browser regression](../artifacts/bindings-default-browser-final-20261025/report.json)
also passes animated sampling and paused property replacement through DevTools.

The [collection audit](../artifacts/workshop-fidelity-bindings-final-20261025/report.md)
retains 65/65 loading. The [delta](../artifacts/workshop-fidelity-bindings-final-20261025/collection-delta.json)
resolves 77 of the original 106 binding fallback notices across all 13 affected
folders. The remaining 29 notices affect seven folders. Previous objects and
effect inputs are unchanged. Arbitrary SceneScript, media-driven text, localized
profiles, Unicode glyphs and scripted positioning remain unsupported. Existing
font, alignment, parent and text-effect limits can still omit optional objects.
Native display cadence, host integration and complete engine fidelity remain
unverified.
