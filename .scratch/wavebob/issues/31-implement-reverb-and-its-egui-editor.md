# 31: Implement Reverb and its egui editor

**What to build:** Implement WaveBob's reference tank/plate-style Reverb and faithfully recreate its room visual and INPUT/ROOM/DECAY/MIX egui control groups.

**Blocked by:** 30 — Implement Soft Clipper and its egui editor.

**Status:** in-progress

## Acceptance criteria

- [ ] Reproduce the reference network and supported parameter mappings: input M/S and cuts, predelay/free-or-sync, size/diffusion/modulation speed/depth, bass/decay/crossover/damping, dry/early/wet levels and stereo separation. Use source ranges/defaults rather than the old three-parameter reverb.

- [ ] Prepare delay-line/room-size changes off the callback and transition without repeatedly clearing audible state. Preserve fixed-setting reference behavior; document live-transition differences.

- [ ] Render deterministic natural tails through source end, seek/reset and seeded modulation; comply with threshold/hold/cap metadata and stereo channel policy. Tempo switch uses the explicit project BPM.

- [ ] Recreate room visualization and grouped knobs/switches/faders, source captions/units, precise values/reset and per-window help; Tempo is a toggle rather than the illustrative mockup dial.

- [ ] Reference impulse, filtered/M-S stereo, modulation, extreme decay and room-size cases, plus live room changes, dry/wet/early combinations and tail termination, satisfy the per-effect delivery contract.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-reverb/src/view.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/reverb.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

