# Codex desktop wallpaper demo

On 2026-10-11, the real Windows Codex desktop UI displayed wallpaper-kit output behind an empty chat. Submitting a short prompt blurred the animated background, and Codex returned a live agent response. Opening New chat restored the clear background. This is a local experiment in the ignored `integrations/codex-demo` directory.

The installed app was copied without changing its executable or final `app.asar`. Its desktop and Codex profiles are separate from the user's ordinary installation. The user explicitly authorized reusing the local sign-in in the isolated profile. Keep that profile private.

## What inspection established

The installed MSIX package is `OpenAI.Codex_26.1007.2314.0_x64__2p2nqsd0c76g0`. Its archive identifies the app as `openai-codex-electron`, version `26.1007.21434`, build `13901`. Packaging metadata names the runtime `owl`. The frontend loads at `app://-/index.html`; an Electron preload exposes `electronBridge`, with desktop host messages separate from the web UI.

[REA](https://github.com/morluto/rea) 6.4.0 was run through its CLI. Its worker required unsandboxed process execution on this host. A full-archive scan was stopped after it inventoried 16,191 source files, most unrelated to this demo. The scoped host-module analysis parsed 34 JavaScript files and retained 1,079 findings. Its graph contains 384 nodes. Expanded semantic coverage was incomplete because of result and heap limits. Missing graph relationships do not establish absence.

The raw evidence and compact summary are `artifacts/codex-re/rea-main-analysis.json` and `rea-summary.json`. The scoped evidence identity is `ev_f6194b5441bed52257ebb48e82082a4f91493a9c61071de96fccad5e61b64cb1`. The extracted-module digest is `1d31097b5897427669c3b82a5efc51d28d30b4fe3230f0c5724bb2ece2e50765`.

An early changed-entrypoint experiment exited before its bootstrap ran. The executable contains an archive hash. That approach was abandoned, and the original archive restored. The final prepare script compares the copied archive's SHA-256 with the installed archive. No binary patches or changes to the installed app are needed.

The copied Owl runtime exposes a renderer CDP endpoint on loopback. A main-process Node inspector was unavailable. The usual CDP window-bounds command also returned `Browser window not found`, so native resize/minimize were checked through the Computer Use plugin.

## How the demo works

The ordinary Codex app owns its chat, prompt submission, and agent response. The experiment inserts a video underlay and its own CSS through the isolated app's loopback debugging endpoint. It detects visible `data-user-message-bubble` elements in the active chat to select clear or blurred appearance. This DOM contract is specific to the inspected build.

The navigation rail, chat sidebar, application toolbar, and composer use the sidebar's 65% dark tint and an 18px backdrop blur. The chat header below the menu bar is transparent and has no backdrop blur. The same blur radius applies to the wallpaper in active chats. Panel insets, outer gutters, and panel corner cutouts are removed so adjoining panels meet. The composer's opaque backing and the top and bottom chat fades are removed to keep the wallpaper visible through the glass.

The wallpaper keeps its viewport framing during blur transitions. Only blur and brightness change; entering a chat does not scale the video.

The central new-chat icon is fully opaque while the wallpaper styling is enabled, so its default 30% opacity does not reduce contrast against the artwork.

A hidden Electron 44.4.5 companion reuses the existing local T3 native adapter source. A Rust worker renders wallpaper-kit's supported scene images at 3840 × 2160. The companion imports leased D3D11 NT-handle frames using Electron's shared-texture API, sizes its canvas to the imported frame, and sends a video-only WebRTC stream to the Codex page. It requests detail-oriented encoding, preserves resolution and allows up to 40 Mbit/s. There are no ICE servers. Offer/answer signaling goes through the local CDP connection. The receiver is muted and has no pointer input. The shared adapter's default remains 1920 × 1080 for callers that omit output dimensions.

The stream is capped at 30 frames per second. A recorded receiver sample reported 20 decoded frames per second and zero dropped frames. That sample does not establish display cadence or host performance. WebRTC encoding/decoding adds work and is not direct texture sharing into Owl. Presentation code performs no explicit CPU pixel readback; verification captures do.

The receiver reports visibility, reduced motion and conversation changes immediately, with a one-second companion check. Hidden content stops native exports and frame polling. Reduced motion retains a still frame. New chat changes the blur state without reloading or resizing the wallpaper source.

## Settings and wallpaper selection

Open the isolated app's profile menu, choose Settings, then Appearance. The Wallpaper section contains background enablement, installed wallpaper selection, a local `.pkg` picker, a Workshop URL field, image quality, animation, visual effects, reduced flashing and freeze during chats. The controls match the inspected settings page's switch and row appearance; the experiment adds its own DOM section through the existing companion connection. It does not modify the app archive or use a supported Codex settings extension API.

Workshop URLs resolve installed packages under the local Steam Workshop root. Install new items through Wallpaper Engine first; entering a URL does not download the package. The local picker accepts scene packages. Preparation runs in the native worker while the active wallpaper remains visible. Failed preparation preserves that wallpaper and restores the selector.

Settings are saved in the ignored `integrations/codex-demo/wallpaper-settings.json` and restored on launch. An explicit `launch-codex.ps1 -Scene` overrides the saved scene for that run. High image quality selects linear sampling; it does not change output resolution. Reduced flashing suppresses all particles and cannot detect arbitrary flashes in image content.

Freeze during chats is enabled by default. A visible user message blurs and pauses the received video, stops frame polling, and freezes the native playback sample associated with the last exported frame. The host retains its last decoded video frame. New chat resumes animation. Turning the preference off permits animation in a blurred conversation. Settings navigation preserves the current conversation's pause state.

## Run and stop

The existing local T3 checkout and Electron runtime are prerequisites. Their build route is documented in the ignored `integrations/t3code.md`. The Codex build script compiles the shared adapter source into its own `codex-demo/wallpaper.node`; it leaves T3's running binary untouched. The scene remains at the local Steam Workshop path, with engine assets in the Wallpaper Engine installation. Neither is bundled.

Double-click `Launch Codex Wallpaper.cmd` at the wallpaper-kit root. It works from any current directory and keeps errors visible if startup fails. The launcher reuses the isolated sign-in and saved wallpaper, prepares a missing runtime copy, rebuilds a missing adapter or one older than its Rust/Cargo inputs, and waits for a connected decoded wallpaper frame before reporting success. If the demo is already ready, launching again brings it forward without creating another pair of processes.

The equivalent terminal command is:

```powershell
pwsh -File integrations/launch-codex.ps1
```

Use `-Restart` to reopen the demo, or `-Rebuild` to rebuild the adapter and reopen it. An explicit `-Scene` still selects a local package for that launch. The prerequisites remain the installed inspected Codex build, the prepared T3 Electron runtime, Node, and the local Wallpaper Engine assets. Rust is needed when rebuilding. The root launcher does not install dependencies or update the inspected app version.

The prepare script's default installed-app path is pinned to the inspected version; `-InstalledApp` can supply an updated path for a new investigation. A newly created isolated profile needs a sign-in. With explicit authorization, `launch-codex.ps1 -ReuseSignIn` copies the existing local CLI sign-in without printing its contents. A profile already provisioned during this experiment needs no second copy.

The launcher refuses to use port 9340 when its owner is outside the recorded demo. Process IDs, paths and exact start times are checked before reuse or restart. Stale IDs are ignored. Close the experiment with:

```powershell
pwsh -File integrations/stop-codex.ps1
```

Stop verifies the recorded process IDs, executable paths, and exact start times before stopping the two owned processes. It does not stop the installed Codex app. Runtime/profile copies remain available for another run. Port 9340 exposes debugging access to the isolated signed-in app while it is running, so use this only as a local demo and stop it after use.

The original demo source files, without runtime binaries, sign-in data, or artwork, are also saved in `artifacts/codex-re/codex-demo-source.zip`. The settings revision and shared native adapter sources are saved separately in `artifacts/codex-re/codex-settings-source.zip`. Extract either archive into `integrations/` to restore its local source layout.

`artifacts/codex-re/codex-4k-source.zip` contains the current 4K/settings demo and shared adapter sources in the same layout. Its adjacent `4k/build.json` records source/binary hashes and the gate receipt. Build against this wallpaper-kit checkout; the archive contains no app runtime, credentials or Workshop content.

The later one-click launcher and integration sources are saved in `artifacts/codex-re/codex-launch-source.zip`, with hashes in `launch-build.json`. Extract this archive into the wallpaper-kit root. It includes the root CMD entrypoint and `integrations/` source layout, without runtime, credentials or artwork.

## Verification

`pwsh -File integrations/verify-codex-launch.ps1` launches through the root CMD entrypoint from another working directory, checks that repeat launch preserves the two process identities, restarts the owned pair, rejects unmatched process identities without stopping them, and confirms the saved wallpaper settings and sign-in file remain unchanged. `artifacts/codex-re/launch-report.json` records the result. This procedure leaves the demo running.

The 4K revision passed `node integrations/codex-demo/verify-resolution.cjs`. `artifacts/codex-re/4k/report.json` records actual native 3840 × 2160 frames, three-lease backpressure, default 1080p compatibility, invalid-size rejection and 3840 × 2160 WebRTC decoder statistics. `received-video.png` captures the full decoded frame; `codex.png` captures its real new-chat UI. A receiver sample reported 25 decoded frames per second and zero dropped frames; the stats span intentional settings pauses and do not establish sustained cadence. WebRTC uses lossy VP8 encoding. The generated core 4K one-pixel-detail test and the full hardware/native repository gate passed in `artifacts/20261011-230208-0f57e32d/manifest.json`.

The fireworks correction respects Gojo's authored `bloom=false` while retaining float HDR accumulation and its refractive ring. The [4K native receipt](../../artifacts/gojo-fireworks-after-4k/report.json) verifies captures, rewind, pause, retained source uploads and cleanup. The [required gate](../../artifacts/20261011-233510-3a0742cd/manifest.json) passed. The local Codex adapter was rebuilt and `verify-resolution.cjs` again verified 3840 x 2160 decoded video and bounded exports.

`node integrations/codex-demo/measure-fireworks.cjs <receipt.json>` measures eight seconds of visible empty-chat video and browser animation callbacks. Add `--disabled` to measure with wallpaper rendering disabled; it restores the prior settings in `finally`. The [final 4K linear-quality sample](../../artifacts/gojo-fireworks-after-4k/codex-final-timing.json) recorded 21.37 decoded fps, no dropped decoded frames and no reported video freezes. Browser animation callback gaps had a 2.9 ms 95th percentile and a 22.3 ms maximum. A [disabled-wallpaper sample](../../artifacts/gojo-fireworks-after-4k/codex-disabled-timing.json) recorded the same 2.9 ms percentile and a 3.3 ms maximum. These short callback observations do not measure typing, scrolling, display cadence or a sustained performance budget. The native nearest-quality runs recorded about 30 presentation calls per second; differing consumer paths and quality settings prevent attributing the entire difference to transport. Faster polling and H.264 experiments did not improve the observed rate and were removed. The final demo retains VP8 and the previous frame polling behavior.

`node integrations/codex-demo/verify-settings.cjs` exercises the actual Appearance controls and the saved real conversation. Its `artifacts/codex-re/settings/report.json` records persisted settings, installed choices, the original Gojo package, linear sampling, VFX and particle policies, still mode, disable/restore, invalid and missing URLs, failed package rollback, conversation freeze and New chat resume. It finishes on Appearance and captures that page. The private adapter's `native-report.json` in the same directory checks three bounded GPU leases, release/resume, hidden pause, one still frame and static remount. Native `.pkg` dialog selection and light-theme appearance remain unverified.

The settings revision passed the hardware/native repository gate in `artifacts/20261011-185323-d7b0abc8/manifest.json`, including 77 Rust tests, formatting, Clippy, Rustdoc, dependency checks and native consumers. The adapter separately passed formatting, Clippy, a release build, JavaScript syntax checks and three settings boundary tests. These checks establish behavior, not host latency or display cadence.

`node integrations/codex-demo/verify.cjs` reopens the saved test conversation, checks its real response and blur, opens New chat, captures changing received video pixels, checks viewport coverage, and verifies reduced motion stops native exports. It writes `artifacts/codex-re/demo-receipt.json` and adjacent screenshots. The saved conversation was generated by a real submitted request, not seeded messages.

The verification also measures panel joins and outer edges, checks the common tint and 18px backdrop blur in both chat states, and confirms the composer backing is transparent. The chat header is checked separately for a transparent background and no backdrop blur. The earlier common-blur checks passed after relaunch; the header exception was subsequently applied and checked live. The additional backdrop blur's performance cost has not been measured.

Native edge dragging resized the window from 1280 by 820 to 1100 by 780. The wallpaper covered the measured viewport. Native minimize stopped exports at frame 7,921 across a three-second interval; restore resumed them. Those separate observations are in `native-window-checks.json`. The final native composition was inspected in the foreground; CDP screenshots capture web content rather than the complete desktop window.

The full repository gate passed on the hardware Direct3D 11 path, including native consumers. Its manifest is `artifacts/20261011-142300-c8832cf9/manifest.json`. The GPU is an NVIDIA GeForce RTX 5080. The core renderer was unchanged by this demo.

Captured states include `verified-empty.png`, `verified-response.png`, `verified-resize.png`, and two received-video frames. Cleanup and relaunch through the owned-process scripts were executed successfully.

## Limits

### Gojo Satoru comparison

The initial comparison below predates older-package and particle support. The settings revision loads Workshop item [2778759553](https://steamcommunity.com/sharedfiles/filedetails/?id=2778759553) directly, including its two base images and supported particles. Its native report records remaining omissions; this does not establish complete reference fidelity.

At the time of the initial comparison, the original `PKGV0016` package and version-1 scene were rejected by the loader. Its animation uses particles, pulse, water waves, and shake, which that renderer revision did not implement.

The ignored `integrations/codex-demo/prepare-gojo.cjs` creates an explicit static image approximation under `artifacts/codex-re/gojo-image-profile`. It retains the authored city and Gojo image placement, repacks the index as `PKGV0024`, supplies a version-5 image-only scene, and approximates the old `genericimage2` materials with the supported SDR image profile. The package index layout was checked against the public [RePKG reader](https://github.com/notscuffed/repkg/blob/master/RePKG.Application/Package/PackageReader.cs). The old PNG containers declare padded GPU allocation extents; the script normalizes those extents to their authored image/base-mip dimensions. It asserts all pixel payloads and the Workshop original remain unchanged. The profile receipt records every conversion and omission.

```powershell
node integrations/codex-demo/prepare-gojo.cjs
pwsh -File integrations/stop-codex.ps1
pwsh -File integrations/launch-codex.ps1 -Scene artifacts/codex-re/gojo-image-profile/scene.pkg
node integrations/codex-demo/verify.cjs --static
```

The native frame/lease smoke check passed in `gojo-native-receipt.json`. The real Codex clear and blurred states, fixed framing, panel joins, package hash, image IDs `18, 14`, and reduced-motion pause passed in `gojo-demo-receipt.json`. `gojo-clear.png` and `gojo-blurred.png` show the comparison. It is static, with stars, shooting stars, fireworks, clock, authored image effects, camera parallax, and HDR/color processing omitted. It does not establish original Gojo scene compatibility or visual fidelity. The launcher's default remains the earlier Yuta scene.

The scene uses the existing seven-image playback implementation and its implemented sprite, cloud, and blur behavior. Full authored scene fidelity, omitted effects, parallax, audio, and camera/color postprocessing remain outside this demo's evidence.

UI navigation and page reloads that replace the instrumented document require reconnecting the companion. The inspected build's new-chat, saved-conversation and Appearance navigation keep the document and work. Additional windows, light theme, modal readability, other settings pages, and future Codex versions have not been accepted.

CPU/GPU/memory budgets, typing and scrolling latency, encoder/decoder acceleration, display timing, and sustained-load behavior remain unmeasured. This proves the requested observable flow in the actual app; it does not establish a supported Codex extension API or a production adapter.
