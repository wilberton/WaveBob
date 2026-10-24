# 28: Stream effects with safe updates and latency alignment

**What to build:** Extend the shared renderer to execute bounded streaming document/clip processing, with prepared graph handoff, smooth parameter updates, channel/timing policy, processor latency and offline/manual-output parity.

**Blocked by:** 27 — Model live processing state and reference contracts; 08 — Waveform playback, seeking and loop transport; 16 — Hear and inspect a multitrack mix.

**Status:** done

## Acceptance criteria

- [x] A WaveBob processor contract declares preparation, supported parameters, latency, output channels, tail/activity, reset and processing. Manual playback and offline rendering use the same algorithm path; render metadata distinguishes source body, latency and audible tail.

- [x] Callback processing allocates/deallocates no heap buffers, performs no file access or blocking locks, and retires graph resources off the callback. Preparation coalesces superseded work, enforces documented buffer/voice limits and keeps the prior valid audible graph on failure.

- [x] Continuous parameters/envelope gain ramp approximately 10 ms; bypass, reorder and topology replacements transition approximately 20 ms with bounded old/new resources. Per-effect processors may provide justified transition policies without changing fixed-setting reference results.

- [x] Compensate per-path latency in arrangements and dry/bypassed comparisons; prepare changed latency atomically while retaining transport position. Offline trimming removes computational delay, preserving intentional predelay/echo. Tests with known-delay processors verify alignment, crossfades and no double compensation.

- [x] Implement envelope evaluation before its owner stack: independent frame-based linear fades multiply, zero/one-frame rules match the spec, gain is finite validated dB or explicit mute, and source PCM remains unchanged.

- [x] Use explicit channel descriptors: promote mono to dual mono when processing requires it and preserve stereo output. Tempo-synced processors receive the shared visible Effect tempo; reset modulation deterministically. Free time is the default.

- [x] Pause retains state, explicit seeks/rewind reset with off-callback context preparation, loop wrap carries streaming state, and non-loop playback drains tails. Implement the -100 dBFS/100 ms activity threshold, 60-second tail cap and last-10-ms taper with capped-tail metadata, including feedback that never decays.

- [x] Expose requested/applied processing generations and readiness/errors. Deterministic manual-output tests cover preview/cancel, bypass/reorder/remove, reset/loop, invalid/nonfinite output, resource pressure and parity across buffer sizes/rates. Instrument callback allocation and test bounded handoff; actual-effect load/latency measurement is completed in 44.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

Use small deterministic probe processors in tests to prove latency and state handoff before production effects are available. No probe appears as a shipping effect. This ticket owns live and offline renderer foundations; 42 owns linked bounces and final tail/body arrangement geometry.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.



## Implementation checkpoint — 24 October 2026

Streaming source-rate chains, envelope playback, a shared asynchronous offline
render/query path, latency-aware dry comparison, deterministic tail termination,
coalescing preparation, generation/error queries and callback resource retirement
are implemented. A 256 MiB shared prepared-state budget complements the per-chain
64 MiB / one-second latency limits. See [renderer contract](../../../docs/processing-renderer.md).

Focused verification passes: 12 library tests, six public processing tests and
13 manual transport tests. Public rendered PCM and live envelope output agree
exactly at 44.1/48/96 kHz with 1/7/64-frame blocks. Callback instrumentation records
zero allocations/frees for preview gain, cancel and fade topology transitions.
Test-only probes verify latency compensation (including intentional delay), deterministic
reset/state continuity across loops, continuous-target state transfer, -100 dBFS
hold, silent paths, the 60-second cap/taper, nonfinite output and resource rejection.
One coalescing-worker test verifies that 100 superseded/unavailable preparations
leave the previous audible graph intact and recover on a valid update. All-target
Clippy passes with warnings denied. No production effect is enabled by these tests.

The ticket remains in-progress while final integration/verification is completed;
its acceptance boxes are intentionally not claimed from this checkpoint alone.

Additional boundary checks reject sample/project saves during a preview and reject
unimplemented rendered sample exports instead of writing dry PCM. Two signal probes
exercise bypass/reorder/removal and independently delayed sources mixed through an
arrangement crossfade. Pause/tail-resume tests retain the streaming position and
state; the explicit pause ramp does not consume source frames.

Still required before completion: integrate owner processing into the live
arrangement clip path (not only document playback and explicit clip renders),
verify the prepared mix/control update path and complete the remaining adapter
and readiness/error checks against that path. Production-effect measurement and
editors remain owned by 29–40/44; linked-source freshness and final body/tail
geometry are owned by 42. Keep 29 blocked until this foundation is complete.

Final checkpoint verification: `cargo fmt --all -- --check`, the complete
`cargo test --offline --no-default-features` suite (104 passed; existing opt-in
native/independent-decoder checks remain opt-in), and both GUI-enabled and GUI-free
all-target Clippy variants with `-D warnings` pass. The complete direct/headless/MCP
WAV/MP3 baseline workflow also passes. This is a verified implementation checkpoint,
not completion of the remaining live arrangement processing integration above.

## Completion verification — 24 October 2026

Completed the remaining live arrangement adapter. Prepared clip voices now use
source intervals and the shared streaming chain; clip previews/cancel create safe
audible graph replacements without history or source-PCM changes. The mix reader
also drives bounded inspection and explicit interval exports. Preparation is
cancelable, limited to two audio jobs and 128 clip voices, and keeps prior audible
state on failure. Callback handoffs retain compatible processor state or use the
specified 10/20 ms transitions, preserve the rational transport clock and retire
all resources to the control thread. Mix failures/latency/capped tails reach the
transport processing snapshot.

The complete GUI-free suite, including direct/headless/MCP WAV/MP3 workflow,
passes (`.tools/test28-mix-full.log`). Focused checks additionally pass all fourteen
transport tests (`.tools/test28-mix-alloc.log`) and twelve library tests
(`.tools/test28-mix-latency.log`). The new clip preview/cancel test measures zero
callback allocations and frees; nine mix cases include trimmed source intervals,
44.1/48/96 kHz sources, 1/7/64-frame callback blocks and exact live/read parity.
The crossfade probe now runs independently delayed 7/19-frame processors inside
the live mix stream and verifies one-time path compensation. Formatting and GUI
and GUI-free all-target Clippy with warnings denied pass. An overlapping Windows
build briefly encountered an executable lock during the workflow test; rerunning
after that process exited passed.

The preceding checkpoints and their outstanding notes describe historical states.
The foundation is complete; production constructors/editors and measured load
remain 29–40/44, while linked bounces/final tail geometry/export policy remain
42–43 as specified in the ticket scope.
