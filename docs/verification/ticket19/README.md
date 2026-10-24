# Portable project verification

Verified on Windows on 24 October 2026. The complete GUI-enabled suite passes
123 ordinary tests; the GUI-free suite passes 85. Clippy passes both configurations
with warnings denied. Four project storage tests cover lossless WAV/MP3-origin
round trips, pinned and processed revisions, crossfades, deleted original sources,
repeat-save reuse, dirty baselines, stale loads, cancellation, overwrite refusal,
managed-asset protection and corrupt/missing/unsupported/unsafe project inputs.
Two GUI interaction tests cover project chooser completion/cancellation,
source-preserving project save, Save project on quit, Cancel/Discard on open and
failed-load workspace preservation. Existing sample save/MP3 close tests pass.

The ignored `portable_project_reopens_without_sources_and_captures_both_modes`
test in `tests/mcp.rs` drove the actual native app through an attached stdio MCP
client: import a Unicode-path MP3, edit it, insert overlapping pinned clips,
prepare/read the mix, save a project, delete the original, add a discardable
track, reopen and compare document state, clip state and exact mixed PCM.
The retrieved [waveform](project-waveform.png) and
[multitrack](project-multitrack.png) PNGs were inspected. Their session/revision
fences and job asset counts are in [the record](project-roundtrip.json).
The short codec fixture is deliberately much shorter than the design recording.
The fallback font does not cover its CJK filename glyphs; the Unicode path and
audio round trip work. Broader visual workflow checks follow in ticket 20.

Reproduce with `WAVEBOB_TEST_ARTIFACT_DIR` set to a fresh output directory and:

```sh
cargo test --locked --features gui --test mcp portable_project_reopens -- --ignored --nocapture
```

Native macOS/Linux verification remains deferred under the user's no-remote CI
decision. No original source file is needed to reopen a saved project.
