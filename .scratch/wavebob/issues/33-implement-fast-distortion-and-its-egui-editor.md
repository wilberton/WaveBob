# 33: Implement Fast Distortion and its egui editor

**What to build:** Implement Fast Distortion with the reference waveshaping/table algorithm and recreate the compact controls and transfer graph in egui.

**Blocked by:** 32 — Implement Parametric EQ and its egui editor.

**Status:** done

## Acceptance criteria

- [x] Match preamp, threshold, A/B curve type, mix and post-gain mapping and reference transfer/table behavior, including signed input and output levels.

- [x] Keep expensive table preparation and publication outside the callback; define smooth gain/mix changes and bounded transitions between curve types without changing the fixed-setting algorithm.

- [x] Provide source-organized knobs/switches, numeric/reset access and an accurate transfer graph in its separate window. Never imply new oversampling/antialiasing modes that are not implemented.

- [x] Amplitude sweeps, nonlinear sine/multitone inputs, stereo/impulse and parameter extremes compare to the pinned reference. Document reference aliasing characteristics rather than silently replacing the algorithm.

- [x] Complete the per-effect delivery contract, including supported live edits, instance/history isolation, manual/offline parity and inspected actual controls.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

The contract's [per-effect delivery requirements](../effects-and-clip-controls.md#per-effect-delivery-contract)
are part of this ticket's acceptance. Use the [source investigation](../lemon-effects-review.md)
for the reviewed algorithm/dependency findings.

Pinned reference: [effect UI/parameter organization](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/plugins/lemon-small-fx/src/fast_distortion.rs), [DSP algorithm](https://github.com/ItsAlphaNeon/Lemon-Studio/blob/0af860dfd31c7ee9cbf8af6747617e031a1a7742/crates/os-engine/src/effects/fast_dist.rs).
Write focused WaveBob code and egui controls; keep reference tools outside production dependencies.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.


## Completed automated acceptance — 24 October 2026

Implemented the pinned A/B 8192-level table shaper, repeated last entry, precise
signed interpolation, prescribed f32 gain/mix law and final toward-zero rounding.
Published all five stable integer controls/ranges/defaults and derived display
units. Tables prepare off callback. Same-curve pre/mix/post edits slew 10 ms;
threshold/type changes use bounded prepared 20 ms transitions. Mono/stereo policy,
zero latency/tail, memoryless reset and original aliasing behavior are explicit.

The [plan](../../../docs/verification/fast-distortion/plan.md) declares 2e-7
maximum PCM error before comparison. All 132 development cases plus the held-out
96 kHz case match exactly (maximum error 0), covering all 20 curves, three rates,
72 level/mix combinations, signed/table-boundary neighbors, impulses/steps,
independent stereo/multitone and very loud finite inputs. Reference compiler,
source/binary/input/output hashes and settings are retained with complete
lossless captures. No oversampling or antialiasing mode is implied or added.

Public offline/manual blocks 1/7/64 and nonzero seeks match the reference;
direct/headless/MCP agree. Independent owners, nonlinear order, bypass/undo and
source preservation pass. Live pre/mix/post, A/B and threshold edits allocate/free
nothing in callback. Cancellation and repeated preview updates preserve single-
step history. Focused tests verify ten-millisecond monotonic mute, exact reset,
sign symmetry and prepared topology selection.

Actual [A](../../../docs/verification/fast-distortion/fast-distortion-default.png)
and [B](../../../docs/verification/fast-distortion/fast-distortion-type-b.png)
native captures were inspected at 900×430 after final source changes. They show
INPUT/DISTORTION/CURVE/OUTPUT, exact raw/reset controls, derived gain/mix units,
A/B switches and the exact interpolated transfer. Stable native lifecycle and
processing/capture identity pass. The [full delivery contract/results](../../../docs/verification/fast-distortion/README.md)
records source provenance, live-transition differences, headroom and aliasing.

GUI all-target tests **255 passed**; GUI-free all-target tests **138 passed**.
After the native palette follow-up, all **54 application UI tests** and the
final native capture test pass. Both Clippy configurations deny warnings;
final all-target Clippy and rustfmt pass. Logs:
`.tools/test33-{reference,heldout,kernel,public-final,callback,adapters,native-final,
all,nogui,gui-final}.log`, `.tools/clippy33-{final,nogui,palette}.log`.
Automated acceptance is approved; physical listening/device checks remain
explicitly unverified. All criteria are complete, unblocking tickets 34–40.
