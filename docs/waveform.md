# Cursor, selection and waveform navigation

A primary click places the cursor at a sample-frame boundary and clears the
selection. Drag in either direction to select a half-open interval across all
channels. Dragging previews the selection; release commits once. Escape cancels
the preview. With no active gesture, Escape clears the selection and retains the
cursor. Ctrl/Cmd+A selects all. The Edit menu offers both selection actions.

Exact commands use document IDs and unsigned integer frame positions:

```json
{"id":"cursor","command":"documents.cursor.set","params":{"document_id":"UUID","frame":1200}}
{"id":"range","command":"documents.selection.set","params":{"document_id":"UUID","start_frame":1200,"end_frame":3600}}
{"id":"all","command":"documents.selection.all","params":{"document_id":"UUID"}}
{"id":"clear","command":"documents.selection.clear","params":{"document_id":"UUID"}}
```

Reversed selection endpoints normalize; equal endpoints clear the selection and
place the cursor there. Exact out-of-bounds requests fail atomically. The GUI
clamps pointer gestures at document bounds. Cursor-at-end is allowed. Selecting
does not dirty audio or create an audio revision/history entry. Repeating an
identical command is a no-op. Session revision guards reject stale writes; drag
completion carries its starting revision so intervening mutations cannot be
silently overwritten. Document/revision changes invalidate an active preview.

The status bar reports integer selection bounds and frame count plus cursor,
start/end and duration to microsecond display precision. Frame numbers remain
authoritative for exact positions. With a selection, a second line retains hover
and keyboard-focus help; no popup tooltip obscures the waveform. The effects
panel shows Selection or Whole sample as the current scope.

Navigation preserves cursor and selection:

| Input | Behavior |
| --- | --- |
| Overview box drag | Pan, preserving visible duration |
| Overview left/right edge | Resize, anchoring the opposite edge |
| Overview click outside box | Center the current interval there, clamped to bounds |
| Horizontal or ordinary vertical scroll | Pan time |
| Ctrl/Cmd + scroll, pinch | Zoom around the pointer |
| Middle-button drag | Pan time |
| Toolbar minus/plus | Zoom around the visible center |
| Fit button | Show the whole sample |

The minimum interval is one sample frame. The ruler uses absolute document time
at the current zoom. The overview always spans the complete sample. The waveform
uses cached extrema at broad scales and actual connected samples/points at frame
detail; a single visible sample is shown held across that frame's interval.

GUI presentation commands expose exact view intervals separately from core state:

```json
{"id":"view","command":"presentation.viewport.set","params":{"document_id":"UUID","start_frame":96000,"end_frame":384000,"expected_presentation_revision":0}}
{"id":"inspect","command":"presentation.get"}
```

Presentation snapshots contain a `viewports` map keyed by document ID. A missing
entry means the full sample. View changes use presentation revisions, do not
change the core revision or layout preferences, and are remembered per open
document. Closed documents are pruned and intervals clamp when audio lengths
change. Independent GUI-free hosts return `missing_capability` for view commands;
selection and exact sample queries remain available headlessly.

Global selection shortcuts respect text editing, menus/modals, native dialogs,
focus loss and repeated keys. Capture freezes gesture updates while framebuffer
readback is pending. A capture of an active preview reports
`waveform-gesture-preview` in `pending_resources`.
