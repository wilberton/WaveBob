# 39: Implement Stereo Shaper and its egui editor

**What to build:** Implement Stereo Shaper's reference main-output stereo matrix, delay and phase processing with its own native egui editor.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Match the four-coefficient input/output matrix and supported single-channel delay/all-pass phase plus pre/post placement, preserving source parameter order, units and ranges.

- [ ] Expose the main stereo output only. Extra difference-output controls/routing are omitted; no phantom output bus or silent signal loss is introduced.

- [ ] Intentional channel delay/phase behavior survives global latency compensation. Prepare buffers/topology off the callback and report correct remaining output extent; safe updates and reset follow the common contract.

- [ ] Recreate the matrix fader/control layout and exact numeric/reset access in egui, with clear channel labels and instance ownership.

- [ ] Reference basis-vector stereo impulses, swap/invert/mid-side-like matrices, sum/cancellation, phase and delay/placement cases satisfy the complete per-effect contract, including bounded peaks and an inspected editor capture.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/stereo_shaper.rs); follow its engine-algorithm references as documented in the investigation.
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

