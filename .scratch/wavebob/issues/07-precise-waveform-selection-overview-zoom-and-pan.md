# 07: Precise waveform selection, overview zoom and pan

**What to build:** Set a cursor or time selection, navigate the main waveform
with mouse/trackpad or overview handles, and inspect exact selection timing.

**Blocked by:** 22 — Refine the UI to match the accepted mockups.

**Status:** ready-for-agent

- [ ] Click, bidirectional drag, select all, clear and exact-frame commands have
  shared validated semantics across channels; the status bar shows real values.
- [ ] Overview-body dragging preserves viewport duration; edge dragging anchors
  the opposite edge. Main ruler, channels and overview remain synchronized.
- [ ] Scroll/pinch/middle-drag and fit/zoom actions follow the proposed gesture
  mapping, anchor zoom correctly and clamp at document bounds.
- [ ] Selection survives navigation. Tests cover one-frame/whole-sample bounds,
  reversed drags, out-of-range commands and input focus behavior.

## Specification

[WaveBob editor specification](../spec.md).
