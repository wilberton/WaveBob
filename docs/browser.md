# Filesystem browser and audition

The left browser starts in the launch directory. Click its folder breadcrumb to
edit the path, then click Go; use the parent arrow or double-click a folder.
Single-click an audio file
to select it; the explicit preview play button auditions it. Double-click opens
it for editing. Unsupported file rows are dimmed and explain their type in the
status bar. A broken WAV/MP3 is a supported type whose decode error appears in
the preview area. Preview has independent volume and never creates a document.

`browser.navigate {path}` enumerates a folder asynchronously. `browser.get`
reports requested/current paths, parent, entries, loading, selected path,
skipped-path count, errors and audition state. A failed request retains the
previous listing. `browser.select {path}` selects a file from that listing.
`browser.open {path}` stops audition and returns the normal import job.
`audition.start {path}` asynchronously decodes WAV/MP3 and starts preview;
`audition.stop {}` cancels pending decoding or stops the preview.
`audition.volume.set {volume}` accepts a finite amplitude from zero to one.
Only starting audition requires a configured output device. Navigation and
opening also work through headless and MCP commands.

Editor and preview share one output voice. Starting audition pauses editor
playback immediately; starting editor playback cancels pending preview work and
replaces its source. Neither resumes automatically. Changing view/document also
stops audition. Cancelled or superseded decode completions cannot restart it.
The preview has its own source UUID, exposed in audition state and transport
telemetry, without an entry in `session.get.documents`.

Enumeration and decoding run off the UI thread, bounded to two outstanding
workers each. Listings cap at 10000 entries and the GUI draws only visible rows.
Folders sort before files. Symlinks/junctions are followed for navigation;
inaccessible or non-Unicode child paths are counted as skipped. JSON paths use
Unicode strings. Renderer captures report pending listing and preview preparation.
