# 20: Verify a complete AI editing and visual-review workflow

**What to build:** A repeatable human/AI workflow imports audio, edits and
arranges it, exports it, reopens the project and captures reviewable UI images.

**Blocked by:** 03 — Drive sessions and inspect captures through MCP; 09 — Filesystem browsing and independent audition; 18 — Export an arrangement to WAV or MP3; 19 — Save and reopen portable editing projects.

**Status:** done

- [x] One scripted scenario covers import, exact selection, effects, undo/redo,
  clipboard editing, two overlapping clips, mix export and project reopen.
- [x] Include WAV and MP3 sources, browser audition, MP3 sample and mix exports,
  independent reimport validation and device-free headless MP3 operation.
- [x] Direct/headless/MCP scenarios agree on public state and audio. Attached GUI
  captures correspond to the edited revision, not another session or stale frame.
- [x] Inspect actual screenshots of both modes, status hover/focus help, panels
  hidden/restored, narrow layout and high DPI; verify no floating help popups.
- [x] Compare representative captures to the accepted WaveBob mockups for layout,
  spacing, typography, colors and controls, allowing the requested status bar
  and panel arrows. The mockups are the visual acceptance reference.
- [x] Exercise invalid/stale requests, cancellation, missing assets, device failure,
  capture without a renderer and large-file responsiveness. Record results and
  platform limitations without replacing audio correctness tests with images.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

The [complete record](../../../docs/verification/ticket20/README.md) links normal,
narrow, high-DPI, hidden-panel and deterministic native-renderer help captures,
independent MP3 decoding and the ten-minute import/edit/export benchmark.
86 GUI-free and 124 GUI-enabled production tests pass; both Clippy configurations
and formatting pass. The shared scenario agrees across direct, NDJSON and MCP
routes on public results, PCM and exported bytes. Native optimized GUI/MCP runs
also exercise browser audition and project reopen after deleting source files.
Visual inspection refined sliders, empty-track hints and overlapping clip titles.
Mac/Linux and physical-device platform limits are explicitly recorded under the
user's existing remote/CI deferral.
