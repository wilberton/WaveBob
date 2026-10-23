# 09: Filesystem browsing and independent audition

**What to build:** Navigate folders in the left browser, audition a file
without opening it, and double-click to open it for editing.

**Blocked by:** 06 — WAV and MP3 import with scalable waveform jobs; 08 — Waveform playback, seeking and loop transport.

**Status:** ready-for-agent

- [ ] Parent/folder navigation and supported-file listing are asynchronous and
  handle inaccessible or removed directories and native paths on all platforms.
- [ ] Explicit audition controls use a single preview voice; starting preview pauses
  editor playback, and editor playback stops preview without automatic resumption.
- [ ] Browser selection exposes preview state without creating a document;
  double-click opens the intended file without a delayed audition race.
- [ ] Corresponding browse/audition commands report state and errors. Tests verify
  document counts, double-click handling, preview cancellation and hover help.
- [ ] WAV and MP3 files are discoverable, auditionable and openable through the
  browser. Unsupported files are distinguished from broken supported files.

## Specification

[WaveBob editor specification](../spec.md).
