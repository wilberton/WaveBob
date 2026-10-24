# 24: Render continuous waveforms at intermediate zoom

**What to build:** Fix the disappearing/dotted waveform strokes visible in the
user's screenshot at intermediate zoom, retaining truthful sample positions,
min/max envelopes, stereo alignment and the accepted styling.

**Blocked by:** 07 — Precise waveform selection, overview zoom and pan.

**Status:** done

- [x] A regression reproduces missing strokes in valid flat/subpixel peak bins.
- [x] Peak envelopes remain visible and continuous at intermediate zoom and high
  DPI; individual-sample rendering and clipping remain accurate.
- [x] Inspect renderer-backed captures at whole-sample, intermediate and sample
  zoom, in both channels, and retain a focused regression test.

## Verification

Implementation `783f6cc`. The old two-frame flat-bin trace failed while the
single-frame trace passed; both now pass. Actual stereo renderer/MCP captures
were inspected at three zoom levels and 100%/150% scale. See the
[evidence and reproduction steps](../../../docs/verification/tickets23-24/README.md).

## Specification

[Editor spec](../spec.md). Directly requested and approved on 24 October 2026.
