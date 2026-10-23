# 10: Undoable gain, fades and reverse on the editing target

**What to build:** Apply gain, fade in/out or reverse to a selection or the
whole sample through the right panel or commands, then undo and redo.

**Blocked by:** 07 — Precise waveform selection, overview zoom and pan.

**Status:** ready-for-agent

- [ ] Core history and immutable revisions support one undo entry per committed
  Apply. Copy/navigation/parameter changes do not enter edit history.
- [ ] The UI displays and resolves the target explicitly; automation supplies exact
  target IDs/ranges and parameters. Outside-target frames remain unchanged.
- [ ] DSP matches gain/fade/reverse fixtures, including one-frame edge cases;
  undo/redo restores audio and relevant selection/cursor state.
- [ ] Invalid, cancelled, failed and stale jobs leave audio/history unchanged;
  waveform caches and active playback update safely when a new revision commits.

## Specification

[WaveBob editor specification](../spec.md).
