# 30: Implement Soft Clipper and its egui editor

**What to build:** Write the reference exponential soft-clipping algorithm in WaveBob DSP and reproduce its compact transfer-curve/threshold/post-gain editor in egui.

**Blocked by:** 29 — Build the effect stack and native editor framework.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Implement the reference exponential transfer and threshold/post-gain mapping, including below-threshold, knee, negative polarity and overload/headroom behavior; keep channel processing deterministic.

- [ ] Provide labeled threshold and post-gain controls, exact entry/reset and a transfer graph in the separate native window, using the source functional layout within WaveBob styling.

- [ ] Reference cases sweep input amplitude/polarity and parameter bounds, and compare output/curve values. Live threshold/gain changes remain smooth and callback-safe with unchanged source audio.

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

