# 37: Implement Balance and its egui editor

**What to build:** Include Balance as a built-in utility with the reference folding stereo balance matrix and volume, plus its compact egui editor.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Match the reference matrix: attenuated-side input folds into the favored side as specified. Keep this distinct from WaveBob's track stereo balance/mono pan law.

- [ ] Expose balance and volume with correct source mapping/defaults/units, exact entry/reset and clear zero/center behavior. Balance volume is independent of the waveform/clip envelope gain.

- [ ] Use smooth bounded matrix/gain changes with zero algorithmic latency/tail where applicable, preserving correct mono-promotion and stereo output semantics.

- [ ] Reference left-only/right-only impulses, correlated/antiphase stereo, centered/extreme balance and volume extremes verify channel routing and gain rather than only summed levels.

- [ ] Complete the per-effect contract and native editor capture. Balance is required even though other controls can perform some similar operations.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/balance.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/balance.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

