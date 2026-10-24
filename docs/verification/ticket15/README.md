# Ticket 15 verification

Windows, 24 October 2026.

- 100 ordinary GUI-enabled tests and 67 GUI-free tests pass. Both Clippy variants
  deny warnings; formatting passes.
- Public command tests compare all five clip effects with waveform DSP, inspect
  exact processed samples, and verify pinned-source/other-instance isolation.
  Trim bounds, timeline position/duration rounding, shared undo/redo, stale
  identities, cancelled publication and unity no-op history are covered.
- Actual egui inputs trim the right clip edge, apply an effect to the selected
  instance and verify that no selection disables Apply. Track drag cancellation
  and the existing waveform/close/export regressions still pass.
- Native release app driven through MCP creates two shared-source instances,
  processes and trims the first, proves the document/second instance remain
  unchanged, selects the clip and retrieves its actual PNG. Metadata is in
  `clip-effects.json`; `clip-effects.png` was inspected against the mockup shell,
  including selected-clip targeting, exact source controls, status and panel handles.

Prepared clip audio is the common input reserved for mixer playback/export in
the next slices. Native macOS/Linux checks remain deferred until a remote exists.
