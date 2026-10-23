# 06: WAV and MP3 import with scalable waveform jobs

**What to build:** Open the specified integer/float WAV formats and CBR/VBR
MP3 files, and navigate large samples while decoding and waveform preparation
report progress. MP3 import is a version 1 requirement.

**Blocked by:** 22 — Refine the UI to match the accepted mockups.

**Status:** done

- [x] Mono/stereo PCM 16/24/32-bit and float32 WAV fixtures decode with accurate
  metadata, levels and frame counts. Unsupported channel/format cases are clear.
- [x] Mono/stereo CBR/VBR MP3 imports produce editable floating-point revisions;
  source encoding, decoded frame count/rate and available bitrate mode are
  queryable. Honor available delay/padding metadata without guessing silence
  removal when tags are absent. Corrupt/truncated streams fail cleanly.
- [x] Select/document a decoder and reproducible cross-platform build inputs. MP3
  decoding works in shipped desktop and GUI-free builds without a separately
  installed codec executable. Use an independent decoder to validate fixtures.
- [x] Multiresolution peaks are cached by immutable revision; close/cancel/reopen
  does not publish stale peaks into another document or revision.
- [x] Jobs support progress, cancellation and failure without partial documents;
  sample-level display uses real frames when zoomed sufficiently.
- [x] A measured large-file check establishes memory and UI-response baselines and
  proves the UI can still navigate/cancel during background preparation.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

[Verification record](../../../docs/verification/ticket06/README.md): 39 ordinary
GUI-enabled tests and 28 GUI-free tests pass, both Clippy configurations pass with
warnings denied, and formatting is checked. The release GUI passed native MP3
import/capture and ten-minute stereo import/navigation/cancellation checks.
Sample-level queries and connected-point drawing are available; ticket 07 adds
interactive viewport gestures. Native macOS/Linux verification remains deferred.
