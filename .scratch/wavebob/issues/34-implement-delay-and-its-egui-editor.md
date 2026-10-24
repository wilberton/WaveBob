# 34: Implement Delay and its egui editor

**What to build:** Implement the reference multi-model Delay and its grouped native egui editor, using WaveBob timing and bounded tail/render semantics.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** done

## Acceptance criteria

- [x] Reproduce supported mono/true-stereo/ping-pong/off delay models, time/pitch behavior, feedback filtering/distortion, modulation/diffusion and dry/wet mappings from the source.

- [x] Free timing and tempo sync share exact visible units and project Effect tempo; never infer tempo from a selection. BPM/parameter changes invalidate bounces and transition without moving the playhead.

- [x] Delay storage/time-model replacements are prepared/preallocated; modulation/feedback controls remain callback-safe. Reset/seek/loop behavior and intentional echo timing are distinct from computational latency compensation.

- [x] Feedback at and beyond self-sustaining values terminates offline/bounce work through the common 60-second cap with explicit truncation metadata. Silent and bypassed paths do not gain spurious tails.

- [x] Recreate source control groups/graphs/value entry in its separate window. Reference impulses, stereo/ping-pong timing, feedback/filter/nonlinear modes, tempo changes and capped tails satisfy the complete per-effect contract.

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


### Completed implementation and automated acceptance

The focused WaveBob delay and native editor are enabled with all 26 original
controls and plugin START defaults. All four models, original filters/shapers,
modulation/diffusion, stepped smoothing, glide and keep-pitch behavior are
implemented. Fallible model-sized buffers and bounded retained-state maxima
support zero computational latency, deterministic reset/seek and continuous loops.
Shared explicit BPM changes preserve the cursor and invalidate render identity;
self-sustaining feedback uses the common 60 s cap with truncation metadata/taper.

[Detailed verification, controls, differences and native captures](../../../docs/verification/delay/README.md)
record 132 development, 52 endpoint and held-out native original comparisons,
all with zero error, plus the 12 s synced echo. Public offline/manual 1/7/64,
seek/loop, bypass/order/history/isolation, callback allocation/free safety and
direct/headless/MCP parity pass. Actual native editor pixels and exact BPM entry
were verified. GUI all-target tests (273), GUI-free all-target tests (152), final
eight-test Delay kernel pass and Clippy in both configurations pass.

The user approved automated acceptance; physical listening/device checks remain
explicitly unverified. Ticket 44 owns final measured real-time load and integrated
project/bounce/export verification. Pinned unchanged source is a dev-only reference
dependency and is never imported by production builds.

Native follow-up: [Rust run 40127933085](https://github.com/wilberton/WaveBob/actions/runs/40127933085)
passes all three native platforms at `057c003`, including the unchanged original
Delay comparisons, retained capture/source hash, GUI/GUI-free checks and release
builds. The declared tolerances remain unchanged.
