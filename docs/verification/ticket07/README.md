# Ticket 07 verification

Verified on Windows, 24 October 2026. Shared command tests cover cursor/selection,
reversed and empty intervals, one-frame/all-sample bounds, no-op behavior,
out-of-range and stale requests, and unchanged audio revisions/dirty state.
Viewport tests verify independent revisions, anchored zoom math, clamped pan,
one-frame minimum and cleanup on close.

Actual egui pointer/key tests cover click versus drag, uncommitted previews,
reversed drag, Escape cancellation, overview body and both edges, Ctrl+wheel,
pinch anchoring, middle pan, ordinary scrolling, fit/zoom buttons, selection
preservation, text-focus guards and exact narrow status-bar/help readouts.
Existing resize/collapse/backtick and persistent-help tests remain passing.

The native MCP test imports a real stereo WAV, selects frames 144000–240000 and
sets exact visible intervals. It verifies that frame-detail extrema equal exact
sample queries, captures the running renderer, checks revision/resource metadata
and compares delivered PNG bytes with saved files. Source bytes remain unchanged,
and mode switching preserves selection and viewport state.

- [Zoomed selection](waveform-zoomed.png): absolute 2–8 second ruler, overview
  bounds and a 3–5 second selection across both channels.
- [Individual samples](waveform-samples.png): actual connected points in a
  25-frame interval, with the same cursor/selection.
- [One frame](waveform-single-frame.png): minimum viewport, held sample levels
  and microsecond ruler detail.
- [Capture and state metadata](selection-metadata.json).

All representative images were visually inspected against the accepted mockup
style. The real selection, overview box, channel colors, cursor and status bar
now reflect live state. The independent selection and viewport examples are in
[waveform controls](../../waveform.md). Native macOS/Linux verification remains
deferred until a remote exists.

Reproduce: run the ignored
`exact_selection_zoom_and_sample_detail_capture_through_mcp` test in `tests/mcp.rs`
with `WAVEBOB_TEST_ARTIFACT_DIR` set to the desired output directory.
