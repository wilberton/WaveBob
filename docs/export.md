# Sample output commands

The output core is implemented in ticket 13; GUI save/export and dirty-close
dialogs are still being wired in this ticket.

`audio.export` takes `target` (an explicit immutable document revision plus whole
sample or nonempty frame interval), `path`, optional `format`, `sample_rate`,
`overwrite` and `overload`. New exports default to 24-bit PCM WAV. WAV encodings
are `pcm16`, `pcm24`, `pcm32` and `float32`. MP3 uses
`{"kind":"mp3","bitrate_kbps":192}`; supported CBR bitrates are 128/192/256/320.
MP3 rates are 32000/44100/48000 Hz. The source rate is preserved when supported,
otherwise MP3 defaults to 44100 Hz. The shared resampler handles explicit conversion.
Output retains mono/stereo layout and reports settings, conversion, intended
frames, bytes and MP3 delay/padding.

`documents.save` takes `document_id`, `revision_id`, optional `path`, `format`,
`sample_rate`, `overwrite` and `overload`. For WAV, omitted format retains the
current save encoding, initially the source encoding. MP3 Save requires explicit
lossy format settings on every call. Existing destinations require `overwrite:
true`. Lossless WAV Save As is always available by choosing WAV and a `.wav` path.
Plain export cannot replace the document's backing file; use whole-document Save
for that operation. The chosen filename extension must match the format.

Integer WAV quantizes by nearest integer with signed full-scale bounds. Its
default `overload: reject` also applies to MP3; choose `clip` explicitly to clamp
to [-1, 1]. Float WAV retains finite headroom. Summaries report overload and clip
counts. Saving MP3 does not change the lossless working revision or its samples.

Both commands return jobs. `jobs.get.output` is a typed object with `kind:
audio_export` and a `details` summary. Save also returns current document metadata
in the existing `result` field. Save records the exact saved revision, so undo/redo
and edits made during a save retain correct dirty state and the current backing
path/format.

Workers prepare and sync temporary output in the destination directory. Before
publication, the serialized core checks cancellation and the source revision.
Then the worker atomically publishes using explicit overwrite/no-clobber policy.
Cancellation after publication starts returns `busy`; it never reports success
while allowing a cancelled job to write output. Preparation/stale/cancel failures
drop temporary output and retain the destination and saved state. Native Windows
atomic publication requires normal filesystem permissions; the restricted agent
sandbox denies tempfile's attribute update.

See [encoder inputs, metadata and distribution requirements](mp3-encoder.md).
