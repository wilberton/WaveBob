# 32: Implement Parametric EQ and its egui editor

**What to build:** Implement the standard seven-band minimum-phase/IIR Parametric EQ and its native response-graph/spectrum egui editor.

**Blocked by:** 31 — Implement Reverb and its egui editor.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Reproduce the standard reference band filters, supported shapes/slopes, frequency/gain/bandwidth mapping, band enable state and combined response; publish supported parameter metadata and defaults.

- [ ] Expose seven draggable bands, graph scales and exact control entry, shapes/slopes and a live spectrum fed through bounded snapshots. Preserve source interactions/layout while using egui.

- [ ] Offer only the supported standard algorithm modes. Advanced linear-phase and HQ/Enhanced mode controls are omitted; do not approximate those algorithms under their names.

- [ ] Prepare coefficient/topology changes and smooth supported edits without callback allocation or unstable filters; analyzer work scales down when the editor is hidden and never affects the audio result.

- [ ] Compare impulse/frequency response, multitone/stereo processing, bypassed/disabled bands, shape/slope extremes and live graph drags against reference cases; complete the per-effect contract with an inspected graph/editor capture.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-parametric-eq-2/src/view.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/eq/equalizer.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

