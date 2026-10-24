# Parametric EQ verification plan

Pinned reference: Lemon-Studio 0af860dfd31c7ee9cbf8af6747617e031a1a7742,
`effects/eq/{design,equalizer}.rs` and the parametric editor. Implement only
ordinary minimum-phase processing. Seven bands expose stored integer level,
frequency position, width, type, order and enable; main level is hundredths of dB.
The exact source frequency/default/width/steep-section tables govern behavior.
Omit HQ, Enhanced and linear-phase choices.

Predeclare max PCM error 2e-6 and relative RMS error 2e-5 above energy 1e-12,
without gain correction or frame alignment after declared latency compensation.
Response magnitude tolerance is 2e-5 dB above -120 dB. Capture impulses, signed
stereo/multitone, silence, all shapes/orders, gain/width/frequency extremes,
disabled bands, global gain and a held-out mixed seven-band 96 kHz case. Check
44.1/48/96 kHz and public manual blocks 1/7/64.

The reference gate/countdown/denormal cleanup is block-dependent. WaveBob uses
a fixed 64-frame internal quantum to preserve that exact reference behavior
independently of host block size. Its 64-frame computational staging delay is
reported and compensated by the shared renderer; intentional filter phase is
preserved. Reset follows reference cold/prerolled plain state. Continuous same-
shape coefficients and main gain slew over 10 ms, preserving prepared state;
shape/order/mask topology changes prepare and crossfade over 20 ms. Live changes
are an explicit adaptation and do not redefine fixed-setting agreement.

Verify offline/manual/headless/MCP, reset/seek/loop, owner isolation, order/bypass,
preview cancel/one-step history, callback allocations/frees, finite stable
transitions, tail threshold/hold/cap metadata and mono/stereo policy. Inspect a
real native graph/editor capture, with seven draggable bands and exact values.
Compute spectrum on the visible UI from bounded transport snapshots, never in
audio processing. Physical listening/device checks remain unverified under the
user's approved automated acceptance.
