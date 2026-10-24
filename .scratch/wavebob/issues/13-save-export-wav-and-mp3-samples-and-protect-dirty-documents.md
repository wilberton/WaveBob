# 13: Save/export WAV and MP3 samples and protect dirty documents

**What to build:** Save or export edited samples or selected ranges to WAV
and MP3, track dirty state, and close documents without accidentally discarding
unsaved work. MP3 export is a version 1 requirement.

**Blocked by:** 06 — WAV and MP3 import with scalable waveform jobs; 11 — Cut, copy, paste, delete and trim samples.

**Status:** done

- [x] WAV Save preserves supported source encoding; Save As/export offers WAV and
  MP3 with a 24-bit WAV default for new exports and defined overload handling.
- [x] MP3 export offers CBR 128/192/256/320 kbit/s (default 192), preserves mono/stereo
  layout, exposes 32/44.1/48 kHz output and uses the shared resampler when needed.
  Include delay/padding metadata and report lossy conversion and output settings.
- [x] Select/document the encoder, reproducible native build inputs and distribution
  requirements; shipped GUI/headless builds encode without a user-installed
  codec executable. The core command path is shared by GUI, headless and MCP.
- [x] Saving over an MP3 requires explicit lossy encoding/overwrite intent; lossless
  WAV Save As remains available. Do not replace lossless working audio with a
  re-decoded MP3 result or silently change another revision's saved state.
- [x] Output commits atomically with progress/cancellation and explicit overwrite
  intent. A completed save marks only its exact revision as saved.
- [x] GUI close/quit supports save/discard/cancel; headless/MCP commands accept an
  explicit dirty-work policy without hidden prompts.
- [x] Round-trip fixtures check frames/encoding and quantization tolerance; failed
  writes, stale saves and cancellation preserve source and session state.
- [x] For MP3, use independent decode, delay/padding-aware alignment/duration and
  documented signal-error tolerances rather than PCM equality. Cover every
  offered bitrate/rate, selected-range output and reimport of edited exports.

## Specification

[WaveBob editor specification](../spec.md).

## Progress

Core sample export/save is implemented with atomic worker publication, explicit
overload/overwrite policy and exact saved-revision tracking. All 24 MP3 settings
round-trip with exact gapless duration; independent FFmpeg selected-range checks
pass and are recorded in `docs/verification/ticket13/encoder-qa.json`.
GUI save/export and dirty close/quit flows are complete. Native MCP verification
saved an edited MP3 at 48 kHz, reimported its exact gapless duration, retrieved
the actual GUI PNG and received the guarded quit response before process exit.
See [encoder/build/license details](../../../docs/mp3-encoder.md).
The core slice passes 86 GUI-enabled tests and 61 GUI-free tests, both Clippy
variants and formatting. Large Windows catalogue responses now use 64 KiB pipe
buffers; the >64 KiB regression test completes within its 500 ms deadline.

Final validation: 93 GUI-enabled and 62 GUI-free tests pass, plus the native
MCP test and 24 independent encoder checks. GUI tests exercise save/discard/cancel,
lossless Save As for edited MP3, output settings and jobs completing during quit.
See [verification record](../../../docs/verification/ticket13/README.md).
Native macOS/Linux verification remains deferred by the user until a remote exists.
