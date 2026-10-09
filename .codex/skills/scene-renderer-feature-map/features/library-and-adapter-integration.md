# Library and adapter integration

## Status

The public blank/fixture API and native/headless consumers are implemented; use [blank verification](blank-renderer.md) and [diagnostic motion](diagnostic-motion.md). A native consumer imports immutable frames on a separate same-adapter D3D11 device. C ABI, GPUI, Electron, WebView2 and embedded Chromium adapters remain planned.

## Consumer outcome

An application embeds the renderer through a small contract without acquiring a dependency on the example viewer. Each adapter displays frames and handles ownership, synchronization, resizing, visibility, and shutdown correctly.

## How to reach it

The examples consume the public library API. Each framework adapter needs its own minimal real host and verified route. Record those routes separately when implemented.

## Drive and observe

Load the scene, present its frames behind host content, and exercise the affected adapter's size, DPI, playback, visibility, and shutdown behavior. Keep host input working while animation runs. Verify texture synchronization and release with graphics diagnostics where available. Evaluate CPU frame copies or GPU transfers through measurements rather than assuming that a handle implies efficient sharing.

## Evidence and limits

Record the host/framework version, build, backend, actions, output, diagnostics, and adapter-specific measurements. A standalone viewer verifies its own integration only. A C-compatible interface is not browser support. Native graphics API compatibility does not establish an adapter without a working consumer.
