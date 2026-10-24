# Browser tests

## Status

Implemented and verified with the generated fixture, Workshop scene `2194681562`
and video scene `2630394084`. See [browser verification](../../../../docs/browser-testing.md)
for receipts and setup. The required software/headless gate passed; native
window scenarios were skipped in that run.

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
`capture` and `pixel`. Tests compare native/canvas hashes and independent sprite
colours, rewind, live advancement, paused inactivity, bounds, error recovery,
MCP snapshots/screenshots and process cleanup. Retain the run's `report.json`,
`renderer-transcript.json`, `trace.zip`, screenshots and `cleanup.json`.

## Evidence and limits

The browser displays lossless PNG readbacks from a headless D3D11 consumer. One
image response and at most four native commands can be in flight. Test transport
is capped at 30 requests/s and is not evidence of native display cadence, GPU
sharing efficiency, host application responsiveness or exact engine fidelity.
Workshop/engine assets and captures remain local and ignored.
