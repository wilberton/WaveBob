# Delay reference and delivery plan

Pinned Lemon-Studio 0af860dfd31c7ee9cbf8af6747617e031a1a7742,
`effects/delay.rs` and `plugins/lemon-delay/src/{params,view}.rs`.
Use the plugin START preset (true stereo, one beat at explicit 120 BPM, Saturate,
open low pass, no resonance/diffusion spread), with all 26 source ranges and
raw/display mappings. Initialize like the original engine slot: 8192 silent
frames and two resets. WaveBob reset restores that complete deterministic start,
including LFO/hold/envelope state; loops retain state. No tempo inference.

Predeclare maximum PCM error 2e-6 and relative RMS error 2e-5 above energy 1e-12,
zero alignment/gain correction. Original unchanged source runs in an isolated,
test-only reference crate, using the same platform's f32 sine implementation.
This matters for modulated fractional taps: platform sinf ulps must not be
mistaken for a different delay algorithm. Retained Windows captures document
provenance; native comparisons also run against the pinned source on each OS.

Cover mono/true-stereo/ping-pong/off, free/synced times including 12 seconds at
20 BPM, R/L offset and spread, keep-pitch/glide, feedback 0..125%, LP/HP/BP/off,
cutoff/resonance, limiter/saturator/knee/symmetry, sample/bit reduction, modulation,
diffusion and output tone/dry/wet at 44.1/48/96 kHz. Hold out a mixed-control
96 kHz stereo case until development passes. Compare full stereo captures,
arrival times and channel energies, plus public manual blocks 1/7/64.

Delay is intentional with zero computational latency. Mono promotes to stereo.
Power-of-two rings preallocate the model's maximum supported timing/modulation
range; unused model lines are small, and diffusion uses its true maximum range.
Range maxima over 64-sample blocks bound retained signal without scanning full
rings or retaining inaudible old samples for the entire allocation lifetime.
Check the 64 MiB chain budget before allocation; all storage is fallible.

Keep original stepped coefficient smoothing, time glide and keep-pitch crossfade
for compatible updates without clearing state. Model/filter/nonlinear-topology
changes prepare new paths and crossfade 20 ms. Tempo changes use prepared paths
without moving the playhead. Color-filter corners at/above Nyquist are bounded
below Nyquist to avoid unstable out-of-band designs; reference-rate cases below
Nyquist remain unchanged. Tail hold/cap/taper follows the shared renderer;
feedback capable of growth keeps low-level retained state active until drained
or capped at 60 seconds. Literal silence, mute and bypass acquire no tails.

Verify native-source agreement, public/manual/offline/headless/MCP, reset/seek/
loop and capped tails, owner/order/bypass/history/preview isolation, callback
allocations/frees and finite live transitions. Inspect actual native grouped
controls, distortion curve and exact project BPM entry. Physical listening and
device checks remain unverified under approved automated acceptance.
