# WaveBob local tracker

Published: 23 October 2026. The user approved the command test seam and ticket
breakdown and authorized local Markdown publication. The original 21 tickets
were published as `ready-for-agent`; ticket 22 was subsequently added at the
user's request. Read ticket files for current status.

[Canonical specification](spec.md) | [UI mockups](../../docs/ui-concept-01.png) |
[Tracker conventions](../../docs/agents/issue-tracker.md)

The mockups govern appearance, with the specified status bar and panel controls.
MP3 import and sample/arrangement export are required in version 1. They are
included in tickets 06, 13 and 18, with browser, persistence and release coverage.

## Implementation tickets

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
