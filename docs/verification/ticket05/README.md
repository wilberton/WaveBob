# Ticket 05 verification

Verified on Windows, 23 October 2026. The native renderer test imported generated
stereo and mono WAVs through an attached stdio MCP client, inspected decoded
samples and metadata, switched documents, selected each view and retrieved real
PNG image content. Delivered bytes matched the saved files. Source bytes were
unchanged. [Capture metadata](audio-metadata.json) records the actual session and
rendered revisions. These captures precede the dedicated ticket 22 refinement.

- [Real stereo waveform](real-waveform.png)
- [Arrangement preview](real-multitrack.png)

All 33 ordinary GUI-enabled tests passed, including actual egui Open/document
selection gestures, exact PCM values and peak bounds, cancellation, corrupt,
empty, truncated, missing, inaccessible and unsupported files. The new native
MCP import/capture test passed separately. Clippy with warnings denied passed.
The native PNG and preferences tests used normal Windows permissions because the
restricted execution sandbox blocks Windows atomic file publication.

Reproduce the native check:

```powershell
$env:WAVEBOB_TEST_ARTIFACT_DIR = Join-Path $PWD 'docs\verification\ticket05'
cargo test --locked --features gui --test mcp real_audio_import_switching_and_pixels_through_mcp -- --ignored --test-threads=1
```

The native file chooser is a thin RFD adapter; automated interaction tests invoke
its selected-path handler without opening an OS modal dialog. RFD 0.17.2 is locked
for cross-platform native dialogs. The GUI-free build excludes RFD and egui.
Native macOS/Linux verification remains deferred until a remote exists.
