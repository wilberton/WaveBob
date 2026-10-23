# 22: Refine the UI to match the accepted mockups

**What to build:** After ticket 05, refine both editing modes to match the
accepted WaveBob mockups as closely as possible, using actual GUI captures to
compare and iterate before further editor functionality is added.

**Blocked by:** 02 — Attach to the GUI and capture its actual pixels; 04 — Resizable panels and persistent status-bar help; 05 — Open a WAV and see its real waveform.

**Status:** ready-for-agent

- [ ] Compare captures of both modes with the accepted mockups and record concrete
  differences in panel proportions, spacing, alignment, typography, colors,
  borders, controls, waveform treatment, timeline lanes and transport placement.
- [ ] Refine the UI through repeated capture/inspect/edit iterations to match the
  mockups as closely as egui permits. Preserve the requested status bar, hover/focus
  help, resizing, chevrons and backtick behavior as intentional additions.
- [ ] Use the real waveform import available after ticket 05. Arrangement visuals
  may use clearly identified demo data until track editing lands; visual refinement
  must not claim that unimplemented arrangement or audio operations work.
- [ ] Keep editing behind the shared command interface. Preserve attachment and
  screenshot behavior, keyboard/focus handling, readable disabled states and a
  usable narrow/high-DPI layout while refining styling.
- [ ] Retain representative before/after captures of both modes and document any
  remaining visual differences. Verify relevant interaction checks and Windows
  rendering; macOS/Linux native verification remains deferred until a remote exists.

## Specification

[WaveBob editor specification](../spec.md). Ticket ID 22 is retained to avoid
renumbering published work; its execution position is immediately after ticket 05.
