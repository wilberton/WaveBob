# 24: Render continuous waveforms at intermediate zoom

**What to build:** Fix the disappearing/dotted waveform strokes visible in the
user's screenshot at intermediate zoom, retaining truthful sample positions,
min/max envelopes, stereo alignment and the accepted styling.

**Blocked by:** 07 — Precise waveform selection, overview zoom and pan.

**Status:** in-progress

- [ ] A regression reproduces missing strokes in valid flat/subpixel peak bins.
- [ ] Peak envelopes remain visible and continuous at intermediate zoom and high
  DPI; individual-sample rendering and clipping remain accurate.
- [ ] Inspect renderer-backed captures at whole-sample, intermediate and sample
  zoom, in both channels, and retain a focused regression test.

## Specification

[Editor spec](../spec.md). Directly requested and approved on 24 October 2026.
