# Chorus verification

Reference: Lemon-Studio `0af860dfd31c7ee9cbf8af6747617e031a1a7742`,
`crates/os-engine/src/effects/chorus.rs` and
`plugins/lemon-small-fx/src/chorus.rs`. Copyright/GPL notices are retained.
Production imports no upstream engine/plugin SDK. Unchanged original code is a
dev-only reference; its SHA-256 is asserted:
`c1058f431e3c963ffd9ca8821b56b11a931a020b955723b9d7d910c39f0c9fd5`.

The [predeclared plan](plan.md) sets max error 2e-6 and relative RMS 2e-5 above
energy 1e-12, zero gain/frame correction. All **99 development comparisons**
(default, every raw endpoint, four shapes x both bands in wet-only mode) at
44.1/48/96 kHz match exactly: maximum and RMS error zero. The reserved mixed
96 kHz case also matches exactly after development passed. Inputs include signed
impulse/steps, sustained independent stereo tones/multitones and trailing silence.
Native original comparisons avoid treating platform trig ulps as algorithm changes.
The original 192 kHz modulo-buffer behavior is separately checked.

Sixteen full original input/output captures with source/compiler/binary/settings,
seed, 64-frame block and PCM/gzip hashes are retained in
`tests/fixtures/processing/chorus/manifest.json`. Reproduce with
`python tools/effect-reference/render_chorus.py PINNED_SOURCE_ROOT`.

## Controls and signal law

All twelve controls retain original integer IDs/ranges/defaults, including raw
switch regions. Selectors choose canonical source values; exact entry can reach
any raw value in a region. Numeric fields offer arrows, Shift fine adjustment
and double-click reset. The default first waveform's raw 23 remains 23.

| ID | Raw range | Default | Display / meaning |
| --- | --- | --- | --- |
| `delay` | 0..1024 | 512 | 0.05..30 ms linear |
| `depth` | 0..5000 | 2250 | 0..5 ms linear |
| `stereo` | 0..1024 | 341 | Quantized right LFO advance, -0.1..179.9 degrees |
| `speed1` | 0..5000 | 1500 | 5 x squared Hz (default 0.45 Hz) |
| `speed2` | 0..5000 | 2500 | 5 x squared Hz (default 1.25 Hz) |
| `speed3` | 0..5000 | 3500 | 5 x squared Hz (default 2.45 Hz) |
| `wave1` | 0..1024 | 23 | Arc / Sharp arc / Sines / Sharp sines |
| `wave2` | 0..1024 | 0 | Same four regions |
| `wave3` | 0..1024 | 0 | Same four regions |
| `cross` | 0..100 | 0 | Highs through 25; Lows from 26 |
| `cutoff` | 0..5000 | 2500 | MIDI-mapped requested 8.2..12543.9 Hz |
| `wet` | 0..1024 | 0 | Dry + wet through 256; Wet only from 257 |

Wave boundaries are 0..341 Arc, 342..682 Sharp arc, 683..1023 Sines, 1024
Sharp sines. Source integer tables (4096 entries x four shapes), truncated
fixed-point delay/depth/phase/step and wrapping multiplication are preserved.
Input gain is exactly double 0.7 rounded to f32. One-pole crossover divides the
input; selected band feeds three fractionally interpolated taps, summed at 0.6.
Dry + wet adds the 0.7 input; wet-only omits it. Headroom above unity and the
source's interpolation/aliasing remain; no unrequested oversampling is added.
The crossover's actual corner is capped at sample_rate/4.1 as in the source.

## Host adaptations and real-time behavior

Mono promotes to stereo, retained through bypass. Computational latency is zero;
modulated delays are intentional. Seed 0 gives phases [0,1028,28213]. WaveBob
reset restores those phases and empty lines/filter/quantum, while original start
would draw the generator's next three values. This deliberate difference makes
repeated reset/offline/seek renders deterministic. Pause retains states and loop
wraps continue them. Tiny crossover cleanup occurs every internal 64 frames,
independent of host blocks, adding no computational delay.

Tables prepare fallibly once off callback; audio retains a direct immutable
pointer and performs no lazy initialization/locking. Two original 4096-frame
rings are fallibly prepared. The original wrap at very high rates is preserved,
rather than silently changing the algorithm's fixed delay memory. Point/range
maximum trees bound retained crossover/delay state with logarithmic work and no
buffer scan. Conservative charged storage is below 256 KiB per processor.
Silent input acquires no tail; natural tails and bypass follow the common policy.

Timing/depth/stereo/speed, crossover coefficients and dry level slew 10 ms while
retaining lines, filter state and phases. Waveform/crossover-band topology uses
prepared 20 ms paths. Coefficient design/capacity work stays off callback. These
smooth live transitions deliberately replace the source's abrupt setting changes;
fixed settings remain identical. Reset after a slew equals a fresh target instance.

## Public and native verification

Public offline/manual 1/7/64 outputs agree with original captures at all three
rates, including nonzero seeks. A 127-frame loop agrees with continuous repeated
source; pause/resume after eight wraps retains phase/filter history (comparison
excludes the documented output ramp). Independent owners, bypass, nonlinear
stack order, cancel/one-step commit/undo and unchanged source PCM pass.
Instrumented callbacks remain finite and allocation/free counts are zero during
live delay/depth/stereo/speed/cutoff/mix, waveform and band changes. Direct,
headless and MCP samples agree. Natural tails, silence and high-rate wrap are
checked at the shared renderer seam.

Actual 920x630 [default](chorus-default.png) and
[Sharp sines](chorus-sharp-sines.png) native editor captures were inspected.
CHORUS/CROSSOVER occupy the first row; LFO 1/2/3 the second. All knobs, selectors,
raw fields, derived units and reset access are readable. Opening twice keeps one
window; focus/close/missing-window capture and changed processing identity pass.
[Capture metadata](chorus-captures.json) records actual native pixels. Hidden
editors do not drive phases or processing.

The user approved automated acceptance. PCM, state and pixels were measured;
physical listening/device checks remain unverified. Ticket 44 owns final
actual-effect performance and integrated project/bounce/export acceptance.

Reproduction with repository Rust environment and local TMP/TEMP:

- `cargo test --offline --lib dsp::chorus -- --nocapture`
- `cargo test --offline --test chorus`
- `cargo test --offline --test transport chorus`
- `cargo test --offline --test workflow chorus`
- `cargo test --offline --test gui_attachment native_chorus -- --ignored --nocapture`

Validation: `cargo test --offline --all-targets` passes **285 tests**;
`cargo test --offline --no-default-features --all-targets` passes **164 tests**.
Both all-target Clippy configurations pass with `-D warnings`, and
`cargo fmt --all --check` passes. The final explicit native capture test passes
in 4.87 s. Logs: `.tools/test35-{development,kernel,public,pause,native-final,all,
nogui-final}.log` and `.tools/clippy35-{final,nogui}.log`. An earlier concurrent
GUI-free link collided with a headless workflow executable still in use on
Windows; the sequential complete rerun above passes without source changes.
