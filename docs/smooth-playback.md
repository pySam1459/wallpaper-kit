# Playback cadence

The 12 October 2026 change removes three playback limits: T3's fixed 33 ms
delivery timer, coarse native adapter channel waits, and forced 30 fps video
decoding. T3 requests completed textures from `requestAnimationFrame`, with one
transfer in flight. Its native worker samples against absolute deadlines at the
initial display frequency, capped at 240 fps, with a 60 fps fallback. Missed slots
are skipped. The native viewer uses the initial monitor refresh rate with the
same cap and fallback. Changing monitors during playback does not retarget either
consumer yet.

Active adapter polling uses short sleeps under a paired Windows timer-resolution
request. Hidden, stopped and completed still backgrounds release the request;
idle or lease-saturated workers block for a wakeup. Three retained GPU exports
still bound the adapter pool. The narrowly scoped unsafe exception in the local
adapter's `pacing.rs` owns one successful `timeBeginPeriod(1)` request and balances
it with `timeEndPeriod(1)` on drop. The core has no new host dependency.

Video indexing, seeking and FFmpeg output use the source's rational average frame
rate, including rates above 30 fps, up to 240 fps. Variable-rate media is normalized
to that average cadence. Low-rate videos and sprite sequences retain their
authored frames; intermediate frames are not synthesized. Procedural scene
effects sample the common continuous playback clock at the consumer's rate.

## Evidence

The local Panda Store workload was 1920×1080 with linear image sampling, animation
and VFX enabled, on an NVIDIA RTX 5080, driver 32.0.16.1664. The desktop display
was 2560×1440 at approximately 360 Hz. Native exports were warmed for a second,
then observed for four seconds, with immediate lease release and no pixel
readback. These counts measure completed exports, not display presentation.

| Observation | Export fps | p95 gap | Maximum gap |
| --- | ---: | ---: | ---: |
| Prior adapter, simulated old T3 33 ms polling | 21.43 | 47.90 ms | 48.39 ms |
| Final adapter, 60 fps request, 1 ms Node polling | 59.41 | 31.25 ms | 32.77 ms |
| Final adapter, 120 fps request, 1 ms Node polling | 119.99 | 10.25 ms | 11.54 ms |

Raw observations are in `artifacts/smooth-playback/panda-old-impact.json`,
`panda-steady-impact.json` and `panda-120-sleep.json`. The 60 fps polling run still
shows Node timer variability; a separate run reached 60.00 fps with an 18.90 ms
p95 gap. It is not evidence of smooth monitor presentation. The dev-only
`.local/measure-playback.cjs` reproduces local exports using `WALLPAPER_KIT_ADDON`,
`WALLPAPER_KIT_SCENE`, `WALLPAPER_KIT_ENGINE_ASSETS`, `WK_FPS`, `WK_POLL_MS` and
`WK_RECEIPT`. It remains ignored.

The warmed native export process used 0.005 CPU cores disabled and 0.012 enabled
at 60 fps, with RSS approximately 135 MiB and 183 MiB respectively. Startup was
excluded. Polling frequency differs from the old simulated host and these small
samples are diagnostics, not resource budgets. Hardware configuration is retained
in `artifacts/smooth-playback/hardware.json`.

The software gate `artifacts/20261012-160700-19876de9/manifest.json` passed format,
Clippy, public contracts, Rustdoc, dependency boundaries and headless GPU checks;
native presentation was explicitly skipped there. Its generated live 60 fps
video test detects the old 30 fps selection limit. Existing 30 fps seek, loop,
retained-frame and embedded-video tests also pass.

Separate native Panda and Japanese Summer Tram controllers passed 1080p captures,
exact rewind, paused idle and owned-process cleanup. They recorded 1775 and 1423
presentation-call observations over eight seconds respectively. Those counts
include consumer presentation calls and are not unique video frames or display
refresh measurements. T3 addon checks passed three retained leases, release/resume,
hidden pause, still/remount and invalid frame-rate rejection for both scenes.
The collection metric remains 65/65 (100%) in
`artifacts/workshop-support-smooth/report.json`; named fidelity omissions remain
reported separately.

The rebuilt actual T3 window showed changing Panda windmills and accepted an
unsent draft. Two T3 captures and two Wallpaper Engine reference captures are
retained in `artifacts/smooth-playback`. This verifies visible motion and the
host path, with differing window geometry and scene phase. It does not establish
exact engine fidelity, monitor frame timing or measured input latency. The
separate Codex WebRTC demo still uses its existing 30 fps transport.

Ten-second samples of the same owned T3 process tree, including its GPU and
backend processes, used 0.006 CPU cores with wallpaper disabled and 0.724 enabled.
Working sets were 0.94 GiB disabled and 0.97 GiB enabled. This actual host used
2560×1440 wallpaper output with the 240 fps render cap on the approximately
360 Hz desktop. The background disappeared when disabled and the unsent draft
survived. Raw values are in `t3-disabled-impact.json` and `t3-enabled-impact.json`.
These are idle samples; they exclude measured input/scrolling latency and GPU
duration. Higher frame rates increase host work, so these values do not establish
an acceptance budget. Only the temporary T3 process and named engine reference
window were closed after verification.

Close and relaunch T3 through `integrations/launch-t3.ps1` to load the rebuilt
native addon and desktop bundle.
