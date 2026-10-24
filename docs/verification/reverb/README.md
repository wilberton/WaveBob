# Reverb verification

Pinned Lemon-Studio reference: `0af860dfd31c7ee9cbf8af6747617e031a1a7742`,
`crates/os-engine/src/effects/reverb.rs` and `plugins/lemon-reverb/src/{params,view,room}.rs`.
Copyright (C) 2026 ItsAlphaNeon; GPL-3.0-or-later. WaveBob's focused DSP uses no
upstream engine, plugin SDK or UI dependency. The room projection and measured
visual tables retain upstream provenance in `src/app/reverb_room.rs`.

The [predeclared plan](plan.md) sets maximum PCM error 2e-6 and relative RMS
error 2e-5 above reference energy 1e-12, with no alignment or gain correction.
The isolated generator compiles the pinned original DSP unchanged and retains
25 lossless gzip/f32 input/output captures plus hashes and full settings in
`tests/fixtures/processing/reverb/manifest.json`. It covers 44.1/48/96 kHz,
left/right impulses, independent stereo bursts, M/S cancellation, cuts/off,
free/synced predelay (six seconds at 20 BPM), size/diffusion extremes,
fast/slow full modulation, 20-second decay with 300% bass, separated/merged
stereo, dry/early/wet isolation and maximum levels.

All 24 development cases pass. Every non-silent sample matches exactly
(maximum error 0); silence differs by at most 4.793702692923378e-21 due to
WaveBob's exact-silence output suspension. The held-out mixed-control 96 kHz
case was run after development passed and also matches exactly, without tuning.
Complete captures are compared, including tails and arrival timing.

Public offline/manual-output comparisons cover default 44.1 kHz, filtered M/S
48 kHz and held-out 96 kHz at block sizes 1/7/64. Direct/headless/MCP renders
produce identical stereo PCM, including the tail. Reset repeats modulation
exactly. Repeated public seek-to-zero reproduces output. Continuous wet changes
retain the existing tail; room changes select prepared topology transitions.
Additional checks cover owner isolation, clipping/reverb order, bypass, undo,
source preservation and preview cancel versus single-step commit. Instrumented
live playback loops through wet, damping, modulation, size 1/100 and predelay
changes with zero callback allocations or frees and finite output.

The shared tail policy passes natural termination, mono-to-stereo promotion,
zero computational latency, delayed arrival after six seconds, dry-only zero
tail and an extreme decay that reaches the 60-second cap with a final zero
sample. Delay-ring energy and filter state prevent early silence detection.

## Stable controls

All numeric controls are integer raw values with exact entry and double-click
reset. Derived captions follow source units rather than illustrative mockups.

| ID | Range | Default | Display mapping |
| --- | --- | --- | --- |
| `low_cut` | 19..3000 | 75 | Hz; 19 Off |
| `high_cut` | 5..221 | 40 | 100 Hz; 221 Off |
| `predelay` | 0..384 | 0 | 1/384 s free; 1/192 beat synced |
| `size` | 1..100 | 50 | room size |
| `diffusion` | 0..100 | 100 | % |
| `decay` | 1..200 | 15 | 0.1 s |
| `damping` | 5..221 | 40 | 100 Hz; 221 Off |
| `bass` | 20..300 | 100 | % |
| `crossover` | 25..2000 | 500 | Hz |
| `separation` | -64..64 | 0 | ±64 = ±100%; negative separates, positive merges |
| `dry` | 0..160 | 128 | 128 = 100% |
| `early` | 0..160 | 128 | 128 = 100% |
| `wet` | 0..160 | 128 | 128 = 100% |
| `mod_speed` | 0..2048 | 684 | 2048 = 100% |
| `mod_depth` | 0..2048 | 0 | 2048 = 100% |
| `tempo_sync` | boolean | false | Predelay uses shared project BPM (20..400) |
| `mid_side` | boolean | false | Tank receives L−R rather than L+R |

Dry uses the reference raw/128 exponential level law; early/wet use raw/256
before their output matrices. Mono input is promoted to dual mono and output
retains stereo. Early reflections bypass intentional tank predelay. Phase starts
at zero and uses no RNG. Loop wraps retain state; reset/seek rebuild deterministic
context. Live continuous coefficients ramp over 10 ms; size, predelay length and
M/S routing changes prepare a fresh path off the callback and crossfade over
20 ms. Those transitions deliberately differ from the reference's abrupt edits;
fixed-setting behavior is the comparison target.

## Native editor

Actual default and size-100 captures were inspected at 1050×610:
[default](reverb-default.png), [size 100](reverb-size-100.png),
[capture metadata](reverb-captures.json). INPUT, ROOM, DECAY and MIX preserve
source grouping; Tempo is a toggle, levels are vertical faders, and every control
has precise raw entry and derived units. The rotating wireframe uses the source
34.97-second period, camera/side/shape/tint laws; bounded layered egui strokes
approximate the source raster Gaussian glow. This presentation adaptation does
not affect audio. Both captures show all controls, analyzer and per-window help.
Duplicate-open identity, updated processing identity, close and missing-window
capture behavior pass. Small windows scroll to expose all controls.

Reproduce with the repository Rust environment and workspace-local TMP/TEMP:

- `cargo test --offline --lib dsp::reverb -- --nocapture`
- `cargo test --offline --test reverb`
- `cargo test --offline --test transport reverb_`
- `cargo test --offline --test workflow reverb_`
- `cargo test --offline --test gui_attachment native_reverb_ -- --ignored --nocapture`

Set `WAVEBOB_TEST_ARTIFACT_DIR` to this folder to retain native captures. Regenerate
reference captures using `python tools/effect-reference/render_reverb.py PATH_TO_PINNED_SOURCE`.
Automated acceptance is approved. No physical listening, speaker, trackpad or
other-device result is claimed; those checks remain unverified.

Full regression results: GUI all-target 234 passed; GUI-free all-target 123 passed.
Clippy with warnings denied passes both configurations; rustfmt passes. The final
early-only activity refinement also passes its focused tail regression and final
all-target Clippy.

## Explicit shared tempo follow-up

Synced controls now expose exact **Project effect tempo** entry (20..400 BPM)
and a Set tempo action, plus the current shared BPM. The field appears only
when sync is enabled, so the 120 BPM fallback is visible. Setting 90 BPM commits
one history entry; undo restores 120. The actual
[90 BPM native capture](reverb-tempo-90.png) and [metadata](reverb-tempo-90.json)
were inspected at 1050×610 with controls, BPM field and help visible.
The UI input regression and native lifecycle/capture regression pass.
