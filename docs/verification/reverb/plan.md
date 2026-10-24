# Reverb reference plan

Target: Lemon-Studio 0af860dfd31c7ee9cbf8af6747617e031a1a7742,
`crates/os-engine/src/effects/reverb.rs`, with the source parameter/default and
INPUT/ROOM/DECAY/MIX editor layout. Source hashes are pinned in the tracker
reference manifest. The isolated renderer compiles that original source unchanged;
production uses independently structured WaveBob code and no engine/plugin SDK.

The documented structure is sum/side input, high/low cuts, four input allpasses,
intentional predelay, two cross-fed shelf/damping/modulated tank halves, seven wet
taps per side, separate early taps, and dry/early/wet output matrices. Phase starts
at zero; the modulation is deterministic without an RNG. Predelay is intentional
sound, with zero computational latency. Mono is promoted to dual mono and the
processed result retains stereo. Seek/reset begins from the deterministic initial
state; loop wraps carry state. Live continuous coefficients ramp over 10 ms;
room/predelay or routing changes prepare a new path and crossfade over 20 ms.

Predeclared fixed-setting comparison limits: maximum absolute sample error 2e-6,
relative RMS error 2e-5 when reference energy exceeds 1e-12, zero gain correction
and zero frame alignment. Capture raw f32 stereo; preserve reference control
rounding. Impulses in each channel, signed stereo bursts and silence cover defaults,
filter off/extremes, free/synced predelay (including six seconds at 20 BPM), rooms
1/100, diffusion, modulation, separation, dry/early/wet paths and long bass decay.
Rates are 44.1/48/96 kHz; blocks 1/7/64 must agree. Hold out a 96 kHz mixed-control
case until the baseline passes development cases. Compare complete captures,
arrival timing, channel energy, tail windows and finite output, rather than gain-
corrected similarity alone.

Public/manual/offline tests additionally cover reset/seek/loop, prepared live room
changes, state-preserving wet changes, cancellation/undo, owner isolation, bypass/
reorder and callback allocation/free checks. Tail termination follows the renderer's
-100 dBFS/100 ms hold and 60 s cap/taper; retained delay energy conservatively
prevents stopping before a delayed arrival. Exact silence is suspended at output
while deterministic phase advances; reference subaudible bias remains below the
comparison tolerance. Actual native editor captures must be inspected. Automated
acceptance is approved; physical listening/device checks remain unverified.
