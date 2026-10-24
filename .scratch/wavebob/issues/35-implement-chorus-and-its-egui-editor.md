# 35: Implement Chorus and its egui editor

**What to build:** Implement the reference Chorus's modulated delay/LFO and filtering algorithm with a faithful native egui editor.

**Blocked by:** 33 — Implement Fast Distortion and its egui editor.

**Status:** done

## Acceptance criteria

- [x] Match delay/depth/stereo, the three LFO speed/waveform controls, crossover/filtering and dry+wet/wet-only parameter mappings and reference algorithm.

- [x] Use fixed seeded initialization so repeated reset renders reproduce stereo phases. Define and verify pause, seek/reset and looping behavior through the shared processor contract.

- [x] Prepare capacity/filter changes outside the callback and smoothly update supported modulation/gain parameters with bounded delay buffers; channel layout and remaining delay output are reported correctly.

- [x] Recreate the source group layout, knobs/selectors and exact value/reset input in egui; hidden editor visuals do not affect processing.

- [x] Reference impulses, sustained tones/stereo signals, waveform/speed/depth extremes, reset repeats and wet-only operation satisfy the complete per-effect delivery contract.

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


### Completed implementation and automated acceptance

The reference three-LFO, quantized-table, fixed-point modulated delay and
one-pole crossover are implemented with all twelve raw integer controls,
source defaults, waveform/switch regions and exact derived units. Immutable LFO
tables and fallible delay/range-max storage prepare off callback. Compatible
updates slew 10 ms with state retained; waveform/band changes prepare 20 ms paths.
Mono promotes to stereo, computational latency is zero and residual delay/filter
state drains through common tails. Reset deterministically returns to seed-0
phases; pause and loops preserve phase/history. The original 4096-cell high-rate
wrap and deliberate host cleanup/reset/live-transition differences are documented.

[Detailed source contract, controls, results and native captures](../../../docs/verification/chorus/README.md)
record 99 development comparisons and the reserved 96 kHz mixed-control case,
all with zero error, plus 192 kHz wrap, exact reset and tail/silence checks.
Sixteen complete original captures retain hashes/settings/seed/compiler metadata.
Public offline/manual 1/7/64, seek, loop/pause, owner/order/bypass/history/source
isolation and direct/headless/MCP parity pass. Live callback allocations/frees
remain zero through modulation/mix/extremes and topology changes. Final native
920x630 default/Sharp-sines pixels and stable lifecycle/identity were inspected.

GUI all-target **285 tests** and GUI-free all-target **164 tests** pass. Clippy
in both configurations denies warnings; rustfmt and pinned source hash pass.
Final native capture test passes in 4.87 s. The user approved automated acceptance;
physical listening/device checks remain explicitly unverified. Ticket 44 owns
final actual-effect performance and complete project/bounce/export acceptance.
