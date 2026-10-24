# 33: Implement Fast Distortion and its egui editor

**What to build:** Implement Fast Distortion with the reference waveshaping/table algorithm and recreate the compact controls and transfer graph in egui.

**Blocked by:** 32 — Implement Parametric EQ and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Match preamp, threshold, A/B curve type, mix and post-gain mapping and reference transfer/table behavior, including signed input and output levels.

- [ ] Keep expensive table preparation and publication outside the callback; define smooth gain/mix changes and bounded transitions between curve types without changing the fixed-setting algorithm.

- [ ] Provide source-organized knobs/switches, numeric/reset access and an accurate transfer graph in its separate window. Never imply new oversampling/antialiasing modes that are not implemented.

- [ ] Amplitude sweeps, nonlinear sine/multitone inputs, stereo/impulse and parameter extremes compare to the pinned reference. Document reference aliasing characteristics rather than silently replacing the algorithm.

- [ ] Complete the per-effect delivery contract, including supported live edits, instance/history isolation, manual/offline parity and inspected actual controls.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/fast_distortion.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/fast_dist.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

