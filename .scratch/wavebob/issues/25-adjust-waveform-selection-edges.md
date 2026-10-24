# 25: Adjust waveform selection edges

**What to build:** Allow dragging either visible selection boundary in the main
waveform, holding the opposite boundary fixed and showing a resize cursor.
Crossing the fixed boundary normalizes the selection; Escape cancels the preview.

**Blocked by:** 07 — Precise waveform selection, overview zoom and pan.

**Status:** done

- [x] Both visible edges can be dragged with the opposite frame boundary fixed.
- [x] Edge hover shows a resize cursor; status help describes edge adjustment.
- [x] Verify gesture preview, release, crossing, document bounds and cancellation
  with focused egui tests, retaining existing cursor and selection gestures.

## Specification

[Editor spec](../spec.md). Directly requested on 24 October 2026.

## Verification

`cargo fmt --all --check` and `git diff --check` pass. All 37 GUI binary tests
pass, including the new edge-resize regression and existing selection,
overview and navigation tests. Tests use a workspace-local temporary directory
because the sandbox denies publishing exports in the system temporary directory.
