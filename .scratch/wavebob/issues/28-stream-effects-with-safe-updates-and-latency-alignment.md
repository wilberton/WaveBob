# 28: Stream effects with safe updates and latency alignment

**What to build:** Extend the shared renderer to execute bounded streaming document/clip processing, with prepared graph handoff, smooth parameter updates, channel/timing policy, processor latency and offline/manual-output parity.

**Blocked by:** 27 — Model live processing state and reference contracts; 08 — Waveform playback, seeking and loop transport; 16 — Hear and inspect a multitrack mix.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] A WaveBob processor contract declares preparation, supported parameters, latency, output channels, tail/activity, reset and processing. Manual playback and offline rendering use the same algorithm path; render metadata distinguishes source body, latency and audible tail.

- [ ] Callback processing allocates/deallocates no heap buffers, performs no file access or blocking locks, and retires graph resources off the callback. Preparation coalesces superseded work, enforces documented buffer/voice limits and keeps the prior valid audible graph on failure.

- [ ] Continuous parameters/envelope gain ramp approximately 10 ms; bypass, reorder and topology replacements transition approximately 20 ms with bounded old/new resources. Per-effect processors may provide justified transition policies without changing fixed-setting reference results.

- [ ] Compensate per-path latency in arrangements and dry/bypassed comparisons; prepare changed latency atomically while retaining transport position. Offline trimming removes computational delay, preserving intentional predelay/echo. Tests with known-delay processors verify alignment, crossfades and no double compensation.

- [ ] Implement envelope evaluation before its owner stack: independent frame-based linear fades multiply, zero/one-frame rules match the spec, gain is finite validated dB or explicit mute, and source PCM remains unchanged.

- [ ] Use explicit channel descriptors: promote mono to dual mono when processing requires it and preserve stereo output. Tempo-synced processors receive the shared visible Effect tempo; reset modulation deterministically. Free time is the default.

- [ ] Pause retains state, explicit seeks/rewind reset with off-callback context preparation, loop wrap carries streaming state, and non-loop playback drains tails. Implement the -100 dBFS/100 ms activity threshold, 60-second tail cap and last-10-ms taper with capped-tail metadata, including feedback that never decays.

- [ ] Expose requested/applied processing generations and readiness/errors. Deterministic manual-output tests cover preview/cancel, bypass/reorder/remove, reset/loop, invalid/nonfinite output, resource pressure and parity across buffer sizes/rates. Instrument callback allocation and test bounded handoff; actual-effect load/latency measurement is completed in 44.

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

