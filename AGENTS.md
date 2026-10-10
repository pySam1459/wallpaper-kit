# AGENTS.md

## Project context

Read [README.md](README.md) before planning implementation or changing project scope. It records the requirements, MVP acceptance criteria, architecture candidates, and compatibility references.

The immediate target is Workshop scene `3780699842` rendering on Windows. Implement the features its package actually requires. Treat broader scene compatibility, photo and video support, and framework adapters as later milestones unless the task requests them.

Rust and a blank Direct3D 11 backend are implemented. Production scene rendering, Media Foundation, and framework adapters remain to be validated. Read [the blank renderer guide](docs/blank-renderer.md) for actual API and verification routes.

Keep the example app a thin consumer of the library's public API. Put rendering behavior in the library, and use the example app to observe output and collect metrics.

Before changing Rust code, public APIs, dependencies, or lint policies, read [the Rust rules](docs/rust-rules.md). They identify the enforced boundaries, unsafe exceptions, and verification requirements.

When designing agent tooling or verification, read [the source research](docs/research/agent-friendly-environment.md). Adapt its principles to this library's scope and verify commands against the real implementation before documenting them as working.

Before verifying renderer behavior, read [control-wallpaper-kit](.codex/skills/control-wallpaper-kit/SKILL.md) and the relevant entry in [the feature map](.codex/skills/wallpaper-kit-feature-map/SKILL.md). Update affected entries when capabilities or verification routes change. Blank and diagnostic motion verification are implemented; scene verification remains planned.

## Think before coding

- State assumptions explicitly before implementation.
- If interpretations change scope or behavior, present the alternatives and ask for clarification.
- If a simpler approach meets the requirements, explain it.
- When a requirement is unclear, name the uncertainty and resolve it before dependent work.

## Simplicity first

- Write the minimum code that solves the requested problem.
- Add abstractions when concrete callers or repeated behavior justify them.
- Keep speculative features, configuration, and impossible-case error handling out of the change.
- Review the result for complexity a senior engineer would remove.

## Surgical changes

- Touch only files and lines needed for the task.
- Match the surrounding code's style.
- Preserve unrelated edits, formatting, comments, and pre-existing dead code.
- Remove imports, variables, and functions made unused by your own changes.
- Mention unrelated issues separately rather than fixing them without scope.

## Goal-driven execution

Define observable completion criteria before changing code. For multi-step work, state a brief plan with a verification check for each step.

- Reproduce a bug before fixing it, with a regression test where practical.
- Test invalid inputs when adding validation.
- Check behavior before and after a refactor.
- For scene work, inspect the actual rendered output and animation against Wallpaper Engine. A successful build or preview image is insufficient evidence.
- For performance work, measure the real workload and host responsiveness with rendering disabled and enabled. Record hardware, resolution, quality, and frame-rate settings.
- Run checks appropriate to the change and report any unverified behavior or blockers.

The required local gate is `pwsh -File ./scripts/verify.ps1`. It checks format, Clippy, public-contract tests, Rustdoc, dependency boundaries, headless output, diagnostic motion and native presentation. Use `-Software -SkipNative` without an interactive desktop and report the native check as skipped. For motion changes, follow [the motion evidence procedure](docs/motion-verification.md). Report pixel correctness and live timing separately; controlled captures cannot establish smooth playback. Avoid claiming scene or framework compatibility from diagnostic output.

## Rendering and integration constraints

- Keep the core renderer independent of GPUI, Electron, WebView2, and Chromium.
- Make UI integration consume completed frames without waiting for scene loading, decoding, shader compilation, or background rendering.
- Bound frame queues and resource usage. Prefer dropping stale frames or reducing background quality over delaying the host UI.
- Keep GPU frames on the GPU where the host supports it. Establish texture ownership and synchronization before claiming efficient sharing.
- Pause unnecessary animation and avoid continuous work for static content.
- Validate native Windows behavior early. Extra threads or processes do not guarantee isolation from shared CPU, GPU, memory, or power limits.

## Source and asset handling

- Implement compatibility from public documentation, data formats, observable behavior, and references whose licenses fit the project.
- Do not copy decompiled proprietary code. Review licensing before copying or porting open source implementation code.
- Keep Wallpaper Engine assets and downloaded Workshop packages out of commits unless redistribution is permitted.
- Use local test content without modifying the Wallpaper Engine installation or Steam Workshop originals.
- Report unsupported features explicitly rather than silently claiming full scene compatibility.
