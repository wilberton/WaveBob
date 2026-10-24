# Ticket 10 verification

24 October 2026, Windows. The native MCP test opens a generated three-second
stereo texture, selects frames 24000–120000, applies a linear fade out, undoes and
redoes it, then retrieves [the rendered waveform](effects.png) with
[document/history/capture metadata](effects.json). The capture was inspected
against the mockup shell and for the selected interval, fade, dirty marker,
enabled effects/history controls and persistent status readout.

Five public-command effects tests cover exact gain/fade/reverse PCM across stereo
channels, unchanged outside frames, one-frame fade no-ops, invalid/stale targets,
cancellation, overload retention/reporting, source-file preservation and redo
invalidation. Two concurrent edits to the same base revision yield one success
and one stale-result error. A manual-output check verifies safe revision handoff
while playing and after undo. The egui test chooses an effect, applies it once,
checks the refreshed waveform, clicks Undo and uses the redo shortcut.

The adapter contract applies gain to an imported MP3 through direct commands,
the actual headless process and stdio MCP and compares every returned PCM value.
66 ordinary GUI-enabled tests and 45 GUI-free tests pass. Clippy passes with
warnings denied in both configurations; formatting and release builds pass.
The native effect/undo/redo/capture test passes against release binaries.
macOS/Linux native checks remain deferred as requested.
