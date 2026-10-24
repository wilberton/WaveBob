# 26: Drag the crossfade endpoint

**What to build:** Add a visible drag handle at the right endpoint of a multitrack
crossfade, trimming the clip that ends there through the existing trim command.
For containment, trim the contained clip; coincident pairs use insertion order.

**Blocked by:** 15 — Trim and process an individual arrangement clip; 17 — Automatic crossfades on overlapping clips.

**Status:** done

- [x] The endpoint handle takes precedence over overlapping clip bodies and shows
  a horizontal resize cursor and status help.
- [x] Dragging shortens or extends the ending clip within source bounds, leaving
  the other clip fixed; release commits one undoable edit and Escape cancels.
- [x] Focused GUI tests verify overlap geometry, bounds, cancellation and history.

## Specification

[Editor spec](../spec.md). Directly requested on 24 October 2026.

## Verification

All 39 GUI binary tests pass, including endpoint hover, covered-clip trimming,
source-bound extension, unchanged neighboring clip, derived overlap end,
one history entry, undo/redo and Escape cancellation. Formatting and
`git diff --check` pass. The separate mixer integration test build was blocked
by Windows denying replacement of an existing locked `wavebob-headless.exe`.
