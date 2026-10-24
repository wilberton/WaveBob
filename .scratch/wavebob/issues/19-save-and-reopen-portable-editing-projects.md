# 19: Save and reopen portable editing projects

**What to build:** Save sample revisions and the arrangement as a versioned
project with managed audio assets and reopen it without the original files.

**Blocked by:** 13 — Save/export WAV and MP3 samples and protect dirty documents; 15 — Trim and process an individual arrangement clip; 17 — Automatic crossfades on overlapping clips.

**Status:** done

- [x] Persist documents, pinned audio revisions, tracks/clips, processing and project
  rate; UI preferences remain separate and undo history need not survive restart.
- [x] A save writes a coherent snapshot and managed assets atomically; dirty state,
  overwrite intent, failure/cancellation and close policies remain correct.
- [x] Loading missing/corrupt assets or unsupported versions produces clear errors
  without destroying the current workspace.
- [x] Save/reload round trips preserve queryable state and rendered audio, even
  after original source files are moved or deleted from the test workspace.
- [x] MP3-origin audio is stored as lossless managed assets; repeated project saves
  introduce no additional lossy encoding generations.

## Specification

[WaveBob editor specification](../spec.md).

## Progress

The shared core writes versioned, checksummed manifests and managed lossless
float-WAV assets, with cancellation and atomic manifest publication. Tests reopen
WAV/MP3-origin edited/pinned/processed audio after deleting original sources,
compare mixed PCM, verify repeat-save reuse and protect the current workspace
against corrupt/missing assets, unsupported versions, unsafe paths and stale loads.
123 GUI-enabled and 85 GUI-free ordinary tests pass, as do Clippy in both builds.
GUI dialogs and project-aware close handling preserve source-dirty audio safely.
Native MCP save/reopen after source deletion preserves exact mix PCM and retrieves
both modes' screenshots; see [verification](../../../docs/verification/ticket19/README.md).
See [project behavior and format](../../../docs/projects.md).
