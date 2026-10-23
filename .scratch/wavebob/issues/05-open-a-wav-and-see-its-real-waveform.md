# 05: Open a WAV and see its real waveform

**What to build:** Open a mono or stereo 16-bit PCM WAV through a command or
GUI Open action, list its document, and view real overview/channel waveforms.

**Blocked by:** 01 — Command-driven session and headless runner.

**Status:** ready-for-agent

- [ ] Import produces stable document/revision identifiers and accurate metadata;
  file reads and decoding run off the UI thread with observable job completion.
- [ ] Opening multiple files and choosing an open document displays the right audio
  in waveform mode. The empty view and failed import leave the session usable.
- [ ] Waveforms reflect fixture samples and bounds rather than synthetic bars;
  source files remain unchanged.
- [ ] Malformed, missing, inaccessible and unsupported files produce structured
  errors. Command tests verify exact decoded frames and document switching.

## Specification

[WaveBob editor specification](../spec.md).
