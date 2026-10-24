# Fast Distortion verification

Pinned Lemon-Studio reference `0af860dfd31c7ee9cbf8af6747617e031a1a7742`:
`crates/os-engine/src/effects/fast_dist.rs`, `crates/os-dsp/src/level.rs` and
`plugins/lemon-small-fx/src/fast_distortion.rs`. Copyright (C) 2026 ItsAlphaNeon;
GPL-3.0-or-later. WaveBob's focused implementation uses no engine/plugin SDK.

The [predeclared plan](plan.md) sets a 2e-7 maximum absolute PCM error, with zero
gain/frame correction. The isolated harness compiles both original source files
unchanged. Hash-checked source, compiler, binary, controls and complete stereo
input/output captures are retained in
`tests/fixtures/processing/fast-distortion/manifest.json` and lossless gzip/f32
files. The generator is `tools/effect-reference/render_fast_distortion.py`.

All **132 development cases match exactly** (maximum sample error 0), covering
all 20 A/B threshold curves at 44.1/48/96 kHz and 72 gain/mix/post combinations.
Inputs include a dense signed amplitude sweep, integer and half-table positions,
f32 neighbors at table boundaries, quiet and signed zero, positive/negative
steps, very loud finite values, and independent stereo sine/multitone signals.
The held-out mixed-control 96 kHz case was run after development passed and also
matches exactly, without tuning. No interpolation or makeup-gain correction is
applied to the expectations.

## Controls and exact signal law

All controls are stable integer parameters; numeric controls provide precise raw
entry and double-click reset. Displayed gain/mix values use the source level law,
not a linear approximation.

| ID | Range | Default | Display |
| --- | --- | --- | --- |
| `pre` | 64..192 | 128 | Pre amp in derived dB; 128 unity, 192 about +11 dB |
| `threshold` | 1..10 | 10 | Reference threshold number |
| `type` | 0..1 | 0 | A / B switch |
| `mix` | 0..128 | 128 | Derived distorted share in percent |
| `post` | 0..128 | 128 | Post gain in derived dB; 0 mute, 128 unity |

Gain knobs use `(11^position − 1) / 10` with the prescribed f32 intermediate
rounding and exact unity at raw 128. The source computes `wet = mix × post` and
`dry = (post − wet) × pre × post` in f32. Curve A compresses above −25 dB and
adds threshold-dependent makeup; B raises attenuation in dB to a power. An
8192-entry table stores rounded whole 16-bit levels plus a repeated last entry.
Lookup uses `abs(input × pre × 8191)`, doubles for wet-table interpolation over
the dry contribution, and final f32 rounding toward zero. From index 2^31 the
reference returns only dry signal; that very-loud-input behavior is preserved.
Supported controls reject values outside the published ranges.

This is the reference sampled waveshaper. Its nonlinear harmonics/intermodulation
can fold above Nyquist; no oversampling, smoothing filter or antialiasing mode is
added. The same transfer is compared at all three rates, preserving that behavior.
Dry/pre gain combinations can retain output headroom above ±1; float rendering
preserves it and export's existing clipping policy remains explicit.

Mono stays mono, stereo channels remain independent, computational latency and
tail are zero, and reset is memoryless. Prepared unit tables are immutable on the
audio path. Same-curve pre/mix/post coefficients slew over 10 ms without table
creation or destruction in callback. Threshold/type changes use the shared
prepared-path 20 ms crossfade. These live transitions deliberately differ from
the reference's abrupt updates; fixed settings remain identical. Tests verify
monotonic post mute, exact 10 ms completion, reset, sign symmetry and topology
selection.

## Application and native editor

Public offline renders and manual playback at 1/7/64-frame blocks agree with
reference cases at 44.1/48/96 kHz, including nonzero seeks. Metadata reports zero
latency/tail and source PCM remains unchanged. Direct/headless/MCP renders agree.
Independent owners, nonlinear stack order, bypass and undo pass. Live looped
pre/mix/post extremes, A/B switching and threshold changes preserve finite output
and instrumented callback allocation/free counts of zero. Cancellation restores
state and repeated preview updates commit one history entry.

Actual 900×430 native captures were inspected:
[default A](fast-distortion-default.png), [B](fast-distortion-type-b.png),
[capture metadata](fast-distortion-captures.json). INPUT, DISTORTION, CURVE and
OUTPUT preserve reference organization with numeric/reset access, A/B choices,
derived units and window-local help. The graph uses the exact interpolated audio
transfer including every gain/mix control; displayed axes are ±1 while audio
headroom is retained. Curve tables are cached on the UI thread. Stable duplicate
open/focus/close, changed processing identity and missing-window capture pass.

Reproduction with repository Rust paths and workspace-local TMP/TEMP:

- `python tools/effect-reference/render_fast_distortion.py PINNED_SOURCE_ROOT`
- `cargo test --offline --lib dsp::fast_distortion -- --nocapture`
- `cargo test --offline --test fast_distortion`
- `cargo test --offline --test transport fast_distortion`
- `cargo test --offline --test workflow fast_distortion`
- `cargo test --offline --test gui_attachment native_fast_distortion -- --ignored --nocapture`

Set `WAVEBOB_TEST_ARTIFACT_DIR` to this directory to retain native captures.
Automated acceptance is approved; physical listening, speaker/device and
trackpad checks remain explicitly unverified. No subjective listening is claimed.

Regression results: GUI all-target **255 passed**, GUI-free all-target **138
passed**. After the native palette follow-up, all **54 application UI tests**,
final all-target Clippy and the final actual editor capture/lifecycle test pass.
Clippy with warnings denied also passes GUI-free; rustfmt passes.
