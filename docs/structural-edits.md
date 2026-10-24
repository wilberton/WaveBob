# Structural editing and the audio clipboard

The Edit menu provides Cut, Copy, Paste, Delete and Trim to selection.
Ctrl/Cmd+C/X/V use the internal audio clipboard; Ctrl/Cmd+T trims, and
Delete/Backspace deletes the selection. Typing/dialog guards apply. Text fields
keep their normal platform clipboard. Cut/copy/delete/trim require a nonempty
selection in the GUI. Paste inserts at the cursor or replaces the selection.
Copy does not dirty audio or enter history.

`edit.copy`, `edit.cut`, `edit.delete` and `edit.trim` take `target` using the same
explicit revision/range or whole-sample target as [effects](editing.md). Copy
returns clipboard metadata; the other operations return jobs.
`edit.clipboard.get` reports frames, rate, channels and retained source revision.
Clipboard audio survives closing its document and removing the original file.
Cut publishes its clipboard update only when deletion succeeds; cancellation
leaves both unchanged.

`edit.paste` takes `document_id`, `revision_id`, `start_frame`, `end_frame`.
Equal endpoints insert; a nonempty interval is replaced. An empty clipboard is
an error. The result selects the pasted interval and places the cursor at its end.
Delete maps positions inside the removed range to its start and shifts later
positions left. Trim intersects selection/cursor with the retained range and
translates coordinates to zero. Undo restores preceding audio and navigation.
Deleting everything retains a valid empty document, which accepts paste or undo;
playback and effects are disabled. The viewport clamps after length changes and
fits again when an empty view becomes nonempty.

Paste uses a shared deterministic 64-tap Blackman-windowed sinc resampler with
an anti-alias cutoff of 0.94 times the smaller Nyquist rate. Boundary values are
held outside the source interval and weights normalized. Duration rounds to the
nearest destination frame (at least one for nonempty input). Equal-rate conversion
preserves exact frames. Mono duplicates to stereo; stereo downmix averages left
and right. Conversion, copying and waveform preparation run off the UI/audio
callback. Working revisions retain their destination rate/layout. The retained
PCM budget includes clipboard buffers after their documents close.
