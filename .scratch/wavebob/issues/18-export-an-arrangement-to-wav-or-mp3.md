# 18: Export an arrangement to WAV or MP3

**What to build:** Render an arrangement or its selected time range into a
WAV or MP3 file from the GUI, headless runner or MCP without an audio device.

**Blocked by:** 13 — Save/export WAV and MP3 samples and protect dirty documents; 15 — Trim and process an individual arrangement clip; 17 — Automatic crossfades on overlapping clips.

**Status:** done

- [x] Export includes resampling, clip processing, crossfades, track controls and
  silent gaps according to exactly the live mix semantics.
- [x] Range, output rate/encoding, destination and overwrite intent are explicit;
  frame count and tail/end handling are deterministic and visible.
- [x] Jobs are cancellable and atomically commit output; a captured immutable
  arrangement snapshot prevents concurrent edits changing a render mid-job.
- [x] Exported PCM matches mixer fixtures within encoding tolerance, including
  mixed rates and overlap/solo/pan combinations, in a device-free session.
- [x] Reuse the sample-export MP3 encoder/options, resampling and padding metadata;
  independently decoded MP3 mixes match alignment/duration/channel expectations
  and codec-appropriate signal tolerances. No device or external executable is
  required for MP3 mix export on Windows, macOS or Linux.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

117 GUI-enabled and 81 GUI-free tests pass, both Clippy variants and formatting.
Direct/headless/MCP output agrees. Independent MP3 mix decode, actual GUI export,
native MCP output/reimport and screenshot retrieval pass. See
[the verification record](../../../docs/verification/ticket18/README.md).
