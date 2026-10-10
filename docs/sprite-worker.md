# Background sprite preparation

`SpriteWorker` runs package opening, hashing, TEX indexing, page decoding and [frame extraction](sprite-extraction.md) on a dedicated asset thread. The consuming app submits times and takes completed samples. This stage-4 component has no timer or prefetch; it does not implement continuous scene playback.

## Use from a consumer

Create `SpriteWorker::new(source, asset)` with an owned `SceneSource` and asset string. It starts an idle thread without opening files on the caller. The returned capacity-one notification receiver contains coalesced wakeups. Treat a wakeup as a reason to inspect the result; it is not a sample count. Dropping or leaving the receiver undrained does not block publication.

`try_request_at(time)` uses nonblocking metadata access. `Some(id)` accepts the request; `None` means busy metadata and accepts nothing. Retry on another host tick. Accepted requests replace pending work and clear unconsumed prior results. One active preparation may finish, but a newer request identity prevents its superseded sample or error from being published.

`try_take()` also uses nonblocking metadata access. It returns a completed `SpriteResult` once, or `None` while pending, idle, already consumed or temporarily busy. The result supplies `request_id()`, `time()`, `stats()` and `into_sample()`. Asset failures carry the same request identity as successful samples. Opening failures can retry on another explicit request; a successfully opened stream stays bound to its read-locked asset until shutdown. Worker panic/metadata failure is reported rather than leaving callers polling forever.

Use the returned sample's region Arc with the existing region draw or `ImageLayer` API. Keep the previous completed GPU frame when no new sample is ready. Do not put diagnostic waits or readback in a host UI update. The example helper's bounded waiting is for verification and startup tools.

## Ownership and cancellation

Each worker owns one stream and admits one active preparation, one latest pending request and one ready result. There is no unbounded work or frame queue. Repeated selection of the stream's current frame shares its region Arc, avoiding extraction and GPU upload when reused by the same renderer. The stream retains one decoded atlas up to 256 MiB and one extracted frame up to 64 MiB. Its ready sample shares that frame payload; separate workers, caller-held samples, codec temporaries and GPU resources remain outside this per-worker bound. This is not a complete scene memory budget or a globally shared decoder.

Stop submitting times to pause: outstanding work can settle, then the worker sleeps without periodic work. Explicit seeks invalidate previous results. The worker is tied to one asset; async scene replacement and multi-asset scheduling remain open.

`shutdown()` cancels pending work, prevents later publication and joins the worker. Active hashing, file reads or codec calls cannot be interrupted and may finish first. Dropping also joins. Perform either operation outside latency-critical UI work. Returned samples remain valid, and joined shutdown releases asset handles. No worker is detached. Explicit shutdown reports a worker panic; Drop joins without propagating it.

## Verification

```powershell
pwsh -File ./scripts/verify.ps1
pwsh -File ./scripts/verify.ps1 -Software -SkipNative
cargo run --locked --example native-viewer -- --extracted-fixture
```

Deterministic scheduler tests hold one preparation at a barrier while 1,000 time requests complete. Only the active request and final pending request run; the superseded error cannot be taken. Tests also establish idle behavior, cancellation before pending work starts, waiting for active work on shutdown and panic reporting. A public consumer test delivers a missing-asset error asynchronously, retries after creating generated content, verifies actual WARP pixels, same-Arc reuse, an undrained capacity-one notification channel, retained samples and file-handle release after shutdown.

`texture-checks` prepares and draws all eight generated storage/format cases through the worker. It checks independent expected pixels and repeated-source upload reuse. With the paired target roots from [TEX verification](texture-pages.md), it prepares bird frame 18 and character frame 16 at their first page boundaries and requires exact GPU equality with whole-atlas crop captures. An independent diagnostic GPU request is serviced between requesting and taking each prepared sample. This route does not measure an application's UI responsiveness.

Inspect `textures/report.json` fields `generated_worker` and `target_worker`, plus `worker-*.png` and `target-worker-*.png`. Preparation reports include polls, maximum result-poll call duration, stream counters and payloads. Wait time starts after the independent GPU request; it is not the complete request latency. Diagnostic measurements include readback elsewhere in the run and are not GPU duration, display cadence or host input latency.

The native extracted fixture now uses the public worker, records `native-extracted/sprite-preparation.json`, joins preparation before window creation and presents the retained sample. Its capture, resize, idle, diagnostic motion and owned-process cleanup checks remain unchanged. This proves the prepared-sample consumer path; interactive loading and live scene playback remain unverified.

The [hardware/native receipt](../artifacts/20261010-151159-dd476da1/manifest.json) and [WARP/headless receipt](../artifacts/20261010-151340-13047ca2/manifest.json) passed on 10 October 2026, Windows x64 and Rust 1.91. All 48 tests, formatting, Clippy, Rustdoc, dependency and consumer checks passed. Hardware used an NVIDIA GeForce RTX 5080, including the two actual worker-prepared target captures and native extracted capture/resize/owned exit. WARP skipped native presentation and proprietary assets. Captures were inspected; ignored evidence must be regenerated in other checkouts.

In the recorded hardware run, preparation waiting took about 5.26 seconds for birds and 4.53 seconds for the character, including background package opening/hashing. Maximum result-poll calls measured 0.0324 ms and 0.0017 ms. The GPU source payloads remained 13,824,000 and 3,462,912 bytes. These debug single-run observations establish a prompt polling path during slow preparation, with the measurement limits above. Prefetch, sustained two-loop playback, aggregate residency and host-impact acceptance remain open.
