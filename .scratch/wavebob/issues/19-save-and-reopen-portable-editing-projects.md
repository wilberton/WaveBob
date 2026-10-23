# 19: Save and reopen portable editing projects

**What to build:** Save sample revisions and the arrangement as a versioned
project with managed audio assets and reopen it without the original files.

**Blocked by:** 13 — Save/export WAV and MP3 samples and protect dirty documents; 15 — Trim and process an individual arrangement clip; 17 — Automatic crossfades on overlapping clips.

**Status:** ready-for-agent

- [ ] Persist documents, pinned audio revisions, tracks/clips, processing and project
  rate; UI preferences remain separate and undo history need not survive restart.
- [ ] A save writes a coherent snapshot and managed assets atomically; dirty state,
  overwrite intent, failure/cancellation and close policies remain correct.
- [ ] Loading missing/corrupt assets or unsupported versions produces clear errors
  without destroying the current workspace.
- [ ] Save/reload round trips preserve queryable state and rendered audio, even
  after original source files are moved or deleted from the test workspace.
- [ ] MP3-origin audio is stored as lossless managed assets; repeated project saves
  introduce no additional lossy encoding generations.

## Specification

[WaveBob editor specification](../spec.md).
