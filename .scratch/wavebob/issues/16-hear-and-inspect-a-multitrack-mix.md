# 16: Hear and inspect a multitrack mix

**What to build:** Play an arrangement with track mute/solo, volume and pan,
mixed-rate source clips, timeline looping and visible output levels.

**Blocked by:** 06 — WAV and MP3 import with scalable waveform jobs; 08 — Waveform playback, seeking and loop transport; 11 — Cut, copy, paste, delete and trim samples; 14 — Arrange document clips on editable tracks.

**Status:** ready-for-agent

- [ ] GUI track controls and commands implement the specified mute-over-solo rules,
  dB gain, mono pan/stereo balance, and queryable effective track state.
- [ ] Reuse the shared resampling capability for sources at different rates;
  timing follows the fixed project clock without rewriting source revisions.
- [ ] Live playback and an internal offline-render query use one prepared mixer,
  respecting source ranges, silence gaps and any available clip processing.
- [ ] Fixtures verify mixing, positions, mute/solo combinations, pan, loop bounds,
  master overload indication and audio-block handoff during control changes.

## Specification

[WaveBob editor specification](../spec.md).
