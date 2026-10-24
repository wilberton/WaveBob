# Arrangement commands

`arrangement.get` returns tracks in order, their clips, exact cursor/time selection,
selected clip IDs and the arrangement audio revision/dirty state. Coordinates are
integer frames at the project rate (48 kHz); timeline positions are nonnegative
and bounded to 24 hours. Source intervals are half-open frames at the source rate.
Durations round once to the nearest project frame, with a one-frame minimum.

`tracks.add` takes optional `name` and insertion `index`. `tracks.update` takes
`track_id` plus optional name/index/mute/solo/gain_db/pan. Track gain is -120..24 dB,
pan -1..1. `tracks.remove` removes the track and all its clips in one history entry.

`clips.insert` takes `track_id`, `document_id`, `revision_id`, source_start,
source_end and position. It pins that immutable revision; waveform editing and
closing the source document leave the clip unchanged. `clips.move` takes clip_id,
track_id and position; `clips.remove` takes clip_id. Invalid placement, overlap,
stale revisions or invalid identifiers leave state/history unchanged. Overlaps
are rejected pending the automatic crossfade implementation in ticket 17.

`clips.waveform` takes clip_id and bins (1..2048). It reads the pinned source
interval even after closing the source document. Queries never reopen files.

`arrangement.navigate` sets cursor, optional selection and selected_clip_ids.
Navigation stays out of audio history. All sample and arrangement edits use
the same chronological `history.undo` / `history.redo` commands. Source revisions
remain retained by current clips and undo/redo states.

In the GUI, add a track and drag an open document into its timeline. Clip body
drags move it in time or between tracks. Release commits once; Escape, stale
session state or off-track release cancels. Click empty space for a cursor and
drag it for a time selection. Ctrl/Cmd-click toggles selected clips. Rename tracks
in their header and right-click the track number to reorder or remove a track.
Track slider/knob releases commit one change. See [mixing and playback](mixing.md).

`clips.trim` takes clip_id, edit_revision_id, source_start and source_end, and
returns a job. Bounds address the original pinned revision. Moving the left
source edge shifts timeline position by the same duration, rounded to the nearest
project frame; negative positions are rejected. The right edge changes duration.
Drag edges in the GUI or use the exact source frame fields beneath the effects.

`clips.effects.apply` takes clip_id, edit_revision_id and an effect, returning a
job. It uses the same gain/fade/reverse/reverb DSP as waveform edits. Each clip
retains its ordered processing steps (up to 32) and a prepared immutable result.
Trimming rebuilds those steps over the new interval; reverb state resets at its
start and the tail ends at its boundary. The original source revision stays pinned.
Other clips and open documents remain unchanged. Mixer playback and offline
inspection consume that same prepared result.

Completed clip jobs return `output.kind: clip_edit` and the clip in `details`.
`clips.samples` inspects up to 4096 processed source-rate frames, relative to the
clip start. Its revision_id is the clip's edit_revision_id, independent from the
pinned source revision_id and optional processed_revision_id. Clip mutation or
removal during processing rejects the stale result; cancellation never publishes.
No-op processing adds no history. Peaks/processed audio stay retained by undo/redo.
The right panel enables Apply only for exactly one selected clip; parameter
changes alone do not edit audio.

`presentation.timeline.set` sets start_frame/end_frame of the visible interval.
Fit and +/- buttons, Ctrl/Cmd-wheel or pinch zoom, and horizontal/Shift-wheel pan
change only presentation state. The status bar shows exact selection duration,
cursor and selected-clip count. Captures label active gesture previews explicitly.
