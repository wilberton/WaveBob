# 15: Trim and process an individual arrangement clip

**What to build:** Trim a clip's source interval and use the right panel to
apply gain/fades/reverse/reverb to that clip without altering other instances.

**Blocked by:** 12 — Apply bounded reverb to the editing target; 14 — Arrange document clips on editable tracks.

**Status:** done

- [x] Clip-edge and exact-range commands validate source bounds and preserve pinned
  revisions; committed operations participate in shared chronological history.
- [x] The right panel displays selected-clip targeting; no/multiple selected clips
  disable Apply with status-bar explanation in the first release.
- [x] Effects use the same DSP as waveform edits through instance-specific processing;
  another clip of the same source and the open document remain unchanged.
- [x] Tests inspect resulting clip audio/metadata, trim limits and undo/redo. Once
  mixing is available, playback/export consume the same clip processing result.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

100 GUI-enabled and 67 GUI-free tests pass, both Clippy variants and formatting.
Native MCP checks instance isolation, trimming and actual screenshot retrieval.
See [the verification record](../../../docs/verification/ticket15/README.md).
