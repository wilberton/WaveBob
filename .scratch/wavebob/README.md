# WaveBob local tracker

Published: 23 October 2026. The user approved the command test seam and ticket
breakdown and authorized local Markdown publication. The original 21 tickets
were published as `ready-for-agent`; ticket 22 was subsequently added at the
user's request. On 24 October the user approved the live-effects spec/mockup and
publication of tickets 27–44, including all eleven effects and the documented
mode/routing limitations. Read ticket files for current status.

[Canonical specification](spec.md) | [UI mockups](../../docs/ui-concept-01.png) |
[Tracker conventions](../../docs/agents/issue-tracker.md)

The mockups govern appearance, with the specified status bar and panel controls.
MP3 import and sample/arrangement export are required in version 1. They are
included in tickets 06, 13 and 18, with browser, persistence and release coverage.

## Implementation tickets

Follow-up: [62 — Load Ogg Vorbis audio](issues/62-load-ogg-vorbis-audio.md),
approved on 26 October 2026, depends on completed ticket 06.

Current follow-up: [61 — Unify knob dragging and compact effect windows](issues/61-unify-knob-dragging-and-compact-effect-windows.md),
approved for implementation on 26 October 2026 after completion of 47–60.

Approved work: [Live effect stacks, waveform envelopes and linked arrangement
bounces](effects-and-clip-controls.md) (`ready-for-agent`, 24 October 2026), part
of the canonical spec. All eleven algorithms will be written as focused WaveBob
implementations using the pinned upstream source as reference, with egui editors.

This table is navigation. Read individual tickets for current status, acceptance
criteria and blockers; only start a ticket when all its blockers are `done`.

| ID | Ticket | Blocked by |
| --- | --- | --- |
| 01 | [Command-driven session and headless runner](issues/01-command-driven-session-and-headless-runner.md) | None |
| 02 | [Attach to the GUI and capture its actual pixels](issues/02-attach-to-the-gui-and-capture-its-actual-pixels.md) | 01 |
| 03 | [Drive sessions and inspect captures through MCP](issues/03-drive-sessions-and-inspect-captures-through-mcp.md) | 02 |
| 04 | [Resizable panels and persistent status-bar help](issues/04-resizable-panels-and-persistent-status-bar-help.md) | 01 |
| 05 | [Open a WAV and see its real waveform](issues/05-open-a-wav-and-see-its-real-waveform.md) | 01 |
| 22 | [Refine the UI to match the accepted mockups](issues/22-refine-the-ui-to-match-the-accepted-mockups.md) | 02, 04, 05 |
| 06 | [WAV and MP3 import with scalable waveform jobs](issues/06-wav-and-mp3-import-with-scalable-waveform-jobs.md) | 22 |
| 07 | [Precise waveform selection, overview zoom and pan](issues/07-precise-waveform-selection-overview-zoom-and-pan.md) | 22 |
| 08 | [Waveform playback, seeking and loop transport](issues/08-waveform-playback-seeking-and-loop-transport.md) | 07 |
| 09 | [Filesystem browsing and independent audition](issues/09-filesystem-browsing-and-independent-audition.md) | 06, 08 |
| 10 | [Undoable gain, fades and reverse on the editing target](issues/10-undoable-gain-fades-and-reverse-on-the-editing-target.md) | 07 |
| 11 | [Cut, copy, paste, delete and trim samples](issues/11-cut-copy-paste-delete-and-trim-samples.md) | 10 |
| 12 | [Apply bounded reverb to the editing target](issues/12-apply-bounded-reverb-to-the-editing-target.md) | 10 |
| 13 | [Save/export WAV and MP3 samples and protect dirty documents](issues/13-save-export-wav-and-mp3-samples-and-protect-dirty-documents.md) | 06, 11 |
| 14 | [Arrange document clips on editable tracks](issues/14-arrange-document-clips-on-editable-tracks.md) | 22, 10 |
| 15 | [Trim and process an individual arrangement clip](issues/15-trim-and-process-an-individual-arrangement-clip.md) | 12, 14 |
| 16 | [Hear and inspect a multitrack mix](issues/16-hear-and-inspect-a-multitrack-mix.md) | 06, 08, 11, 14 |
| 17 | [Automatic crossfades on overlapping clips](issues/17-automatic-crossfades-on-overlapping-clips.md) | 16 |
| 18 | [Export an arrangement to WAV or MP3](issues/18-export-an-arrangement-to-wav-or-mp3.md) | 13, 15, 17 |
| 19 | [Save and reopen portable editing projects](issues/19-save-and-reopen-portable-editing-projects.md) | 13, 15, 17 |
| 20 | [Verify a complete AI editing and visual-review workflow](issues/20-verify-a-complete-ai-editing-and-visual-review-workflow.md) | 03, 09, 18, 19 |
| 21 | [Deliver launchable desktop and automation builds](issues/21-deliver-launchable-desktop-and-automation-builds.md) | 20 |
| 23 | [Load audio from external file drops](issues/23-load-audio-from-external-file-drops.md) | 06 |
| 24 | [Render continuous waveforms at intermediate zoom](issues/24-render-continuous-waveforms-at-intermediate-zoom.md) | 07 |
| 25 | [Adjust waveform selection edges](issues/25-adjust-waveform-selection-edges.md) | 07 |
| 26 | [Drag the crossfade endpoint](issues/26-drag-the-crossfade-endpoint.md) | 15, 17 |

## Live effects and envelope work

Execution order for this extension is 27 through 44 below. Each effect ticket
delivers working DSP, its own egui editor and reference/public-seam verification.
All eleven effects, including Balance, are required. Advanced EQ quality modes,
external sidechains and Stereo Shaper extra outputs are deferred. Older pre-alpha
project/preset/command migration is outside scope.

Ticket files own status/blocker truth; `ready-for-agent` does not imply unblocked.
At publication, 27's baseline blockers (10, 15, 19) are complete, so it is the new
work's entry point. Ticket 21's separately deferred release checks do not block
this development; preserve and report their remaining verification obligations.

| ID | Ticket | Blocked by |
| --- | --- | --- |
| 27 | [Model live processing state and reference contracts](issues/27-model-live-processing-state-and-reference-contracts.md) | 10, 15, 19 |
| 28 | [Stream effects with safe updates and latency alignment](issues/28-stream-effects-with-safe-updates-and-latency-alignment.md) | 27, 08, 16 |
| 29 | [Build the effect stack and native editor framework](issues/29-build-the-effect-stack-and-native-editor-framework.md) | 28, 03 |
| 30 | [Implement Soft Clipper and its egui editor](issues/30-implement-soft-clipper-and-its-egui-editor.md) | 29 |
| 31 | [Implement Reverb and its egui editor](issues/31-implement-reverb-and-its-egui-editor.md) | 30 |
| 32 | [Implement Parametric EQ and its egui editor](issues/32-implement-parametric-eq-and-its-egui-editor.md) | 31 |
| 33 | [Implement Fast Distortion and its egui editor](issues/33-implement-fast-distortion-and-its-egui-editor.md) | 32 |
| 34 | [Implement Delay and its egui editor](issues/34-implement-delay-and-its-egui-editor.md) | 33 |
| 35 | [Implement Chorus and its egui editor](issues/35-implement-chorus-and-its-egui-editor.md) | 33 |
| 36 | [Implement Limiter and its egui editor](issues/36-implement-limiter-and-its-egui-editor.md) | 33 |
| 37 | [Implement Balance and its egui editor](issues/37-implement-balance-and-its-egui-editor.md) | 33 |
| 38 | [Implement Stereo Enhancer and its egui editor](issues/38-implement-stereo-enhancer-and-its-egui-editor.md) | 33 |
| 39 | [Implement Stereo Shaper and its egui editor](issues/39-implement-stereo-shaper-and-its-egui-editor.md) | 33 |
| 40 | [Implement Transient Processor and its egui editor](issues/40-implement-transient-processor-and-its-egui-editor.md) | 33 |
| 41 | [Add waveform and clip gain and fade handles](issues/41-add-waveform-and-clip-gain-and-fade-handles.md) | 29, 25, 26 |
| 42 | [Refresh linked bounces and mix preserved tails](issues/42-refresh-linked-bounces-and-mix-preserved-tails.md) | 17, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41 |
| 43 | [Save editable processing and export rendered audio](issues/43-save-editable-processing-and-export-rendered-audio.md) | 42, 18, 19 |
| 44 | [Verify the complete live effects workflow](issues/44-verify-the-complete-live-effects-workflow.md) | 43, 20 |
| 45 | [Refine the six-dial Reverb window](issues/45-refine-the-six-dial-reverb-window.md) | 31, 29 |
| 46 | [Refine the Distortion window](issues/46-refine-the-distortion-window.md) | 33, 45 |

## Shared effect visualization renderer

Approved for specification/ticket publication on 25 October 2026:
[shared rendering contract](effect-visualization-rendering.md). Execute
**56 → 57 → 58 → 59 before 47–55**. Ticket 59 verifies the renderer and the
Reverb/Distortion migration before the remaining windows adopt it. Ticket 60 is
a later whole-image multi-scale bloom refinement, after the UI pass by default;
it does not block those nine tickets. Publication starts no implementation.

All shapes marked as glowing share one emission image in each native window.
Every blur level processes that complete image. Effects specify source
colour/intensity rather than individual blur radii.
Use one GPU implementation, with explicit error reporting and no fallback path.

| ID | Ticket | Blocked by |
| --- | --- | --- |
| 56 | [Build the shared visualization scene interface](issues/56-build-the-shared-visualization-scene-interface.md) | 29, 45, 46 |
| 57 | [Render pooled emission and GPU bloom](issues/57-render-pooled-emission-and-gpu-bloom.md) | 56 |
| 58 | [Migrate Reverb and Distortion to the shared renderer](issues/58-migrate-reverb-and-distortion-to-the-shared-renderer.md) | 57 |
| 59 | [Verify the visualization renderer readiness gate](issues/59-verify-the-visualization-renderer-readiness-gate.md) | 58 |

At publication the blockers of 56 are done. It is available to claim; subsequent
renderer tickets and the nine UI refinements are dependency-blocked. All new
tickets retain the `ready-for-agent` triage role; read the actual blocker files
before starting.

## Effect-window visual refinements

Approved on 25 October 2026:
[shared implementation and visual acceptance contract](effect-window-refinements.md)
and [mockup gallery](../../docs/design/effects-suite/README.md).
Each ticket embeds its approved full-size mockup. Retain the existing
selected-band EQ layout, including direct node frequency edits and Shift-drag
bandwidth edits. This publication starts no implementation.

After the renderer gate, execution order is 47–55. Read each ticket and its
actual blocker files before starting; `ready-for-agent` alone does not assert
dependency availability.

| ID | Ticket | Blocked by |
| --- | --- | --- |
| 47 | [Refine the Soft Clipper window](issues/47-refine-the-soft-clipper-window.md) | 30, 46, 59 |
| 48 | [Refine the Parametric EQ window](issues/48-refine-the-parametric-eq-window.md) | 32, 46, 59 |
| 49 | [Refine the Delay window](issues/49-refine-the-delay-window.md) | 34, 46, 59 |
| 50 | [Refine the Chorus window](issues/50-refine-the-chorus-window.md) | 35, 46, 59 |
| 51 | [Refine the Limiter window](issues/51-refine-the-limiter-window.md) | 36, 46, 59 |
| 52 | [Refine the Balance window](issues/52-refine-the-balance-window.md) | 37, 46, 59 |
| 53 | [Refine the Stereo Enhancer window](issues/53-refine-the-stereo-enhancer-window.md) | 38, 46, 59 |
| 54 | [Refine the Stereo Shaper window](issues/54-refine-the-stereo-shaper-window.md) | 39, 46, 59 |
| 55 | [Refine the Transient Processor window](issues/55-refine-the-transient-processor-window.md) | 40, 46, 59 |

## Later bloom-quality refinement

| ID | Ticket | Blocked by |
| --- | --- | --- |
| 60 | [Refine whole-image multi-scale bloom](issues/60-refine-whole-image-multi-scale-bloom.md) | 59 |

This later quality stage combines multiple blur scales of the entire shared
emission image. It is not a blocker for 47–55 and is scheduled after those
refinements by default.

## Initial frontier

Ticket 01 can start immediately. The other tickets wait for their declared
blockers. This is the publication-time frontier; derive the live frontier from
the ticket files rather than maintaining another status copy here.

## Publication decisions

- Retain published IDs. Ticket 22 adds the user-requested UI refinement step
  immediately after 05, gating the remaining editor work without renumbering it.
- Match the generated WaveBob mockups; Artcraft is architectural background.
- Include CBR/VBR MP3 import and MP3 sample/mix export in version 1.
- Ticket 09 depends on 06 for MP3 audition/import coverage.
- Ticket 13 depends on 11 for shared export resampling.
- Ticket 18 depends on 13 for the shared MP3 encoder.
- The command/query seam is approved; focused gesture/render tests complement it.
- The accepted live-effects mockup governs compact stack rows and separate native
  egui editor windows. Reference values/units govern real controls where image
  text is illustrative.
- Include all eleven effects, stable typed parameters and original WaveBob DSP
  following the reference algorithms; preserve source provenance/applicable notices.
- Preserve tails and refresh all linked instances without timeline ripple. Source
  editing, envelopes and stacks have separate ownership and chronological history.
- Tickets 27–44 extend the delivered behavior; completed baseline tickets keep
  their acceptance/verification history. Publication starts no implementation.

## Completion —25 October2026

All44 implementation tickets are complete under the user's automated acceptance
policy. [Final local verification, native captures, performance and Windows kit](../../docs/verification/live-effects-workflow/README.md)
records the full delivered workflow and explicit physical/other-platform limits.
The final stage used local suites and targeted repairs; no further GitHub platform
checks were dispatched. Individual ticket histories remain authoritative.

## Effect-window and renderer completion — 25 October 2026

The shared renderer (56–60) and remaining effect-window refinements (47–55) are
delivered, with their dependency-ordered commits, native captures and per-ticket
records preserved. [Final integrated verification](../../docs/verification/effect-ui-final/README.md)
records the full GUI/GUI-free suites, follow-up lifecycle repairs and final Windows
executables. Individual tickets remain the status and acceptance authority.
