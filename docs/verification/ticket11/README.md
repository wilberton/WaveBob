# Ticket 11 verification

24 October 2026, Windows release app. MCP copies a one-second stereo interval at
48 kHz, deletes all audio from a 44.1 kHz mono destination, captures the
[empty sample](empty-sample.png), pastes with rate/downmix conversion, then
captures [the 44100-frame result](cross-rate-paste.png).
[Metadata](structural.json) records document and capture revisions. Both captures
were inspected for truthful state, disabled empty-sample controls, restored
waveforms, status readouts, side-panel handles and the mockup shell.

Six command tests cover first/last/whole deletion, cut, trim, insertion/replacement,
one-step undo/redo, cursor/selection remapping, unchanged files, empty clipboard,
cancelled cut atomicity, clipboard lifetime after close/source removal, stereo
duplication/downmix and exact resampled duration. Analytic sine tests measure
passband preservation and downsampling alias suppression. GUI tests drive audio
clipboard shortcuts with no OS text dependency, delete-all/paste/undo and viewport
recovery; a separate input test verifies platform text clipboard delegation.

74 ordinary GUI-enabled tests and 51 GUI-free tests pass. Both Clippy variants
pass with warnings denied; formatting, release build and the native MCP workflow
pass. macOS/Linux native checks remain deferred as requested.
