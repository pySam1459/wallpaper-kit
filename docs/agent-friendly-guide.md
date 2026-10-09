# Agent-friendly engineering guide

This guide compiles Lauren "Poteto" Tan's ideas about agent-friendly architecture, verification tools, skills, and evals from the supplied interviews, video, screenshots, and pstack references. It adapts those ideas to scene-renderer, an open source Rust rendering component with a small Windows example app.

The central idea is to make good work the conventional path, give agents tools to observe the result, and improve the environment when failures repeat. Trust comes from evidence about specific behavior and engineering constraints.

This is a synthesis, not a verbatim transcript or a replacement for [AGENTS.md](../AGENTS.md). The [source research](research/agent-friendly-environment.md) records attribution and limitations. Rust mechanisms and renderer scenarios below are project adaptations. They do not imply that Tan prescribes this exact implementation.

## Architecture carries the strongest guidance

Agents often work with a narrow prompt and a few nearby files. They copy existing patterns and favor changes that satisfy their immediate task. A workaround in the codebase can become the pattern later agents repeat.

Tan describes several layers of guidance, with architecture and data structures providing stronger constraints than prose. Compiler diagnostics, static analysis, and CI enforce additional rules. Skills and instructions supply context and judgment. Human review remains responsible for questions those mechanisms cannot settle. [Video transcript, lines 53-71](/C:/Users/samba/Downloads/poteto/video-1.txt:53)

For this project, that means choosing structures that make ownership and dependencies clear before accumulating instructions about them. A private resource owner is stronger than a comment asking callers not to mutate shared state. A failing dependency check is stronger than asking every reviewer to remember forbidden imports.

### Dune's five-rule contract

The supplied Dune screenshots state the following rules. The user clarified the fifth rule's obscured text.

1. The conventional path requires fewer decisions than a shortcut.
2. Forbidden dependencies fail mechanically.
3. Every durable value has one obvious writer.
4. New product work adds isolated files rather than branches in shared roots.
5. Exceptions are narrow, explicit, and reviewed as architecture changes.

Dune's application concepts include Feature, Entrypoints, Transcript cards, Client, and Host. They illustrate the contract within Tan's framework. This renderer needs boundaries appropriate to a library, its backends, and its consumers.

### Applying the contract in Rust

These are enforcement directions to introduce as the implementation develops:

- Keep the core independent of GPUI, Electron, WebView2, and Chromium. Adapters consume the renderer's public API. The example app follows that same path.
- Use module privacy and restricted visibility to contain implementation details. Add crate boundaries when real dependency or ownership constraints justify them.
- Use typed states and commands to express valid lifecycle transitions. Parse external scene data at the boundary so internal code can rely on validated values.
- Give mutable playback state, asset caches, and GPU resources explicit owners. Define who may write state and how other actors request changes.
- Make frame lifetime, release, and synchronization part of the integration contract. Rust ownership alone cannot prove correct synchronization across graphics APIs or foreign code.
- Put new scene capabilities in modules with clear responsibilities. Avoid growing one central function with unrelated feature branches.
- Add targeted dependency checks when Cargo and visibility rules cannot enforce a required boundary. Test the checker with a forbidden dependency that must fail.
- Record architectural exceptions with their scope, reason, affected invariant, and verification. Review the concrete change without turning routine implementation choices into approval ceremonies.

The aim is fewer ambiguous decisions. It does not require a crate per feature, automatic registration, or a general application framework.

## Verification gives agents observation and control

Tan's verification skills let an agent run the application, exercise behavior, and inspect output through snapshots, traces, and metrics. Without direct observation, a human must relay results before the agent can iterate. [Interview 1, lines 63-67](/C:/Users/samba/Downloads/poteto/interview-1.txt:63)

For scene-renderer, verification has to reach the rendered frames and the consuming application's behavior. Compilation establishes useful properties, but cannot establish correct pixels, animation, or host responsiveness.

The small example app is a real consumer of the library. It displays output and provides the controls needed to exercise loading, playback, resize, and shutdown. Scene interpretation and rendering remain in the library. A special rendering path available only to the example app would weaken the evidence about downstream integration.

### Keep mechanics in reusable tools

Tan separates deterministic operations into a CLI and leaves judgment in the skill. Her interviews describe agents repeatedly rebuilding and discarding verification scripts before those mechanics became reusable tools. [Interview 1, lines 73-83](/C:/Users/samba/Downloads/poteto/interview-1.txt:73)

Our controller should eventually provide repeatable launch, inspect, load, drive, capture, measure, and stop operations. Its skill explains when to use those operations and how to judge their output. A procedure becomes a verified capability only after a complete run, including cleanup.

The control-glass screenshots provide useful operational practices to adapt:

- Inspect readiness before driving an instance. Report build freshness, runtime identity, and missing prerequisites through a doctor operation.
- Identify the intended checkout and process explicitly. Give separate runs their own state, endpoints, and evidence directories.
- Refuse ambiguous attachment to another running instance. Worktree verification must not silently target the user's main session.
- Prefer stable operations and observable states over coordinates and guessed delays. Use bounded waits with a defined failure result.
- Exercise the normal consumer path. Avoid controller shortcuts that directly mutate private renderer state.
- Keep unrelated settings unchanged. Record diagnostic overlays, captures, and other instrumentation that can affect measurements.
- Stop only processes and resources owned by the run. Preserve evidence after cleanup.

Playwright, DOM selectors, and CDP fit Glass's web interface. Our native viewer needs suitable control operations. There is no requirement to adopt a browser solely to reproduce those tools.

## A feature map makes behavior discoverable

The Glass feature map describes features from the user's perspective, including how to reach and exercise them. It supplies navigation context a launch command alone cannot provide. [Interview 2, lines 101-126](/C:/Users/samba/Downloads/poteto/interview-2.txt:101)

For a rendering library, the corresponding perspective is the consumer's. Each entry needs the expected outcome, prerequisites, actual API or viewer route, actions, observable results, evidence, and limits. Source code and architecture documents explain implementation details.

Entries have explicit statuses: planned, implemented but unverified, or verified against a recorded build and configuration. A successful run on one revision does not establish every future revision. Changes to behavior or control routes require updates to the affected entry.

Read the entries relevant to the task rather than loading every feature document into context. Keep unsupported paths and unavailable prerequisites visible. A skipped check must not appear as a pass.

The existing [feature-map skill](../.codex/skills/scene-renderer-feature-map/SKILL.md) covers the Workshop MVP, playback and lifecycle, performance and host impact, and library and adapter integration. Additional entries should follow concrete implementations.

## Evidence supports a repeatable improvement loop

The useful loop is a change followed by an observation that can guide the next change. A practical verification sequence is:

1. Define the consumer outcome and the observations that establish it.
2. Establish the baseline or reproduce the failure where practical.
3. Make the smallest change that addresses the outcome.
4. Run relevant structural, behavioral, visual, and performance checks.
5. Compare results against the baseline and inspect failures.
6. Correct the change and repeat the affected checks until the outcome holds or a concrete blocker remains.
7. Retain evidence and report what passed, failed, or remains unverified.

Each run needs the exact build and local changes, scene identity, backend, hardware, driver, resolution, quality, and playback settings. Record the action sequence, logs, capture timestamps, metric definitions, and measurement limits.

For the Workshop MVP, evidence must come from the actual scene package and include its visible layers, effects, and animation. Compare against Wallpaper Engine. A preview image or one still frame cannot establish scene playback. Use independent references rather than generating expected output from the renderer under test.

Small redistributable fixtures help isolate behavior. Fixed time and random seeds improve repeatability where the implementation supports them. Real-time playback and performance still need separate measurements, including timing variability and nondeterminism.

Measure host responsiveness with rendering disabled and enabled under matching conditions. The example app's frame rate does not establish GPUI or Electron input latency. Threads and processes share hardware, so separation alone does not prove negligible impact.

## Evals test the environment as well as the product

Tan describes evals as unit tests for agents. A rubric states the intended outcome, and isolated task runs test whether a skill produces it. Her examples include multiple models and separate judging to reduce evaluator bias. She recommends observing local behavior before scaling automation. [Interview 2, lines 159-211](/C:/Users/samba/Downloads/poteto/interview-2.txt:159)

For our project, evaluations cover three different subjects:

- Renderer evaluations check output, animation, lifecycle behavior, resource use, and compatibility on defined workloads.
- Example-app evaluations check whether a real consumer can present frames, expose controls, collect reliable evidence, and clean up.
- Agent-environment evaluations give agents realistic tasks and inspect whether they find the relevant feature, change the right code, verify behavior, and report limitations accurately.

An agent eval needs a known repository state, an isolated workspace, a task, a rubric, and retained artifacts. Completion claims are evidence to inspect, not the grade. Use independent checks where possible and record model, skill version, configuration, elapsed time, and cost when available.

Early scenarios can test truthful reporting when the runtime is missing, handling an invalid scene path, preserving another instance, or recognizing that a still image does not prove animation. Later scenarios can include a known rendering defect, a performance regression, and a changed adapter lifecycle contract.

### Add evaluations as behavior becomes runnable

Our agreed sequence starts with a minimal library and an example app embedding a blank renderer. That establishes startup, presentation, idle resource use, and cleanup baselines. It cannot establish scene quality or realistic scene performance.

Image and layer rendering then allow output comparisons. Animation adds temporal checks. Host adapters add input, visibility, synchronization, and lifecycle scenarios inside each actual framework. Agent tasks become more representative as these capabilities arrive.

This sequence is our project adaptation. It avoids building a large eval system around features that do not exist while keeping measurement needs visible from the beginning.

### Improve metrics without weakening the outcome

Hill climbing means repeatedly changing an implementation or skill and measuring whether the defined result improves. Correctness and visual quality remain acceptance gates. Performance gains must retain the agreed effects, workload, resolution, and quality unless a quality change is the explicit experiment.

Keep visual fidelity, host latency, CPU usage, GPU timing, memory, and agent task success visible as separate results. One aggregate score can conceal a regression. Compare matching configurations, repeat noisy measurements, and separate capture overhead from normal playback.

Eval results identify areas to investigate. Passing a finite scenario set does not prove all scene compatibility or replace architectural review.

## Repeated corrections belong in the environment

Tan recommends observing agent work and turning repeated mistakes into changes to architecture or enforcement. The upstream [encode-lessons principle](https://github.com/cursor/plugins/blob/main/pstack/skills/principle-encode-lessons-in-structure/SKILL.md) describes that practice.

When failures repeat, first ask whether a clearer type, ownership boundary, or API can eliminate the mistake. Otherwise, add a targeted check or runtime guard. Keep instructions for decisions that still require judgment. Verify that the new mechanism catches the original failure and permits the intended path.

Existing bad patterns deserve deliberate removal because agents copy them. Keep that work scoped and reviewable. A newly observed problem does not justify unrelated cleanup in every task. Small, coherent changes also make regression investigation and reversion easier. [Interview 2, lines 315-320](/C:/Users/samba/Downloads/poteto/interview-2.txt:315)

Human responsibility remains. Review uncertain visual results, architectural exceptions, and the adequacy of the verification itself. Expand automation after local procedures earn trust through actual use.

## Adapt the principles to this library

Dune's full application discovery, transcript cards, authentication workflows, and automatic merging are outside the current renderer scope. Its blanket comment ban is also unsuitable here. Public Rust APIs need documentation, and unsafe graphics boundaries need explanations of their safety requirements.

The transferable principles are conventional ownership, enforced boundaries, reusable control tools, discoverable consumer behavior, direct evidence, and measured improvement. Add machinery when a real capability or failure justifies it.

## Current foundations and source material

The initial library now renders blank GPU frames, with native and headless consumers. The [control skill](../.codex/skills/control-scene-renderer/SKILL.md) provides a read-only doctor and repeatable blank runtime verification. The [blank renderer guide](blank-renderer.md) records real commands and limitations. Scene loading, animation, full performance collection and agent eval execution remain planned. Doctor checks evidence freshness and keeps scene verification blocked.

Use the [control contract](../.codex/skills/control-scene-renderer/references/control-contract.md) for planned operations and evidence requirements. Use the [renderer specification](spec/scene-renderer.md) for implementation scope and delivery gates.

The compilation draws on these materials:

- [Interview 1 transcript](/C:/Users/samba/Downloads/poteto/interview-1.txt), especially verification tools, deterministic mechanics, and responsibility for the environment.
- [Interview 2 transcript](/C:/Users/samba/Downloads/poteto/interview-2.txt), especially feature discovery, evals, local iteration, and enforced architecture.
- [Video transcript](/C:/Users/samba/Downloads/poteto/video-1.txt), especially codebase patterns, structural constraints, and recurring failures.
- User-supplied Dune contract, control-glass, and Glass feature-map screenshots, including the clarified fifth contract rule.
- [First-party pstack repository](https://github.com/cursor/plugins/tree/main/pstack) and the specific references recorded in the [research note](research/agent-friendly-environment.md).

The transcripts contain caption errors and ambiguous speaker turns. This guide summarizes their themes without attributing every interview remark to Tan. Screenshot-specific practices and project adaptations are identified in the relevant sections. Upstream links refer to mutable source content reviewed during this project's planning.
