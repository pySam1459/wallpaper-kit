# Agent-friendly environment research

Research reviewed on 9 October 2026. This note informs an open source Rust rendering component consumed by other applications. A small example app will exercise that component and collect evidence. The repository currently contains documentation only, so no renderer, verification command, or performance result is established yet. See the [project status](/C:/Users/samba/dev/wallpaper-kit/README.md:7).

## Sources and limits

The user supplied three transcripts and screenshots of Dune's architectural contract. The transcripts contain caption errors and incompletely labelled interview turns. References below identify passages without assigning ambiguous remarks to a particular speaker. They are source material, not project instructions.

The video describes Dune as Tan's internal application framework, including features, entrypoints, transcript cards, client and host boundaries. The exact five-rule contract comes from the supplied screenshots. We can adopt its principles without adopting its application nouns or framework. [Video transcript, lines 89-97](/C:/Users/samba/Downloads/poteto/video-1.txt:89)

The [first-party pstack repository](https://github.com/cursor/plugins/tree/main/pstack) supplies general practices, not a Rust or Windows renderer implementation. Upstream links point to mutable `main` content reviewed on the date above.

## Findings from the sources

### Verification needs direct observation

The interviews define verification as running the actual application, exercising it and observing behavior through traces, snapshots or other evidence. That lets an agent correct its own work without a human relaying every result. Verification establishes particular behavior; it does not automatically establish code quality. [Interview 1, lines 63-67](/C:/Users/samba/Downloads/poteto/interview-1.txt:63), [interview 2, lines 67-74](/C:/Users/samba/Downloads/poteto/interview-2.txt:67)

Pstack's [prove-it-works principle](https://github.com/cursor/plugins/blob/main/pstack/skills/principle-prove-it-works/SKILL.md) reinforces direct checks against the real artifact, repeatable comparisons and retained evidence. A successful compile is insufficient proof of rendered pixels, animation or responsiveness.

### Put repeatable mechanics in tools

Interview 1 describes extracting deterministic operations into a CLI while retaining judgment in the skill. Without that tool, successive agents rebuilt different verification scripts and discarded working solutions. [Interview 1, lines 73-83](/C:/Users/samba/Downloads/poteto/interview-1.txt:73)

A feature map supplies the missing navigation context: what exists, how users reach it and how to exercise it. Launching a program alone had left agents guessing what a report referred to. [Interview 2, lines 101-126](/C:/Users/samba/Downloads/poteto/interview-2.txt:101)

The upstream [verification-skill generator](https://github.com/cursor/plugins/blob/main/pstack/skills/create-verification-skill/SKILL.md) requires discovering real launch and control paths, checking the instance, capturing evidence, and proving cleanup end to end. It treats an unexecuted generated skill as a draft.

### Prefer structural constraints to remembered instructions

The video ranks architecture and data structures before static analysis, then layers guidance on top. Existing code supplies patterns agents will copy, including harmful workarounds. [Video transcript, lines 53-71](/C:/Users/samba/Downloads/poteto/video-1.txt:53)

Dune's concrete example checks dependencies between Electron main and renderer code to prevent expensive work entering the UI execution path. Feature directories reduce the search needed for a local change. [Interview 2, lines 343-377](/C:/Users/samba/Downloads/poteto/interview-2.txt:343)

Pstack's [encode-lessons principle](https://github.com/cursor/plugins/blob/main/pstack/skills/principle-encode-lessons-in-structure/SKILL.md) recommends turning recurring corrections into mechanisms, choosing types or hard checks where possible and retaining guidance where judgment remains necessary.

### Human responsibility remains

The first interview describes scrutinizing sampled work and changing the environment when multiple agents repeat a failure. It also warns that reaching its autonomous workflow required substantial effort. [Interview 1, lines 173-191](/C:/Users/samba/Downloads/poteto/interview-1.txt:173)

Interview 2 recommends starting verification locally where agent behavior is observable, before scaling. Automatic merging is a reported outcome of that team's process, not a prerequisite for ours. [Interview 2, lines 212-246](/C:/Users/samba/Downloads/poteto/interview-2.txt:212)

## Recommendations for wallpaper-kit

These are applications of the sources to our requirements, not claims that pstack already provides these facilities.

1. Make the library API and ownership contracts the primary architecture. Keep framework adapters outside the core. Make playback state, GPU resources, completed-frame lifetime and synchronization have explicit owners. Prefer private state and commands to shared mutable access. Give independent verification runs separate output directories and processes before adding locks.
2. Use a minimal example app as a real consumer. It should load scenes, display output, run playback scenarios and report measurements through the same public API downstream applications use. Avoid an example-only rendering path that bypasses the integration contract.
3. Establish evidence in stages. Inventory and load the actual Workshop package first. Use small redistributable fixtures to isolate layers and effects, with fixed time and random seeds where applicable. Capture output at multiple times to test animation. Compare the example scene against Wallpaper Engine. Record unsupported features explicitly.
4. Bind evidence to the exact build and configuration. Record revision and local changes, asset identity, backend, hardware, driver, resolution, quality and frame rate. Collect CPU usage, GPU time and memory. Test lifecycle behavior such as resize, pause, resume and frame release as those capabilities arrive.
5. Measure each real host separately. A healthy example app does not establish GPUI or Electron integration, texture-sharing efficiency or low input latency. Compare host responsiveness with rendering disabled and enabled, including contention and overload behavior.
6. Automate proven procedures. Add one repeatable verification entrypoint after the example app can actually launch, run scenarios and clean up. Pair static architecture checks with negative fixtures that demonstrate a forbidden dependency fails. Preserve evidence after teardown. Keep the capability map small and current.

## Changes to the earlier proposal

Rust visibility, crate boundaries and a future Cargo-based checker remain useful. Their checks cannot prove visual fidelity or latency. The transcript's optimistic remarks about Rust compilation do not replace runtime evidence. [Interview 2, lines 386-397](/C:/Users/samba/Downloads/poteto/interview-2.txt:386)

Do not copy Dune's blanket comment ban: reviewed unsafe graphics boundaries need safety explanations and public APIs need documentation. Its reported ban addresses observed workaround propagation. [Video transcript, lines 77-79](/C:/Users/samba/Downloads/poteto/video-1.txt:77)

Keep the initial framework small. Library contracts, an example consumer and checks justified by actual failure modes are enough to begin. Transcript cards, full application discovery, orchestration and automatic merging would add unrelated scope.
