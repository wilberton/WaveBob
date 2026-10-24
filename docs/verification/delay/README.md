# Delay verification

Pinned Lemon-Studio `0af860dfd31c7ee9cbf8af6747617e031a1a7742`,
`crates/os-engine/src/effects/delay.rs` and
`plugins/lemon-delay/src/{params,view,lib}.rs`. Copyright/GPL notices remain.
Production uses WaveBob DSP and egui modules without an upstream engine/SDK.
The unchanged original is confined to the test-only reference crate and capture
harness. [Predeclared plan](plan.md): maximum error 2e-6, relative RMS 2e-5 above
energy 1e-12, no alignment/gain correction.

All **132 development comparisons** at 44.1/48/96 kHz, the mixed 96 kHz held-out
case and 20 BPM / 12-second echo match exactly on Windows (maximum and RMS error
zero). A further 52 individual control-minimum/maximum comparisons also have
zero error. Cases cover every model, offset/spread, pitch/glide, filters/resonance,
limiter/saturator, drive/knee/symmetry, sample/bit reduction, modulation/diffusion,
tone and dry/wet. Native tests compare unchanged original source on every OS,
preserving the tolerance despite platform sinf ulps in modulated fractional taps.
Fourteen complete reference captures with compiler, binary/source, settings,
input/output hashes and reset/tempo are retained in
`tests/fixtures/processing/delay/manifest.json`. Reproduce with
`python tools/effect-reference/render_delay.py PINNED_SOURCE_ROOT`.
Original SHA-256: `6abba71c6613e039408d5464629d4b44b6f1834f6b10369785785cc5b916ecfa`.

## Host behavior

The plugin START defaults are True stereo, synced one beat at visible shared
120 BPM, Saturate, open LP, zero resonance/diffusion spread. Numeric values offer
exact raw entry, keyboard/Shift fine adjustment and double-click reset. Boolean
and discrete choices expose every supported state.

Mono promotes to stereo; bypass retains the prepared stereo layout, dry signal
and zero tail. Computational latency is zero; echo/input-offset arrivals remain
intentional. The original engine start (8192 silent frames and two resets)
establishes fixed settings. WaveBob reset restores complete scalar/LFO/hold state
and ring positions; the original partial reset and plugin cold direct-frame
startup differ deliberately. Loop wraps preserve state.

Compatible changes retain buffers and the source every-eight-frame coefficient
smoothing, glide and 100 ms keep-pitch crossfade. The original 5 ms coefficient
constant is stepped every eight frames; its response is preserved. Model/filter/
shaper topology and shared BPM changes prepare 20 ms paths. Explicit shared
20..400 BPM invalidates render identity without moving the cursor; selection
never determines tempo. Reverb and Delay share the same exact BPM entry.

All model-sized power-of-two storage is fallibly prepared off callback, after
checking the 64 MiB chain budget. A 192 kHz true-stereo instance fits; excessive
rates/storage return resource errors. Range maxima over 64-frame blocks bound
potentially audible history without full-buffer scans or allocation-length tails.
Silence/wet-zero/bypass acquire no tail. At feedback >=100%, retained state remains
active until drained or capped. The shared 60 s cap reports truncation and tapers
the final 10 ms to zero. Output-tone corners at/above Nyquist are bounded just
below it at low rates; below-Nyquist reference math is unchanged. SVF keeps the
original 0.49-rate clamp. Nonlinear aliasing remains reference behavior; no new
oversampling or alias suppression changes the sound.

## Public and native acceptance

Offline/manual 1/7/64-frame output agrees with native original PCM for every
model across the three reference rates, including seeks 113/2049. A 127-frame
loop agrees with continuously repeated source. The public 12 s predelay survives
the silent gap. Natural tails, silence, wet-zero, bypass and self-sustaining cap
metadata pass. Owner isolation, nonlinear order, undo and unchanged source PCM
pass. Live timing/feedback/modulation/diffusion extremes and topology changes stay
finite with callback allocation/free counts zero; preview cancel and single-step
commit/undo pass. Direct/headless/MCP samples agree.

Actual 1450x730 [default](delay-default.png) and [Ping pong](delay-ping-pong.png)
native captures were inspected against the source organization. INPUT, DELAY TIME,
DELAY MODEL, FEEDBACK, MODULATION, DIFFUSION, FEEDBACK DISTORTION and OUTPUT are
readable with exact values, source units, static shaper graph and shared BPM.
The limiter graph is labelled static while audio follows its envelope. Off
retains filtering/shaping. Duplicate open/focus/close and changed capture identity
pass; [metadata](delay-captures.json) records actual pixels. UI BPM entry commits
once and undoes to 120 BPM.

The user approved automated acceptance. Reference/manual output and native pixels
were measured. Physical listening, hardware and physical-device checks remain
unverified. Ticket 44 measures actual-effect load/input latency and cross-effect
export acceptance.

Reproduction with repository Rust environment and local TMP/TEMP:

- `cargo test --offline --lib dsp::delay -- --nocapture`
- `cargo test --offline --test delay`
- `cargo test --offline --test transport delay`
- `cargo test --offline --test workflow delay`
- `cargo test --offline --bin wavebob synced_delay`
- `cargo test --offline --test gui_attachment native_delay -- --ignored --nocapture`

## Published controls

| Stable ID | Raw range | Default | Derived meaning |
| --- | --- | --- | --- |
| `input` | 0..6000 | 6000 | Linear % |
| `type` | 0..3 | 1 | Mono / True stereo / Ping pong / Off |
| `tempo_sync` | boolean | true | Toggle |
| `keep_pitch` | boolean | false | Toggle |
| `time` | 1..768 | 192 | 1/192 beat synced; cubic 1..1000 ms free |
| `offset` | 0..6000 | 3000 | Signed R/L % |
| `smooth` | 0..1000 | 500 | 0..1000 ms glide |
| `spread` | 0..6000 | 6000 | Stereo % |
| `mod_rate` | 0..6000 | 3000 | Source interpolated Hz |
| `mod_time` | 0..6000 | 0 | Squared modulation amount |
| `mod_cutoff` | 0..6000 | 0 | Cutoff modulation % |
| `diffusion_level` | 0..6000 | 0 | All-pass gain % |
| `diffusion_spread` | 0..6000 | 0 | All-pass size % |
| `distortion_type` | 0..1 | 1 | Limit / Saturate |
| `feedback` | 0..6000 | 3000 | 0..125% (4800 = 100%) |
| `filter` | 0..3 | 0 | LP / HP / BP / Off |
| `cutoff` | 0..6000 | 6000 | Source corner tables in Hz |
| `resonance` | 0..6000 | 0 | Resonance % |
| `drive` | 0..6000 | 0 | Source threshold/makeup % |
| `hardness` | 0..6000 | 0 | Parabolic knee % |
| `symmetry` | 0..6000 | 0 | Asymmetric ceilings % |
| `rate_reduction` | 0..60000 | 60000 | Source hold, displayed Hz |
| `bits` | 0..6000 | 6000 | Nonlinear bits; 6000 = off |
| `wet` | 0..6000 | 6000 | Linear % |
| `tone` | -3000..3000 | 0 | Signed LP/HP colour %, 0 neutral |
| `dry` | 0..6000 | 6000 | Linear % |

Validation: GUI `cargo test --offline --all-targets` passes 273 tests; GUI-free
`cargo test --offline --no-default-features --all-targets` passes 152 tests.
The final Delay kernel pass adds the endpoint test and passes all eight tests
(including all 52 endpoints). GUI and GUI-free Clippy with `-D warnings` pass;
`cargo fmt --all` and the preserved original-source hash check pass. Native
viewport acceptance was explicitly run and passed in 5.92 s. Logs are retained
locally under `.tools/test34-*` / `.tools/clippy34-*`.
