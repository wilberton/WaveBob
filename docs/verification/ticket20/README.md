# Complete workflow and visual review

Windows, 24 October 2026. The [reproducible workflow](../../automation.md) is shared
by the example client, adapter-parity integration test and native MCP test.
It imports WAV and MP3, auditions through the filesystem browser, selects exact
frames, applies gain, undoes/redoes, pastes stereo 48 kHz audio into mono 32 kHz
audio, creates four tracks/five clips, processes one clip, crossfades an overlap,
exports a selected sample and whole mix as MP3, reimports both, and reopens a
portable project after deleting all original sources.

The library, real headless process and independent MCP process agree on the
public summary, 4096 frames of stereo mixed PCM and complete exported float-WAV,
mixed MP3 and selected-sample MP3 bytes. Standalone routes use no window/device.
The full suites pass 86 GUI-free and 124 GUI-enabled ordinary tests, plus native
checks described here; both Clippy variants deny warnings. Formatting passes.

The actual optimized app/MCP workflow captures both modes with panels shown,
hidden and restored, with session/core/presentation revision fences. PNG image
content from MCP is byte-identical to each saved capture. Records retain PCM and
capture metadata. Browser audition ran through the native output adapter; this
verifies device/callback operation, without a subjective listening claim.

Independent FFmpeg 7.1 decoding confirms a 576000-frame stereo 48 kHz MP3 mix and
176400-frame stereo 44.1 kHz sample export. Compared with the lossless WAV without
fitted alignment, mix RMS error is 0.004435, maximum error 0.01413 and correlation
0.99999991, excluding 1024 edge frames. See [independent results](independent-mp3.json)
and `scripts/check-workflow-mp3.py`. This decoder is QA tooling only.

## Visual inspection

Compared the accepted [mockup](../../ui-concept-01.png) with the live captures:

- [Wide waveform](wide/waveform.png) and [arrangement](wide/multitrack.png): compact
  mode tabs/header, Inter typography, blue selection/waveforms, teal/blue/gold
  tracks, pane proportions, framed overview and centered transport.
- [Hidden waveform](wide/waveform-hidden.png) and
  [hidden arrangement](wide/multitrack-hidden.png): accessible restore arrows and
  full-width selection/status readout; restoring returns remembered widths.
- [Narrow waveform](narrow/waveform.png), [narrow arrangement](narrow/multitrack.png),
  [150% waveform](high-dpi/waveform.png) and [150% arrangement](high-dpi/multitrack.png)
  retain panel arrows, controls, selection and
  status rows. Inspection caught overlapping clip titles; final titles elide at
  the next overlapping clip's start and clip to their own boxes.
- [Hover](help/status-hover.png) and [keyboard focus](help/status-focus.png) show
  persistent help at the bottom with no floating help popup. These use the real
  application UI and native renderer in the QA-only `gui-review` wrapper, with
  deterministic egui input injected at the input boundary. The normal app has no
  input-injection command or file watcher. Hidden-window OS mouse messages lose
  hover immediately, so those attempts were not treated as verification.

Final refinements add blue-filled sliders with round handles and an outlined empty
track drop area. Extra undo/redo, precise clip bounds, project output and master
controls support the implemented editor. Tall windows give lanes/channels more
height than the concept's compact aspect ratio. The overview correctly marks the
zoomed interval; the sample is an owned 12-second analytical fixture. Status bar
and panel arrows are requested additions. OS chrome is outside client captures.

## Failure and responsiveness checks

The workflow rejects a stale audio target; command tests additionally cover invalid
frames/parameters, stale session/presentation revisions and unavailable renderer
requests. Storage tests cover missing/corrupt assets, unsafe paths, unsupported
versions and preserving the current workspace. Transport tests inject device loss,
verify recoverable retry, and guard zero callback allocations/frees. Real native
import cancellation preserves the existing document. Save/export cancellation,
overwrite and publication races remain covered by their command tests.

The ten-minute 48 kHz stereo case exposed unbuffered per-sample WAV file writes:
export exceeded the 120-second test deadline. A shared 256 KiB buffer fixes both
sample/mix exports and managed project assets, with flush errors checked before
atomic publication. All existing PCM/byte-parity checks pass afterward.

| Optimized native check | Recorded result |
| --- | --- |
| Source / decoded PCM | 115200044 / 230400000 bytes |
| Import | 1.586 s |
| Whole-file gain | 0.775 s |
| Whole-file 16-bit WAV export | 1.293 s |
| Maximum live mode-command round trip during these jobs | 263.9 ms |
| Full-range 1024-bin waveform query | 319.9 ms |
| Resident before / imported / after edit and export | 90.4 / 315.7 / 541.5 MiB |
| Peak working set across edit/export | 763.1 MiB |

[Raw benchmark](performance/large-import.json) includes progress and memory.
Round trips include hidden-window scheduling, IPC and MCP serialization; they are
not frame render times or a guarantee for arbitrary file sizes. Other local build
and test work was active during recording. The existing 512 MiB per-audio and
2 GiB retained PCM budgets remain enforced. Native macOS/Linux, X11/Wayland,
physical trackpad and subjective audio checks remain deferred until a remote and
those platforms are available, consistent with the user's CI deferral.

Reproduce the ordinary scenario with `cargo test --locked --no-default-features
--test workflow`. Native cases are ignored tests in `tests/mcp.rs`; use
`complete_editing_workflow_and_visual_review_through_mcp` with release executable
overrides and `WAVEBOB_TEST_ARTIFACT_DIR`. Window size/scale use
`WAVEBOB_WORKFLOW_SIZE` / `WAVEBOB_WORKFLOW_SCALE`. For deterministic native help
captures, build `cargo build --example gui-review`, point `WAVEBOB_GUI_TEST_BIN` at
that example and run `native_status_hover_and_keyboard_focus_capture`.
