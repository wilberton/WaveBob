# Live processing state

Documents and clips own separate `Processing` values: a gain/fade envelope,
ordered stable effect instances, whole-stack bypass and a processing edit revision.
Source PCM revisions are independent. `processing.get` returns committed state,
any active preview, body frames, effect tempo and a render identity.

`processing.catalogue` lists all eleven approved kinds. An unavailable processor
cannot be added and never silently passes audio. Supported parameter metadata
is published with its processor implementation, before that processor is enabled.
Values distinguish boolean, integer and real parameters. Unknown IDs, incorrect
types, nonfinite values and out-of-range values fail before publication.

`processing.edit` requires an explicit document/clip owner and its expected
processing edit revision. Its change is one of `set_envelope`, `add`, `remove`,
`move`, `bypass`, `bypass_all` or `set_parameter`. Gain uses finite -120..60 dB;
JSON null means mute. Fades use source frames and must fit the body independently.
Zero/one-frame fades are unity; intersecting ramps multiply.

Begin a gesture with `processing.preview.begin`, retaining the returned preview ID.
Updates validate the same actions as direct edits and coalesce into one transient
state. Queries expose both states. Commit creates one chronological history entry;
cancel restores committed state. A no-op creates no history or revision. An invalid
update preserves the last valid preview. Only one preview exists per session.
Starting another valid preview replaces it. Successful concurrent mutations cancel
an unfinished preview; stale source/owner state cannot commit. Previews expire after
60 seconds without an update. GUI windows must retain owner IDs and cancel gestures
on target changes; selection never changes the explicit automation owner.

Render identity hashes source revision, envelope, ordered effects/parameters/bypass,
project Effect tempo, DSP version and tail policy. Cursor, selection, tab/window state
and processing edit revision do not contribute. Tempo starts at 120 BPM and accepts
20..400 BPM. Tempo changes participate in chronological history.

New clips explicitly link to documents and record full-body versus trimmed intent.
Closing a linked document hides its tab but retains its editable source in the
project. Activate the document ID to reopen it. `processing.make_independent`
preserves a current pinned snapshot; stale document processing requires a completed
bounce before detachment. Clip envelopes and stacks remain independent.

The state foundation precedes streaming audio, native editors and bounce refresh.
Those capabilities are delivered by tickets 28, 29 and 42 respectively. Project
render/save integration is ticket 43. Explicit source transformations use `edit.process` and `clips.pcm.process`; the
obsolete `effects.apply` and `clips.effects.apply` command names are removed.
These source operations never represent live stack state. The old tool selector
is replaced by native stack editors in ticket 29. Structural reverse remains
accessible through the source-edit command.
