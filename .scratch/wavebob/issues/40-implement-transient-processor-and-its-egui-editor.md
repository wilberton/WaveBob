# 40: Implement Transient Processor and its egui editor

**What to build:** Implement the reference Transient Processor's detector/envelope and gain-shaping algorithm with its compact egui control window.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Match attack/release amounts and shapes, split-frequency/balance, drive/gain and processing toggle, including the source detection/filter/transfer order and parameter mapping.

- [ ] Prepare crossover/filter changes off the callback, keep detector history stable for smooth parameter edits where applicable, and specify state reset/seek/loop behavior.

- [ ] Expose exact source-organized controls, numeric/reset entry and visible processing state in a native egui window; toggling processing does not silently delete the effect.

- [ ] Reference attack/decay envelopes, isolated transients, split-band tones, sustained stereo/silence and amount/drive extremes verify detector timing and gain response against the reference.

- [ ] Complete the per-effect delivery contract, including fixed-setting manual/offline parity, live transition safety, isolation/history and inspected editor capture.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/transient_processor.rs); follow its engine-algorithm references as documented in the investigation.
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

