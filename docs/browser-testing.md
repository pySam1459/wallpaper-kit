# Browser verification

The test integration runs wallpaper-kit's native renderer in a headless child
process and displays its completed frames in Chrome. Playwright and the official
Chrome DevTools MCP server can inspect it without a desktop viewer. Scene
rendering stays in the Rust library.

## Run the tests

Requires Windows, Rust/MSVC, Node 24 or newer, and installed Google Chrome.
Install the pinned development dependencies and test the generated fixture:

```powershell
pwsh -File scripts/verify-browser.ps1 -Install
```

Subsequent runs omit `-Install`. WARP software rendering is the default. Test a
local scene on the hardware adapter:

```powershell
pwsh -File scripts/verify-browser.ps1 -Hardware -ScenePath 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/2194681562/scene.pkg'
```

Use `-EngineAssets` for another engine installation. Videos need the existing
FFmpeg prerequisites. Each run creates an ignored `artifacts/browser-*` folder
with native transcripts, screenshots, a Playwright trace, MCP results, source
hashes, omissions and a process cleanup receipt.
Each session runs a pinned copy of the renderer executable and records its hash,
so an open browser does not prevent Cargo from rebuilding. Reconnect an MCP
session after rebuilding to inspect the new renderer.

The fixture checks independent sprite colours at 0 and 125 ms and compares the
entire browser canvas with the native RGBA hash. Tests cover seek/rewind, live
advancement, pause, paused GPU inactivity, resize, invalid commands, startup
errors, bounded requests and cleanup. DevTools MCP attaches to the same Chrome
for script evaluation, accessibility snapshots and screenshots. The test also
exercises the standalone MCP entry point and cleanup on client disconnect.

For the generated particle turbulence probe, add `-TurbulenceFixture` with its
`-ScenePath`. It checks independent force/mask pixels and 121 controlled samples
at approximately 120 Hz. See [the particle procedure](particle-profiles.md#browser-turbulence-approximation-24-october)
for generation commands and fidelity limits. Actual-scene runs also retain
canvas screenshots at 0 and 1500 ms before testing resizing.

## Use Chrome DevTools MCP

The [Foliage Sway](foliage-sway.md#browser-verification) and
[Water Ripple](water-ripple.md#browser-verification) reference procedures
also execute installed effect shaders locally in headless Chrome WebGL and
compares it with native pixels. Its generated inputs are original; installed
shader source is read locally and kept out of commits. This tests an isolated
effect pass, independently of the complete Wallpaper Engine application.

Register the entry point in this project's ignored Codex configuration:

```powershell
pwsh -File scripts/setup-browser-mcp.ps1
```

The installer preserves other settings and an existing `wallpaper_kit_browser`
server. Absolute local paths make startup independent of the client's working
directory. Project configuration requires a trusted project; see the
[official MCP reference](https://learn.chatgpt.com/docs/extend/mcp).
Reload the connection or start a new coding session to load its tools.

Other MCP clients can launch `node <repo>/tools/browser-test/mcp.mjs`. It starts
an isolated headless Chrome and renderer, then forwards the official DevTools
MCP protocol over stdio. Add `--headed` to watch the window or `--software` for
WARP. To select a wallpaper, pass `--scene <scene.pkg> --engine-assets <assets>`
and reconnect. Personal Chrome profiles are not opened. DevTools usage
statistics, update checks and performance CrUX requests are disabled.

Call `list_pages` and use the returned ID for “wallpaper-kit browser test” with
`evaluate_script`. The page exposes:

```javascript
await window.wallpaperTest.ready;
await window.wallpaperTest.seek(1500);
await window.wallpaperTest.resize(1280, 720);
await window.wallpaperTest.play();
await window.wallpaperTest.pause();
await window.wallpaperTest.capture();
await window.wallpaperTest.inspect();
await window.wallpaperTest.setProperties({ clock: true, _24h: false });
await window.wallpaperTest.setCalendarTime(Date.UTC(2026, 9, 25, 13, 4, 5));
window.wallpaperTest.pixel(40, 100); // displayed native RGBA
```

Use snapshot, screenshot, console and network tools on the same page. Clients
saving files through DevTools MCP must provide a workspace root containing the
destination. The verifier grants only its evidence directory. Disconnecting
stops the owned Chrome, HTTP server and renderer.

`setProperties` merges a bounded update and prepares a replacement playback in
the native debug process. Failed preparation retains the previous playback.
`setCalendarTime` fixes wall time for text captures and pauses scene playback.
Property names vary by scene. See [property binding verification](property-bindings.md)
for the installed-script formatting oracle and literal-text pixel comparison.

For manual browsing, run `pwsh -File scripts/start-browser-test.ps1` and open its
printed local URL in Chrome. Scene and software arguments match the verifier.
Ctrl+C stops the server and renderer. The HTTP server binds only to `127.0.0.1`,
uses a fresh session token and rejects cross-origin API requests.

## Interpretation and evidence

PNG readback is a debug transport, capped at 30 browser requests/s with one frame
in flight. The native process owns loading, decoding, GPU waits and PNG encoding.
Paused pages stop requesting frames. Commands have a four-request bound and a
timeout; image responses are bounded to 40 MiB.

Browser traces measure this page and transport. Use native consumers for display
cadence and GPU sharing checks, and separate Wallpaper Engine reference evidence
for fidelity comparisons. The core has no Chromium, Node or MCP dependency.

Verified on 24 October 2026:

- [Fixture and standalone MCP lifecycle](../artifacts/browser-1792863672788-a4e959/report.json).
- [Hardware image/particle scene](../artifacts/browser-1792863816136-6f30a2/report.json).
- [Hardware Japanese Summer Tram video](../artifacts/browser-1792863817365-dcec5f/report.json).
- [Required software/headless gate](../artifacts/20261024-182615-9d235504/manifest.json), with native window checks skipped.

For sequential image-pass verification, use the
[ordered-effects procedure](image-effect-chains.md#browser-verification).
`verify-effect.mjs --effect chain` compares the native renderer with installed
GLSL passes executed in Chrome. It checks pass order and optional-pass fallback,
in addition to the individual [Foliage Sway](foliage-sway.md) and
[Water Ripple](water-ripple.md) references.

For accumulated-scene effects, use the
[composition-layer procedure](composition-layers.md#reproduce-browser-verification).
`verify-effect.mjs --effect composition` also checks transformed capture,
foreground order, authored source resolution, resize and HDR. Its independent
capture oracle feeds the installed GLSL passes; it does not reuse native captures
as reference inputs. Native-size tolerance is two channel values and resized
tolerance is three, with both counts retained in reports.

For brightness, tint and alpha pulsing, use the
[Pulse procedure](pulse.md#reproduce-browser-verification).
`verify-effect.mjs --effect pulse` runs installed Pulse and blending shader
sources in Chrome. It covers masks, noise, both checked blend modes, transparency,
HDR, composition capture and resize, with a two-channel-value tolerance.
