# Ticket 16 verification

Windows, 24 October 2026.

- 107 ordinary GUI-enabled tests and 73 GUI-free tests pass. Both Clippy variants
  deny warnings; formatting passes.
- Deterministic fixtures compare live mixed frames exactly with offline inspection,
  including mono/stereo, mixed rates, positions, silent gaps and clip DSP.
  Mute-over-solo, equal-power mono pan, stereo balance, dB/master gain,
  overload reporting, selection loops beyond audible content, pause and rewind
  are checked. Cancelling pending play cannot start audio after preparation.
- Callback instrumentation reports zero allocations and zero frees while adopting
  a replacement mix graph and converging through 200 queued seeks.
- Actual egui inputs exercise multitrack Space, Enter and slash, track mute,
  timeline cursor updates and the master meter.
- Native release-app MCP imports 24 kHz mono WAV and 44.1 kHz stereo MP3 onto
  separate tracks, prepares and inspects mixed PCM, starts native output, observes
  nonzero callback PCM within the timeline loop, retrieves an actual playing-app
  PNG, and pauses through an acknowledged output handoff. `mix.json` retains
  device/position/peak evidence. No subjective listening claim is made.
- `mix.png` was inspected against the mockup shell: timeline, colored clips,
  mute/solo, gain/pan, centered transport, output meter, selection status and handles.

Native macOS/Linux verification remains deferred by the user until a remote exists.

A fractional device-rate regression also verifies interpolation across the end-to-start
loop boundary. The next interpolation frame wraps before end clamping, avoiding
a held final sample at the wrap when device and source rates differ.
