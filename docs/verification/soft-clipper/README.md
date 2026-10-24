# Soft Clipper verification contract

Pinned reference: Lemon-Studio `0af860dfd31c7ee9cbf8af6747617e031a1a7742`,
`crates/os-engine/src/effects/soft_clipper.rs`, `crates/os-dsp/src/level.rs`, and
`plugins/lemon-small-fx/src/soft_clipper.rs`. Copyright (C) 2026 ItsAlphaNeon,
GPL-3.0-or-later. WaveBob's adaptations retain this provenance in the DSP source.

Before comparison: require maximum absolute PCM error **2e-7**, with no alignment
or gain correction. Fixed/reset settings use the reference's f32/f64 rounding,
exponential knee, x87-grid correction and post-gain level curve. Do not change the
tolerance in response to an unexplained mismatch.

Stable controls are integer `threshold` (1..127, default 100) and `post`
(0..160, default 128). Threshold level is raw/128; post gain is the reference
(11^position-1)/10 level mapping with its prescribed intermediate rounding.
The editor displays derived dB and provides exact raw control values. Post 0 is
mute and 128 is unity. Inputs above full scale bend toward ±1 before post gain;
output may retain headroom up to approximately 1.9033 at post 160.

Each channel is independent. Mono stays mono; stereo stays stereo. There is no
computational latency, audible tail, randomness or tempo dependence. Reset is
memoryless. Live threshold and gain coefficients ramp over 10 ms in WaveBob;
reference abrupt updates remain the fixed-setting comparison target. Envelopes
precede this stage, and source PCM is never replaced.

The isolated reference harness directly compiles the pinned original source files
with small state-serialization/trait adapters. It does not link production WaveBob
code or upstream engine/plugin dependencies. The generator records source/input/
output hashes, Rust compiler, raw settings, rate, seed and block policy. Generated
fixtures include both polarities, threshold neighbors, silence, impulse/steps,
multitone stereo signals and finite extreme inputs at 44.1/48/96 kHz.

Implementation/measurement results and inspected native captures follow below.

## Measured results

The isolated generator (`tools/effect-reference/render_soft_clipper.py`) compiled
the original pinned files unchanged and produced 36 cases. The manifest and PCM
live under `tests/fixtures/processing/soft-clipper/`. Comparison measured **zero
maximum absolute error** across every fixture value, within the predeclared 2e-7
tolerance. No gain/alignment correction was used.

All 36 settings/rate cases also pass public `processing.render`/samples comparison
and deterministic manual playback at 1/7/64-frame block sizes. Source rates are
44.1/48/96 kHz with independent stereo signals. Additional mono clip cases verify
output layout, production-stage ordering, bypass/removal, chronological undo,
instance isolation and unchanged source PCM. Live preview/cancel ramps smoothly
and callback instrumentation records zero allocations/frees through preparation
handoff and parameter changes. Repeated preview updates commit one history entry.
Direct, headless and MCP public renders produce identical PCM.

Actual native default and threshold-32 client-area captures were inspected at
720×460. The editor has separate CLIPPER and CURVE cards, labeled threshold/post
knobs, exact raw values, derived dB readouts and the reference transfer extent.
The post gain is included in the curve. Screenshot input locking preserves normal
opacity and blocks mutations. The first native test found that eframe 0.37's
immediate-window renderer never completes screenshot actions despite painting the
correct viewport. Moving editors to deferred native viewports fixed the failing
capture loop (8–11 s red; approximately 4.3 s green). The regression also verifies
curve/readout changes, stable duplicate-open identity and close behavior. Temporary
`DEBUG-capture30` instrumentation and its failure log were removed.

Reproduction: set the repository Rust environment and workspace-local TMP/TEMP,
then run `cargo test --offline --no-default-features --test soft_clipper --test
transport`, `cargo test --offline --no-default-features --test workflow
soft_clipper_render`, and `cargo test --offline --test gui_attachment
native_soft_clipper -- --ignored --nocapture`. Set `WAVEBOB_TEST_ARTIFACT_DIR` to
this directory to retain the actual viewport images.

No human listening result is claimed. The acceptance interpretation for physical
listening/device checks is pending the user's response; objective results above
are measured, and the captured native editor is visually inspected.
