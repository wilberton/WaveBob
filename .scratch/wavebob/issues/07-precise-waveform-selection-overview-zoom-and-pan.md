# 07: Precise waveform selection, overview zoom and pan

**What to build:** Set a cursor or time selection, navigate the main waveform
with mouse/trackpad or overview handles, and inspect exact selection timing.

**Blocked by:** 22 — Refine the UI to match the accepted mockups.

**Status:** done

- [x] Click, bidirectional drag, select all, clear and exact-frame commands have
  shared validated semantics across channels; the status bar shows real values.
- [x] Overview-body dragging preserves viewport duration; edge dragging anchors
  the opposite edge. Main ruler, channels and overview remain synchronized.
- [x] Scroll/pinch/middle-drag and fit/zoom actions follow the proposed gesture
  mapping, anchor zoom correctly and clamp at document bounds.
- [x] Selection survives navigation. Tests cover one-frame/whole-sample bounds,
  reversed drags, out-of-range commands and input focus behavior.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

[Verification and inspected MCP captures](../../../docs/verification/ticket07/README.md).
All 46 ordinary GUI-enabled tests and 29 GUI-free tests pass, both Clippy
configurations pass with warnings denied, and release binaries build. Eleven
egui interaction tests include selection/overview/wheel/pinch/middle-drag,
keyboard/focus, panel controls and persistent status help. The native MCP
selection/view/sample-detail check also passes against the release app.
