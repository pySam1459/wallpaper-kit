# Rust rules

These rules apply to changes in Rust code, exported APIs, dependencies, and lint policies. [AGENTS.md](../AGENTS.md) supplies the general workflow. [The blank renderer guide](blank-renderer.md) describes the implemented ownership and consumer contracts.

## Enforced checks

Run the required gate from the checkout containing the change:

```powershell
pwsh -File ./scripts/verify.ps1
```

The same entrypoint runs in Windows CI with `-Software -SkipNative`. That configuration checks WARP/headless behavior and records native verification as skipped. Local native verification requires an interactive Windows desktop. Hardware and software evidence establish different configurations.

[Cargo.toml](../Cargo.toml) defines the lint policy. [The verification script](../scripts/verify.ps1) runs formatting, Clippy with warnings treated as errors, tests, Rustdoc, dependency checks, and actual consumers. Rustdoc checks public documentation links. Clippy checks safety documentation and comments around unsafe operations. These checks establish specific properties, not a general proof of graphics correctness.

Handle `Result` and other must-use values. Propagate recoverable failures or handle them at the appropriate boundary. An explicit `let _ =` is appropriate when discarding a result is intentional, such as a coalesced wakeup that a full notification channel already represents. Use it with a reason tied to the operation, rather than to silence an error.

## Unsafe graphics boundaries

Unsafe Rust is denied by default. The two existing module-level exceptions are [the GPU backend](../src/gpu.rs) and [native presentation](../examples/native-viewer/presentation.rs). They contain Windows graphics operations requiring unsafe calls.

Keep unsafe operations in those boundaries where the responsibility belongs there. Each unsafe block or implementation needs a nearby safety explanation. Each public unsafe function needs a `# Safety` contract. An unsafe function still needs explicit unsafe blocks for unsafe operations.

Adding an exception or suppressing a lint changes enforcement. Keep the exception local and explain the invariant, why the checked path cannot express it, and how the change is verified. Review graphics lifetime and synchronization assumptions even when the lints pass. Do not widen an exception to the whole crate to make a check pass.

## Dependency and API boundaries

Keep the renderer's implementation private except for items concrete consumers need. Preserve checked construction for values such as `Size`. Public documentation must explain behavior the signature cannot show, including errors, blocking, lifetime, and safety conditions.

Examples use the public API. Host-window, capture-encoding, and report tooling remain example dependencies. [The boundary checker](../scripts/check-boundaries.ps1) follows production and build dependencies and rejects the listed host packages, including transitive paths. Its self-test promotes the real Winit example dependency to a production dependency and requires rejection. Each verification run retains that result in `boundaries.json`.

When dependencies or responsibilities change, update the boundary policy only when the architecture requires it. Adding a new framework integration belongs in its consumer or adapter. New modules should have clear owners; additional crates or generic backend traits need a concrete reason.

## Ownership and verification

Preserve the documented worker, frame, and consumer ownership contracts. Bound pending work, keep UI frame acquisition independent of GPU waits, and define shutdown behavior when adding background work. Explicit blocking diagnostic capture remains separate from normal presentation.

Test observable behavior through the public consumer path. Add focused parsing or state tests when they cover cases a consumer test cannot express clearly. Check the existing example callers when changing an exported API. Keep test failures, unavailable checks, and unsupported capabilities visible in the report.

Use the controller and relevant feature-map entry named in `AGENTS.md`. Retain the verification manifest, boundary result, consumer reports, and captures. Blank output proves a limited native/headless contract; [diagnostic motion](motion-verification.md) adds deterministic pixels and separate live telemetry. Scene fidelity, continuous-animation budgets, device recovery, framework adapters, and host latency need their own evidence as they are implemented.
