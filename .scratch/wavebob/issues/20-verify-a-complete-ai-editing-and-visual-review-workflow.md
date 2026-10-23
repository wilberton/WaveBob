# 20: Verify a complete AI editing and visual-review workflow

**What to build:** A repeatable human/AI workflow imports audio, edits and
arranges it, exports it, reopens the project and captures reviewable UI images.

**Blocked by:** 03 — Drive sessions and inspect captures through MCP; 09 — Filesystem browsing and independent audition; 18 — Export an arrangement to WAV or MP3; 19 — Save and reopen portable editing projects.

**Status:** ready-for-agent

- [ ] One scripted scenario covers import, exact selection, effects, undo/redo,
  clipboard editing, two overlapping clips, mix export and project reopen.
- [ ] Include WAV and MP3 sources, browser audition, MP3 sample and mix exports,
  independent reimport validation and device-free headless MP3 operation.
- [ ] Direct/headless/MCP scenarios agree on public state and audio. Attached GUI
  captures correspond to the edited revision, not another session or stale frame.
- [ ] Inspect actual screenshots of both modes, status hover/focus help, panels
  hidden/restored, narrow layout and high DPI; verify no floating help popups.
- [ ] Compare representative captures to the accepted WaveBob mockups for layout,
  spacing, typography, colors and controls, allowing the requested status bar
  and panel arrows. The mockups are the visual acceptance reference.
- [ ] Exercise invalid/stale requests, cancellation, missing assets, device failure,
  capture without a renderer and large-file responsiveness. Record results and
  platform limitations without replacing audio correctness tests with images.

## Specification

[WaveBob editor specification](../spec.md).
