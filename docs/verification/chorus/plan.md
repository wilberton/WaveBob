# Chorus reference plan

Pinned Lemon-Studio 0af860dfd31c7ee9cbf8af6747617e031a1a7742,
`effects/chorus.rs` and `plugins/lemon-small-fx/src/chorus.rs`.
Publish all twelve source integer controls/ranges/defaults and source display
laws, including >quarter crossover/wet switches and four shape regions.

Predeclare max PCM error 2e-6 and relative RMS 2e-5 above energy 1e-12, without
alignment/gain correction. Compare unchanged original natively, with the same
input and 64-frame reference blocks. Cover all shapes and speed/depth/cutoff/
stereo/delay extremes at 44.1/48/96 kHz, impulses/steps/independent multitone,
wet-only/band choices and silence. Reserve a mixed-control 96 kHz stereo case
until development comparisons pass. Retain complete reference captures/hashes.

Prepare the four 4096-entry integer LFO tables off callback and retain a direct
immutable pointer during audio. Fallibly preallocate two 4096-frame source rings;
retain the reference's modulo-wrap behavior at high rates. Fixed seed 0 gives
phases [0,1028,28213]. WaveBob reset returns to those phases/empty lines, rather
than drawing the original generator's next phases. A fixed internal 64-frame
quantum performs the source's tiny crossover-state cleanup independently of host
block size, without adding latency. Mono promotes to stereo, computational
latency is zero, residual crossover/delayed output drains through common tails.

Use exact range-max trees for bounded retained-state queries and no ring scans.
Compatible timing/depth/stereo/speed/filter coefficients/dry mix slew 10 ms,
preserving delay/filter/LFO state; waveform and crossover-band topology changes
prepare 20 ms paths. Native grouped CHORUS/CROSSOVER and LFO 1/2/3 controls expose
all raw values and derived units/reset; no editor-driven state changes.

Verify public offline/manual 1/7/64, pause/seek/loop, reset, tail/silence/bypass,
owner/order/history/preview isolation, callback allocations/frees and adapter
parity. Inspect actual native pixels. Physical listening/device checks remain
unverified under the user's approved automated acceptance.
