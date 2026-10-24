# 34: Implement Delay and its egui editor

**What to build:** Implement the reference multi-model Delay and its grouped native egui editor, using WaveBob timing and bounded tail/render semantics.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Reproduce supported mono/true-stereo/ping-pong/off delay models, time/pitch behavior, feedback filtering/distortion, modulation/diffusion and dry/wet mappings from the source.

- [ ] Free timing and tempo sync share exact visible units and project Effect tempo; never infer tempo from a selection. BPM/parameter changes invalidate bounces and transition without moving the playhead.

- [ ] Delay storage/time-model replacements are prepared/preallocated; modulation/feedback controls remain callback-safe. Reset/seek/loop behavior and intentional echo timing are distinct from computational latency compensation.

- [ ] Feedback at and beyond self-sustaining values terminates offline/bounce work through the common 60-second cap with explicit truncation metadata. Silent and bypassed paths do not gain spurious tails.

- [ ] Recreate source control groups/graphs/value entry in its separate window. Reference impulses, stereo/ping-pong timing, feedback/filter/nonlinear modes, tempo changes and capped tails satisfy the complete per-effect contract.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-delay/src/view.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/delay.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

