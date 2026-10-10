# scene-renderer

An open source renderer for high-quality photos, videos, and animated Wallpaper Engine scenes, designed for application backgrounds and other embedded content.

The project starts on Windows. The consuming application's responsiveness takes priority over background animation.

## Status

The Rust library renders blank Direct3D 11 textures and a controlled-time diagnostic animation through a bounded reusable output pool. It also inspects local scene packages and their asset dependencies through a read-only public API. A native Windows viewer and headless verification runners consume its public API. Scene rendering, production animation, photo/video playback, and framework adapters remain unimplemented.

The [renderer specification](docs/spec/scene-renderer.md) defines the architecture, dependency choices, delivery gates, and measurable MVP acceptance criteria.

## Build and run

Requires Windows, Rust 1.91 or newer with the MSVC toolchain and C++ build tools, and PowerShell 7 for verification scripts. Hardware runs require a Direct3D 11 feature-level 11.0 adapter. Software WARP runs are available explicitly for CI.

```powershell
cargo run --locked --example native-viewer
cargo run --locked --example headless
cargo run --locked --example motion-checks
```

The viewer displays a dark blank background, supports ordinary window resizing, and exits with Escape or the close button. The headless runner verifies GPU pixels, idle behavior, resize, and frame lifetime. Each run creates its own evidence directory. Pass `-- --software` to a Cargo run command to select WARP; there is no silent hardware fallback.

Run all source checks and consumer scenarios:

```powershell
pwsh -File ./scripts/verify.ps1
```

For a machine without an interactive desktop:

```powershell
pwsh -File ./scripts/verify.ps1 -Software -SkipNative
```

Evidence is retained under `artifacts/`, which Git ignores. The [blank renderer verification guide](docs/blank-renderer.md) explains the API, control protocol, evidence, and limits. The Windows CI workflow runs the software/headless route; the workflow itself has not yet run on GitHub.

The [motion verification guide](docs/motion-verification.md) describes fixed-time GPU captures, independent pixel checks, verifier fault cases, and separate live timing. These give agents evidence about motion without relying on a model to judge a video. Display timing, production scene fidelity, and host responsiveness still need additional measurements.

The [frame exchange guide](docs/frame-pool.md) describes the three-output limit, retained-frame backpressure, nonblocking completion and verified GPU reuse.

## Project requirements

- Display high-quality photos, play videos, and render existing Wallpaper Engine scenes from Steam Workshop.
- Keep CPU usage, GPU work, and memory bounded while preserving visual quality.
- Treat Windows as a primary development and testing platform, with native execution.
- Provide an embeddable renderer with adapters for applications built with different technologies.

Scene compatibility grows incrementally. Supporting one scene does not imply support for every Workshop scene.

## First milestone

The MVP is to render [Yuta Okkotsu | Jujutsu Kaisen](https://steamcommunity.com/sharedfiles/filedetails/?id=3780699842), Workshop item `3780699842`, on Windows.

Completion requires the following evidence:

- Load the scene's actual package and assets.
- Reproduce its visible layers, effects, and animation, rather than showing its preview image.
- Compare the output against Wallpaper Engine for visual fidelity and playback behavior.
- Measure CPU usage, GPU frame time, and memory on Windows, with the test hardware and settings recorded.

The [initial package inventory](docs/research/example-scene-inventory.md) identifies two sprite sequences, particles, parallax, and four effect types. Their large texture pages make bounded streaming part of the MVP. Completing the transitive asset inventory and validating behavior against Wallpaper Engine remain implementation gates.

General photo and video support, more scene formats and features, and additional framework adapters follow this milestone. Embedding feasibility should be tested early to avoid building a renderer that only works in its own window.

## Scene inspection

The [scene inspection guide](docs/scene-inspection.md) describes the public API, command, limits and real-package verification. The inspector checks PKG/TEX metadata, follows asset references, records hashes, and reports missing or unsupported features. It preserves object order and effect bindings. This does not render the scene or decode its image payloads.

## Example app and verification

The renderer is the product. A minimal example app is a consumer of the library, used by developers and agents to view output, exercise playback, and gather evidence.

The example app uses the same public API available to downstream applications. Rendering and scene behavior belong in the library. The app provides only the controls and automation needed to load a scene, resize, pause, resume, capture output, and collect metrics.

Repeatable scenarios should record their scene, build, hardware, and settings alongside visual output and performance results. Small fixtures can use fixed time and random seeds where supported. Real scene verification also checks animation over time. Measuring the example app does not establish performance or compatibility inside every host framework.

Agent workflows follow the useful principles of structural enforcement, repeatable verification, and accountable review. Reference frameworks inform the design without expanding this project into a full application framework.

## Performance approach

Background rendering must not block the consuming application's UI thread. The host uses the latest completed frame while the renderer drops stale frames or retains the previous frame when behind.

The intended design caps frame rate, render resolution, particles, and effect work. It reuses GPU resources, avoids routine GPU-to-CPU frame transfers, and performs asset decoding and shader compilation away from the UI thread. Static photos require no continuous animation timer. Hidden or minimized backgrounds pause, and constrained workloads reduce background quality or animation frequency.

Threads and separate processes still share hardware resources. Zero impact cannot be guaranteed for arbitrary scenes. Performance claims require measurements, including host input and scrolling latency with the background disabled and enabled. Integrated graphics are an important test target.

## Architecture direction

The core is a Rust library with a diagnostic Direct3D 11 backend and scene-inspection modules. Production asset streaming, scene state and animation follow later. GPUI remains an optional integration layer rather than a core dependency. JSON parsing and hashing are library dependencies for inspection; Winit and PNG capture tooling remain example-only. A checked Cargo dependency boundary enforces the host separation.

The blank prototype verifies same-adapter Direct3D 11 texture sharing into an independent native consumer device. This does not settle production scene performance or GPUI integration. Media Foundation remains a candidate for video decoding; `wgpu` is a portability alternative, and raw Vulkan is not a committed requirement.

The integration direction is a Rust API, a C-compatible library interface for other languages, and optional framework adapters:

- GPUI consumes GPU frames through a Rust adapter.
- Electron uses a native addon and a JavaScript wrapper to import shared GPU frames where supported.
- WebView2 can host a transparent web interface above a native background.
- Embedded Chromium or CEF needs a native composition adapter.

These are proposed integrations, not supported APIs. Texture lifetime, synchronization, resizing, DPI, visibility, and input belong in the adapters. An ordinary browser page would need a separate browser-compatible backend. A native DLL alone does not provide browser support.

## Compatibility research

Use public documentation, scene data, observable behavior, and appropriately licensed open source references to implement compatibility independently. Existing projects provide useful evidence, but their platform choices and licenses do not determine this project's architecture.

- [Wallpaper Engine shader documentation](https://docs.wallpaperengine.io/en/scene/shader/syntax.html)
- [Open Wallpaper Engine](https://github.com/waywallen/open-wallpaper-engine), GPL-2.0-only
- [Rust Linux Wallpaper Engine](https://github.com/wqLouis/linux-wallpaperengine), GPL-3.0

The project's MIT license does not grant rights to Wallpaper Engine code, engine assets, or Workshop artwork. Keep third-party test content outside the repository unless its license permits redistribution. Review licenses before reusing implementation code, including translations or ports.

## License

Project code and documentation are available under the [MIT license](LICENSE).
