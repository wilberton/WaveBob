# Sample effects and history

Choose gain, fade in, fade out or reverse in the right panel, then press Apply.
The displayed target resolves to the current selection or the whole nonempty
sample. Moving the gain slider selects gain; parameter/effect choices alone do
not change audio. Apply is unavailable during a selection gesture or without a
sample. Reverb is available with bounded room/decay/wet controls. Clip-specific
processing arrives in ticket 15. See [structural edits](structural-edits.md) for
cut/copy/paste/delete/trim.

Undo/Redo buttons and the Edit menu share `history.undo` and `history.redo`.
Ctrl/Cmd+Z undoes; Ctrl/Cmd+Shift+Z or Ctrl+Y redoes. Typing/modal guards apply.
Navigation, selection, parameter changes and playback do not enter history.
Undo restores the audio revision and relevant cursor/selection. A new committed
edit clears redo. Closing a document removes its history entries; no source file
is written by an effect.

`edit.process` takes an explicit target and effect and returns a cancellable
job. A whole target uses `scope: whole`, `document_id` and `revision_id`. A range
target uses `scope: range` with those IDs plus `start_frame` and `end_frame`.
Ranges are nonempty, half-open and span all channels. The supplied revision must
be the document's current immutable audio revision. A superseded background
result fails with `stale_revision`, never replacing newer audio.

```json
{"id":"fade","command":"edit.process","params":{"target":{"scope":"range","document_id":"DOCUMENT_UUID","revision_id":"AUDIO_UUID","start_frame":48000,"end_frame":96000},"effect":{"kind":"fade_in"}}}
```

Effect kinds are `gain` (finite `db` from -120 to +60), `fade_in`, `fade_out`,
`reverse` and `reverb`. Gain converts dB to amplitude. Fades use linear amplitude with
exact zero/unity endpoints; a single-frame fade uses unity. Reverse changes frame
order independently for each channel. Outside-target frames remain unchanged.
All processing stays floating point. Document metadata reports `peak_level` and
`overloaded_samples`; the panel displays overloads. Only device/export conversion
clips or quantizes. Nonfinite or overflowing processing results fail atomically.

`history.get` reports availability, labels and stack counts. An edit that actually
changes PCM creates one immutable revision and one history entry at publication.
No-op processing succeeds without a new audio revision or history entry. Job
cancellation, invalid requests and failed/stale results leave audio/history intact.
Decoding, DSP, comparison and waveform preparation run off the UI thread. Working
and history revisions share immutable buffers. The current retained PCM budget
is 2 GiB; allocation/budget failures produce structured errors. Playback adopts
new revisions at its bounded audio-block handoff, and cached waveforms refresh
by revision identity.

## Reverb

Choose Reverb, set Room (0–1), Decay (0.1–10 seconds) and Wet (0–1), then Apply.
Wet zero is unchanged dry audio; one is entirely reverberated. The displayed
policy is **Tail is truncated at target end**. Each target starts with reset
effect state; no preceding audio bleeds into a selection. Outside frames and
document length remain unchanged. Undo/redo and job cancellation/stale-result
handling use the same edit path as the other effects.

The command effect is `{"kind":"reverb","room_size":0.5,"decay_seconds":1.5,"wet":0.25}`.
The deterministic Schroeder network has four parallel feedback comb delays and
two serial all-pass diffusers per channel, with bounded room scaling and decay
feedback derived from RT60. It processes in floating point and reports overloads.
This is a general-purpose built-in reverb, with no external plugin dependency.

`presentation.effects.set {effect, expected_presentation_revision?}` chooses
visible effect parameters in a GUI without applying audio or recording history.
`presentation.get.effect` reports them. The GUI uses this same presentation seam;
MCP clients can set parameters, apply an explicit audio target, then fence a
screenshot using the returned presentation revision. Layout preferences remain
separate; these transient effect settings are not saved as layout preferences.
