# Ticket 17 verification

Windows, 24 October 2026.

- 112 ordinary GUI-enabled tests and 77 GUI-free tests pass; both Clippy variants
  deny warnings and formatting passes.
- Exact mixer fixtures check staggered, contained, shared-start/shared-end,
  coincident insertion ordering and one/two-frame intersections. Endpoint gains
  and the center of odd containment intervals are exact. No NaNs are generated.
- Live output agrees with offline samples. Moving, trimming and undo/redo update
  regions and audible output; source PCM remains immutable. Third simultaneous
  overlaps preserve state/history on rejection; adjacent regions remain valid.
- Callback instrumentation reports no allocation or destruction while evaluating
  envelopes and adopting replacement graphs. Hard pan endpoints are exact zero.
- Actual egui gestures verify the overlay disappears after moving a clip and
  third-overlap rejection is visible in persistent status help/error treatment.
- Paused edits avoid unnecessary preparation. A seek accepted while a new graph
  prepares is retained until adoption, rather than replaced by old telemetry.
- Native release-app MCP creates a four-track arrangement, checks a mixed frame
  against independently calculated equal-power gains and rejects a third overlap.
  It retrieves `crossfade.png`; `crossfade.json` retains audio/state evidence.
  The PNG was inspected against the mockup, including the X curves and a caption
  separated from clip names, track colors, controls, panel handles and status.

See [the documented discrete overlap rules](../../crossfades.md). Native macOS/Linux
verification remains deferred by the user until a remote exists.
