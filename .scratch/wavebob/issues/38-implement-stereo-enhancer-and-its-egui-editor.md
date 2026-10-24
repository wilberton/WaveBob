# 38: Implement Stereo Enhancer and its egui editor

**What to build:** Implement the reference Stereo Enhancer and recreate its stereo controls in a separate egui window.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Match separation, pan, phase offset, pre/post placement, volume and channel-inversion controls with their reference order/matrix and delayed-channel behavior.

- [ ] Distinguish intentional inter-channel phase/time offset from algorithmic latency; compensation must not remove the audible effect. Expose output channels and remaining delayed extent correctly.

- [ ] Prepare delay capacity/topology and smooth supported changes without callback allocation or uncontrolled phase discontinuities; reset/seek behavior is deterministic.

- [ ] Recreate the source organization and exact values/reset in egui. Verify mono-source promotion and explicit mono downmix, including phase cancellation outcomes without silently changing the matrix.

- [ ] Reference asymmetric impulses, correlated/antiphase stereo, phase offset extremes and placement/inversion combinations satisfy the complete per-effect delivery contract and inspected editor capture.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/stereo_enhancer.rs); follow its engine-algorithm references as documented in the investigation.
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

