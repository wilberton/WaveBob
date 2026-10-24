# Ticket 09 verification

24 October 2026, Windows. [MCP screenshot](browser.png) and
[browser/preview metadata](browser.json) come from the running release app.
The test navigates to the owned audio fixture directory, selects a tagged MP3,
auditions its 8192 decoded frames without creating a document, then opens that
file and retrieves the actual framebuffer through MCP. The final capture was
inspected for the browser, selection, waveform, transport and status bar.

Command tests cover asynchronous listing, Unicode folders and native paths,
unsupported versus corrupt supported files, error recovery retaining the previous
listing, WAV/MP3 preview, independent volume, cancellation and exclusive handoff
with editor playback. An egui interaction test verifies that single-click selects
without audition/open, while double-click opens the intended audio document.

60 ordinary GUI-enabled tests and 40 GUI-free tests pass. Both Clippy variants
pass with warnings denied, formatting passes and release binaries build. Native
MCP preview/open/capture passes with the final browser layout. macOS/Linux native
checks remain deferred under the user's existing instruction.
