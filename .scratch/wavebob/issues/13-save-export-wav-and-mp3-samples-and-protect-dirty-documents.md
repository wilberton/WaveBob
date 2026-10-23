# 13: Save/export WAV and MP3 samples and protect dirty documents

**What to build:** Save or export edited samples or selected ranges to WAV
and MP3, track dirty state, and close documents without accidentally discarding
unsaved work. MP3 export is a version 1 requirement.

**Blocked by:** 06 — WAV and MP3 import with scalable waveform jobs; 11 — Cut, copy, paste, delete and trim samples.

**Status:** ready-for-agent

- [ ] WAV Save preserves supported source encoding; Save As/export offers WAV and
  MP3 with a 24-bit WAV default for new exports and defined overload handling.
- [ ] MP3 export offers CBR 128/192/256/320 kbit/s (default 192), preserves mono/stereo
  layout, exposes 32/44.1/48 kHz output and uses the shared resampler when needed.
  Include delay/padding metadata and report lossy conversion and output settings.
- [ ] Select/document the encoder, reproducible native build inputs and distribution
  requirements; shipped GUI/headless builds encode without a user-installed
  codec executable. The core command path is shared by GUI, headless and MCP.
- [ ] Saving over an MP3 requires explicit lossy encoding/overwrite intent; lossless
  WAV Save As remains available. Do not replace lossless working audio with a
  re-decoded MP3 result or silently change another revision's saved state.
- [ ] Output commits atomically with progress/cancellation and explicit overwrite
  intent. A completed save marks only its exact revision as saved.
- [ ] GUI close/quit supports save/discard/cancel; headless/MCP commands accept an
  explicit dirty-work policy without hidden prompts.
- [ ] Round-trip fixtures check frames/encoding and quantization tolerance; failed
  writes, stale saves and cancellation preserve source and session state.
- [ ] For MP3, use independent decode, delay/padding-aware alignment/duration and
  documented signal-error tolerances rather than PCM equality. Cover every
  offered bitrate/rate, selected-range output and reimport of edited exports.

## Specification

[WaveBob editor specification](../spec.md).
