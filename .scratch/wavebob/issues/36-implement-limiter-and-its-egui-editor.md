# 36: Implement Limiter and its egui editor

**What to build:** Implement Limiter's reference limiter/compressor/gate processing and native egui graph/meter editor using an internal detector.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Reproduce supported limiting, compression and gate modes, timing/envelope and lookahead behavior, gains/ceiling/curves/saturation and parameter mapping from the reference Dynamics path.

- [ ] Expose internal detection only: external sidechain selector/input/routing is omitted. Implement all supported internal controls rather than reducing Limiter to a simple hard clipper.

- [ ] Preallocate/prepare lookahead, RMS and smoother state off the callback. Report changing algorithmic latency and update the chain's compensation/dry alignment atomically without changing clip timing.

- [ ] Recreate mode controls and level/gain-reduction graphs in its own egui window; publish bounded meter history and avoid expensive hidden-window analysis.

- [ ] Reference steps/transients/level sweeps, stereo linking, gate/ratio/timing extremes, ceiling/overshoot, silence and lookahead changes satisfy the complete per-effect contract, including no extra delay in bounces.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-limiter/src/view.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/dynamics.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

