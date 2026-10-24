# 35: Implement Chorus and its egui editor

**What to build:** Implement the reference Chorus's modulated delay/LFO and filtering algorithm with a faithful native egui editor.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Match delay/depth/stereo, the three LFO speed/waveform controls, crossover/filtering and dry+wet/wet-only parameter mappings and reference algorithm.

- [ ] Use fixed seeded initialization so repeated reset renders reproduce stereo phases. Define and verify pause, seek/reset and looping behavior through the shared processor contract.

- [ ] Prepare capacity/filter changes outside the callback and smoothly update supported modulation/gain parameters with bounded delay buffers; channel layout and remaining delay output are reported correctly.

- [ ] Recreate the source group layout, knobs/selectors and exact value/reset input in egui; hidden editor visuals do not affect processing.

- [ ] Reference impulses, sustained tones/stereo signals, waveform/speed/depth extremes, reset repeats and wet-only operation satisfy the complete per-effect delivery contract.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/chorus.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/chorus.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

