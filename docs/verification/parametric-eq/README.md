# Parametric EQ verification

Pinned Lemon-Studio reference: `0af860dfd31c7ee9cbf8af6747617e031a1a7742`,
`crates/os-engine/src/effects/eq.rs`, `eq/{design,equalizer}.rs` and
`plugins/lemon-parametric-eq-2/src/{params,view,editor}.rs`.
Copyright (C) 2026 ItsAlphaNeon; GPL-3.0-or-later. WaveBob adapts the ordinary
coefficient mathematics and implements its own fixed-quantum streaming and native
editor. No engine/plugin SDK, matched/HQ/Enhanced or linear-phase processing is
linked into production. Those unsupported modes have no parameters or controls.

The [predeclared plan](plan.md) sets max PCM error 2e-6, relative RMS error 2e-5
above energy 1e-12, and response error 2e-5 dB above -120 dB, without gain or
frame correction after declared computational latency compensation.
The isolated harness compiles original, hash-checked reference modules unchanged
in a private module tree; production DSP is not used to generate expectations.
Reference source/compiler/binary/input/output hashes and all settings are retained
in `tests/fixtures/processing/parametric-eq/manifest.json`. Lossless gzip/f32 files
preserve complete stereo captures, not sparse response samples.

All **88 development cases** pass with **zero maximum PCM error and zero
relative RMS error**. They cover defaults and multitone stereo at 44.1/48/96 kHz,
every supported shape and all seven orders, frequency/width/gain extremes,
disabled types and band masks, main level, silence and the gate/countdown boundary.
The held-out mixed seven-band 96 kHz case was run after development passed and
also matches exactly, without tuning. Every case additionally compares 128
logarithmic response points against the original reference.

## Stable control contract

There are seven independent bands (suffix `1`..`7`) and one main level:

| IDs | Integer range | Defaults | Meaning |
| --- | --- | --- | --- |
| `level1`..`level7` | -1800..1800 | 0 | Hundredths of dB; shelves and peaking |
| `freq1`..`freq7` | 0..65536 | 10923, 18204, 25486, 32768, 40050, 47332, 54613 | 20 × 1000^(raw / 65536) Hz, with reference f32 rounding |
| `width1`..`width7` | 0..65536 | 40124 for shelves, 25412 for middle peaks | Normalized bandwidth; UI shows percentage |
| `type1`..`type7` | 0..7 | 5, 6, 6, 6, 6, 6, 7 | Disabled, low pass, band pass, high pass, band stop, low shelf, peaking, high shelf |
| `order1`..`order7` | -3..3 | 0 | Steep 8/6/4, 2, Gentle 4/6/8; 1..4 sections |
| `enabled1`..`enabled7` | boolean | true | Independent band enable mask |
| `main` | -1800..1800 | 0 | Hundredths of dB before the band cascade |

Exact unit fields enter dB, Hz and bandwidth percent, quantized to the published
integer settings. Shape/order menus are available below the graph and on its
context menu. Double-click resets a numeric control. Reset selected band or
Alt-click restores level/frequency/type-appropriate width, preserving shape and
order. Graph tokens drag frequency/gain together; Shift-drag and wheel adjust
width. All multi-parameter gestures commit one chronological undo entry; Escape
restores the prior state and suppresses further edits until pointer release.

## Streaming and timing

Plain reference processing gates and tidies state at block boundaries. A fixed
**64-frame internal quantum** preserves that behavior across arbitrary host
blocks. The stage reports **64 computational latency frames**, compensated once
by offline/manual/arrangement rendering; minimum-phase filter phase remains.
Cold reset follows the reference plain preroll behavior; loop wraps carry filter,
countdown and staging state. Mono stays mono; stereo channels process independently.
The reference silence gate and zero-gain countdown are preserved, including their
subaudible cleanup. The shared renderer still applies tail hold/cap accounting.

Same-shape frequency/width/level and main-gain updates interpolate stable biquad
coefficients over 10 ms while retaining section/staging state. Shape, order or
band-mask changes prepare a new path off the callback and crossfade over 20 ms.
These live transitions deliberately differ from reference abrupt automation;
fixed-setting comparisons use unchanged reference coefficients and evaluation.

Public offline/manual comparisons pass at blocks 1/7/64 across 44.1/48/96 kHz,
including non-aligned seeks at frames 113 and 2049. A 127-frame loop agrees with
a continuous repeated source despite its boundary crossing the internal quantum.
Owner isolation, band/effect bypass, nonlinear stack order, undo and source PCM
preservation pass. Direct/headless/MCP renders match exactly. Callback allocation
instrumentation covers continuous extremes, shape/order changes, bypass and
removal (64-to-zero latency transition): zero allocations/frees, finite output,
and convergence to the aligned dry signal. Reset repeats output exactly.

## Native editor and analyzer

Actual 1120×700 native captures were inspected:
[default](parametric-eq-default.png), [band 4 +12 dB](parametric-eq-band-4.png),
[capture metadata](parametric-eq-captures.json). They show the logarithmic
20 Hz–20 kHz / ±18 dB graph, seven colored tokens, eight level faders, band enable
switches and selected-band shape/order/exact controls. The changed capture shows
the correct +12 dB response and corresponding fader, with stable owner/window IDs.
Small windows scroll to expose controls. Duplicate-open/focus/close and capture
identity regression passes.

The output-spectrum overlay is computed only in the visible UI, using a Hann
window on at most 128 recent transport frames and 64 log frequency points.
Stereo channel powers are averaged so anti-phase signals do not disappear.
The short bounded snapshot limits low-frequency resolution; it is an output
activity aid, while the yellow curve is the exact prepared filter response.
Analyzer calculation never runs in or changes the audio processor and stops
when the native window is hidden. Automated tests check finite spectra, silence
and anti-phase energy equivalence.

Reproduction with repository Rust paths and workspace-local TMP/TEMP:

- `python tools/effect-reference/render_parametric_eq.py PINNED_SOURCE_ROOT`
- `cargo test --offline --lib dsp::parametric_eq -- --nocapture`
- `cargo test --offline --test parametric_eq`
- `cargo test --offline --test transport parametric_eq`
- `cargo test --offline --test workflow parametric_eq`
- `cargo test --offline --bin wavebob parametric_eq`
- `cargo test --offline --test gui_attachment native_parametric_eq -- --ignored --nocapture`

Set `WAVEBOB_TEST_ARTIFACT_DIR` to this directory for native captures. Automated
acceptance is approved. Physical listening, speakers, devices and trackpad checks
remain explicitly unverified. No subjective listening result is claimed.

Full regression verification: GUI all-target **248 passed**, GUI-free all-target
**131 passed**. Clippy with warnings denied passes both configurations; rustfmt
passes. The final actual native capture/lifecycle test also passes after exact-
field precision and context-menu changes.
