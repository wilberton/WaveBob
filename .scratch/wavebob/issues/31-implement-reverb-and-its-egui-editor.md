# 31: Implement Reverb and its egui editor

**What to build:** Implement WaveBob's reference tank/plate-style Reverb and faithfully recreate its room visual and INPUT/ROOM/DECAY/MIX egui control groups.

**Blocked by:** 30 — Implement Soft Clipper and its egui editor.

**Status:** done

## Acceptance criteria

- [x] Reproduce the reference network and supported parameter mappings: input M/S and cuts, predelay/free-or-sync, size/diffusion/modulation speed/depth, bass/decay/crossover/damping, dry/early/wet levels and stereo separation. Use source ranges/defaults rather than the old three-parameter reverb.

- [x] Prepare delay-line/room-size changes off the callback and transition without repeatedly clearing audible state. Preserve fixed-setting reference behavior; document live-transition differences.

- [x] Render deterministic natural tails through source end, seek/reset and seeded modulation; comply with threshold/hold/cap metadata and stereo channel policy. Tempo switch uses the explicit project BPM.

- [x] Recreate room visualization and grouped knobs/switches/faders, source captions/units, precise values/reset and per-window help; Tempo is a toggle rather than the illustrative mockup dial.

- [x] Reference impulse, filtered/M-S stereo, modulation, extreme decay and room-size cases, plus live room changes, dry/wet/early combinations and tail termination, satisfy the per-effect delivery contract.

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


## Completed automated acceptance — 24 October 2026

Implemented the reference input filters/M-S routing, early reflections, predelay,
four diffusers, modulated two-half tank, decay/bass shelf/damping and output
matrices. Published all 15 integer controls and two toggles with source defaults,
ranges, units and raw/display laws. Live coefficient edits preserve prepared
state; room/predelay/routing topology changes prepare off the callback and
crossfade. Mono promotes to stereo; latency is zero; tails obey the shared policy.

The [predeclared plan](../../../docs/verification/reverb/plan.md) and
[results/control contract](../../../docs/verification/reverb/README.md) retain
reference provenance, fixed-setting tolerance, intentional transition/visual
adaptations and physical-check limits. All 24 development reference captures
pass: non-silent PCM error is zero, silent bias differs by <4.8e-21. The held-out
96 kHz case passed after development without tuning, also with zero error.
Public offline/manual playback passes at 1/7/64-frame blocks, and direct/headless/
MCP adapters agree. Natural tails, six-second predelay, 60-second cap/taper,
early-only termination, reset, seek, bypass/order, independent owners,
cancellation, single-step undo and unchanged source PCM are verified. Live room,
wet, damping, modulation and predelay changes allocate/free nothing in callback.

Actual [default](../../../docs/verification/reverb/reverb-default.png) and
[size-100](../../../docs/verification/reverb/reverb-size-100.png) native captures
were inspected with INPUT/ROOM/DECAY/MIX, exact entry, derived units, Tempo toggle,
room visualization and window help visible. Duplicate-open/focus/close and capture
identity regression passed. The initial inherited-horizontal layout error was
found in the first actual capture, corrected and recaptured.

Validation: GUI all-target tests **234 passed**, GUI-free all-target tests
**123 passed**; all-target Clippy in both configurations denies warnings, and
rustfmt passes. The final early-only activity refinement passes its focused tail
regression and final Clippy. Logs: `.tools/test31-{kernel,public,callback,adapters,
native-v2,all,nogui,tail-final}.log`, `.tools/clippy31{-nogui,-final}.log`.
Automated acceptance was approved by the user; physical listening/device checks
remain explicitly unverified. All criteria are complete; ticket 32 is unblocked.

## Shared tempo UI follow-up

The synced editor additionally exposes exact shared project BPM entry with a
single Set tempo action and current-value readout. A 90 BPM input/undo regression
passes, and the actual [synced native capture](../../../docs/verification/reverb/reverb-tempo-90.png)
was inspected with the field and help visible. `.tools/test31-tempo-ui.log` and
`.tools/test31-tempo-native.log` record passing checks. Physical checks remain
unverified under the approved automated acceptance.
