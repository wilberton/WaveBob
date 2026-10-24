# Ticket 14 verification

Windows, 24 October 2026. The synthetic arrangement demo has been removed.

- 96 ordinary GUI-enabled and 64 GUI-free tests pass; both Clippy variants and
  formatting pass. Focused command and egui gesture tests cover placement,
  cross-track movement, off-track/Escape cancellation, single history commits,
  negative/overflow/stale rejection, chronological sample/arrangement history,
  exact navigation and pinned source lifetime after edits/document closure.
- Native release-app MCP creates four tracks and real mono/stereo/mixed-rate
  clips, closes a source document, verifies unchanged pinned peaks, selects time
  and a clip, switches mode/viewport and retrieves the actual framebuffer PNG.
- `arrangement.png` was visually inspected against the accepted mockup: teal,
  blue and gold clips, compact track headers, mute/solo/gain/pan controls, dark
  grid/ruler, side panels with chevrons and persistent selection status.
- Overlaps are explicitly rejected until ticket 17. Audible mix playback waits
  for ticket 16; project saving waits for ticket 19. Dirty arrangement quit already
  requires explicit discard or cancellation.

macOS/Linux native verification remains deferred by the user until a remote exists.
