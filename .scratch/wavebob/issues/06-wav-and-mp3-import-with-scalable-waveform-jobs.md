# 06: WAV and MP3 import with scalable waveform jobs

**What to build:** Open the specified integer/float WAV formats and CBR/VBR
MP3 files, and navigate large samples while decoding and waveform preparation
report progress. MP3 import is a version 1 requirement.

**Blocked by:** 22 — Refine the UI to match the accepted mockups.

**Status:** ready-for-agent

- [ ] Mono/stereo PCM 16/24/32-bit and float32 WAV fixtures decode with accurate
  metadata, levels and frame counts. Unsupported channel/format cases are clear.
- [ ] Mono/stereo CBR/VBR MP3 imports produce editable floating-point revisions;
  source encoding, decoded frame count/rate and available bitrate mode are
  queryable. Honor available delay/padding metadata without guessing silence
  removal when tags are absent. Corrupt/truncated streams fail cleanly.
- [ ] Select/document a decoder and reproducible cross-platform build inputs. MP3
  decoding works in shipped desktop and GUI-free builds without a separately
  installed codec executable. Use an independent decoder to validate fixtures.
- [ ] Multiresolution peaks are cached by immutable revision; close/cancel/reopen
  does not publish stale peaks into another document or revision.
- [ ] Jobs support progress, cancellation and failure without partial documents;
  sample-level display uses real frames when zoomed sufficiently.
- [ ] A measured large-file check establishes memory and UI-response baselines and
  proves the UI can still navigate/cancel during background preparation.

## Specification

[WaveBob editor specification](../spec.md).
