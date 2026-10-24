# 30: Implement Soft Clipper and its egui editor

**What to build:** Write the reference exponential soft-clipping algorithm in WaveBob DSP and reproduce its compact transfer-curve/threshold/post-gain editor in egui.

**Blocked by:** 29 — Build the effect stack and native editor framework.

**Status:** in-progress

## Acceptance criteria

- [x] Implement the reference exponential transfer and threshold/post-gain mapping, including below-threshold, knee, negative polarity and overload/headroom behavior; keep channel processing deterministic.

- [x] Provide labeled threshold and post-gain controls, exact entry/reset and a transfer graph in the separate native window, using the source functional layout within WaveBob styling.

- [x] Reference cases sweep input amplitude/polarity and parameter bounds, and compare output/curve values. Live threshold/gain changes remain smooth and callback-safe with unchanged source audio.

- [ ] Complete and record the per-effect delivery contract, including command/manual/offline agreement, isolation/history and an inspected actual editor capture; this is the first complete production effect proving the framework.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/soft_clipper.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/soft_clipper.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.


## Implementation evidence — 24 October 2026

Soft Clipper is registered with stable integer threshold/post controls and a real
streaming constructor. The reference's exponential/level-curve intermediate
precision is preserved; memoryless reset results match all 36 pinned cases exactly.
WaveBob live coefficient updates ramp over 10 ms, retain source PCM and allocate
nothing in the callback. Native controls show raw exact values with derived dB,
and the transfer curve includes post gain. Actual editor PNGs were inspected.

See [effect verification and capture evidence](../../../docs/verification/soft-clipper/README.md),
[reference fixture manifest](../../../tests/fixtures/processing/soft-clipper/manifest.json)
and the isolated source-driven generator under `tools/effect-reference/`.

The first production native capture exposed an eframe immediate-viewport readback
limitation. Deferred native rendering over the shared UI model fixes it; the
native regression now passes for default/changed states, stable focus reuse and
close. Numeric reference/public/manual agreement, mono/stereo layout, stage
order/bypass/removal, isolation/undo and live callback safety checks pass.

The last delivery-contract box remains open for the required listening acceptance
interpretation. No physical listening result is fabricated. The user has been
asked asynchronously whether automated reference/native checks may establish
acceptance while physical checks remain explicitly unverified. Other authorized
implementation/verification continues while that response is pending.

Final automated checks pass: complete GUI-free suite including both baseline
WAV/MP3 and Soft Clipper direct/headless/MCP workflows (`.tools/test30-full.log`),
GUI library/application tests (`.tools/test30-gui.log`), and GUI and GUI-free
all-target Clippy with warnings denied. The actual native regression passes on
the original one-second input after debug instrumentation cleanup; retained PNGs
have normal opacity and capture metadata sidecars. Ticket 30 stays in-progress
only for the explicitly pending listening-acceptance decision. Ticket 41 is an
independent next unblocked implementation while 31 remains blocked by 30.
