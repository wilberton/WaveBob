# 32: Implement Parametric EQ and its egui editor

**What to build:** Implement the standard seven-band minimum-phase/IIR Parametric EQ and its native response-graph/spectrum egui editor.

**Blocked by:** 31 — Implement Reverb and its egui editor.

**Status:** done

## Acceptance criteria

- [x] Reproduce the standard reference band filters, supported shapes/slopes, frequency/gain/bandwidth mapping, band enable state and combined response; publish supported parameter metadata and defaults.

- [x] Expose seven draggable bands, graph scales and exact control entry, shapes/slopes and a live spectrum fed through bounded snapshots. Preserve source interactions/layout while using egui.

- [x] Offer only the supported standard algorithm modes. Advanced linear-phase and HQ/Enhanced mode controls are omitted; do not approximate those algorithms under their names.

- [x] Prepare coefficient/topology changes and smooth supported edits without callback allocation or unstable filters; analyzer work scales down when the editor is hidden and never affects the audio result.

- [x] Compare impulse/frequency response, multitone/stereo processing, bypassed/disabled bands, shape/slope extremes and live graph drags against reference cases; complete the per-effect contract with an inspected graph/editor capture.

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


## Completed automated acceptance — 24 October 2026

Implemented ordinary seven-band IIR processing with reference shape/order/steep
section mathematics, frequency/gain/width mapping, band enables and main level.
HQ, Enhanced/matched and linear-phase modes are absent from DSP metadata and UI.
The [plan](../../../docs/verification/parametric-eq/plan.md) predeclares PCM and
response tolerances and the fixed 64-frame internal quantum, with reported and
compensated computational staging latency. Reset, source block gate/countdown
and f32/f64 precision follow the reference. Continuous same-shape edits preserve
state and slew 10 ms; topology changes prepare off-callback and crossfade 20 ms.

All 88 development captures plus the held-out mixed seven-band 96 kHz capture
pass with zero PCM error. Every case passes 128 response-point comparisons.
Public offline/manual blocks 1/7/64 and non-aligned seeks agree across rates;
a non-aligned 127-frame loop agrees with a continuous repeated source.
Direct/headless/MCP parity, independent owners, nonlinear order, bypass, undo,
source preservation and deterministic reset pass. Instrumented continuous,
shape/order, bypass and removal transitions allocate/free nothing in callback,
remain finite and converge to the correctly aligned dry signal.

Actual [default](../../../docs/verification/parametric-eq/parametric-eq-default.png)
and [band 4 +12 dB](../../../docs/verification/parametric-eq/parametric-eq-band-4.png)
native captures were inspected after final control changes. The editor has
seven draggable tokens, logarithmic scales, faders, enable switches, exact dB/Hz/
width entry, shapes/orders/context menus and per-window help. Visible-only
bounded output spectra preserve anti-phase channel energy. Input regressions
verify two-parameter graph drags and resets commit once, and Escape restores
state without restarting a held drag. Native open/focus/close/capture identity
regressions pass. The first input test caught a drag-origin hit-test error, which
was corrected before final verification.

[Full results/control contract](../../../docs/verification/parametric-eq/README.md)
records provenance, adaptations and all controls. GUI all-target tests **248
passed**; GUI-free all-target tests **131 passed**; both all-target Clippy
configurations deny warnings and rustfmt passes. Logs are
`.tools/test32-{reference,heldout,public-final,loop,callback,adapters,ownership,
gui-input,native-final,all,nogui}.log` and `.tools/clippy32-{final,nogui}.log`.
The same verification also covers the Reverb explicit shared-BPM UI follow-up.
Automated acceptance is approved; physical listening/device checks remain
explicitly unverified. All criteria are complete; ticket 33 is unblocked.
