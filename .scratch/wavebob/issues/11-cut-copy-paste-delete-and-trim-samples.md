# 11: Cut, copy, paste, delete and trim samples

**What to build:** Perform structural waveform edits using commands or GUI
actions, with predictable paste conversion and full undo/redo.

**Blocked by:** 10 — Undoable gain, fades and reverse on the editing target.

**Status:** done

- [x] Cut/copy/delete/trim have defined nonempty-selection behavior; paste inserts
  at the cursor or replaces an active selection using an internal audio clipboard.
- [x] Cross-rate paste uses a deterministic shared resampling capability; mono/stereo
  duplication/downmix rules are explicit and verified with fixtures.
- [x] Length-changing edits remap/clamp cursor, selection and viewport; peak caches
  and playback reflect the committed revision.
- [x] Tests cover first/last-frame edits, empty clipboard, whole-document deletion,
  frame counts, unchanged source audio and one-step undo/redo of each edit.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

[Command, resampling, keyboard and inspected native MCP checks](../../../docs/verification/ticket11/README.md).
74 ordinary GUI-enabled tests and 51 GUI-free tests pass, both Clippy variants
pass with warnings denied, and release/native MCP verification passes.
