# 14: Arrange document clips on editable tracks

**What to build:** Create/reorder/name tracks, drag an open document into the
timeline, and move or remove clips with undoable command-driven placement.

**Blocked by:** 22 — Refine the UI to match the accepted mockups; 10 — Undoable gain, fades and reverse on the editing target.

**Status:** done

- [x] Track and clip IDs, exact positions/source ranges and pinned source revision
  are queryable; later waveform edits leave existing clips unchanged.
- [x] Drop previews and clip drags commit once on release, support another track,
  clamp/reject negative time, and cancel safely on invalid/off-track drops.
- [x] A seconds ruler, timeline navigation, cursor/time selection and selected-clip
  count work with command equivalents. Deleting a track removes its clips in
  one undoable operation.
- [x] Command and gesture tests verify placement, drag cancellation and history.
  Until the crossfade slice lands, overlaps are explicitly unavailable rather
  than appearing to implement the final transition behavior.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

96 GUI-enabled and 64 GUI-free tests pass, both Clippy variants and formatting.
Native MCP placement, pinned-source lifetime and framebuffer capture pass; see
[the verification record](../../../docs/verification/ticket14/README.md).
