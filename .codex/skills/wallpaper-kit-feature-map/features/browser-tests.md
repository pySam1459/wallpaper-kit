# Browser tests

## Status

The [Pulse route](../../../../docs/pulse.md) passes 18 WARP/hardware reference
cases within two channel values using installed effect and blending sources.
It covers tint, alpha, noise, masks, HDR, composition capture and resize. Actual
`2374964197` and `2288914800` pass Playwright/DevTools controls, bounds and owned
cleanup. Frozen motion and corrupt pixels fail the shader oracle. Audio-driven
profiles, complete engine runtime agreement and native cadence remain open.

The [composition-layer route](../../../../docs/composition-layers.md) passes nine
WARP/hardware cases with an independent capture/placement oracle and installed
GLSL passes. It checks full-screen and rotated adjustable regions, authored
capture resolution, foreground ordering, masks, HDR, resize and rewind. Native
comparisons allow two channel values; resized comparisons allow three and
retain counts above two. Actual Gojo and adjustable scenes pass Playwright and
Chrome DevTools controls, bounds and owned cleanup. Native cadence remains open.

Implemented and verified with the generated fixture, Workshop scene `2194681562`
and video scene `2630394084`. See [browser verification](../../../../docs/browser-testing.md)
for receipts and setup. The required software/headless gate passed; native
window scenarios were skipped in that run.

The [property-binding route](../../../../docs/property-bindings.md) also passes
nine WARP/hardware cases with installed-script formatting and literal-text pixel
oracles. Actual `3299011444` passes Playwright/DevTools controls and replacement
preparation while paused. It retains the frame hash and cleans up owned processes.

## Consumer outcome

Agents inspect real native renderer pixels, omissions, errors and controls in
Chrome using Playwright or Chrome DevTools MCP. Desktop automation is unnecessary
for this route. The core remains independent of browser and host dependencies.

## How to reach it

Run `pwsh -File scripts/verify-browser.ps1 -Install` once, then omit `-Install`.
Add `-Hardware -ScenePath '<scene.pkg>'` for local content. For MCP sessions,
`scripts/setup-browser-mcp.ps1` registers the project-local entry point; reload
the connection. The standalone `tools/browser-test/mcp.mjs` owns its renderer
and isolated Chrome and exposes the official DevTools tools.

## Drive and observe

Use `window.wallpaperTest.ready`, `inspect`, `seek`, `play`, `pause`, `resize`,
`capture`, `pixel`, `setProperties` and `setCalendarTime`. Tests compare native/canvas hashes and independent sprite
colours, rewind, live advancement, paused inactivity, bounds, error recovery,
MCP property replacement, snapshots/screenshots and process cleanup. Property updates prepare a replacement in the native process; invalid preparation retains the prior playback. Retain the run's `report.json`,
`renderer-transcript.json`, `trace.zip`, screenshots and `cleanup.json`.

## Evidence and limits

The browser displays lossless PNG readbacks from a headless D3D11 consumer. One
image response and at most four native commands can be in flight. Test transport
is capped at 30 requests/s and is not evidence of native display cadence, GPU
sharing efficiency, host application responsiveness or exact engine fidelity.
Workshop/engine assets and captures remain local and ignored.
