# Multitrack mixing

Space or the centered play/pause button plays the arrangement from its cursor in
multitrack mode. Enter/comma rewinds; slash toggles looping. A time selection
defines loop bounds, including silent time beyond the last clip. With no selection,
the complete arrangement loops. Switching modes, opening a document view or
starting browser audition pauses the current editor voice.

Track M/S, dB gain and pan are editable in the track header. If any track is soloed,
only soloed, unmuted tracks contribute; mute takes precedence. Mono uses equal-power
pan (center sends -3.01 dB per channel). Stereo uses balance: center preserves both
channels and moving to an edge attenuates the opposite channel. Track and master
gain combine without a limiter. The footer shows peak level before device clipping;
red indicates overload. Master gain is available beside the arrangement heading.

`mix.get` reports effective track audibility, controls, current/prepared revisions,
pending preparation job/error, play intent and output level/overload. `mix.master.set`
takes gain_db (-120..24) and enters shared undo history. Track controls use
`tracks.update`; slider/knob releases commit one edit.

`mix.prepare` takes the arrangement revision_id and returns a background job.
Repeated requests share the in-flight or completed preparation for that revision;
an explicit request retries cancellation/failure. `jobs.get.output` contains
`kind: mix_prepared` and its revision, duration and project rate. A stale or cancelled
preparation cannot replace the current prepared mix.

`mix.samples` takes revision_id, start_frame and frames (1..4096). It returns actual
stereo project-rate PCM, peak level and overloaded-sample count, with silence in
gaps. Preparation and inspection need no window or physical audio device.

`mix.play`, `mix.play_pause`, `mix.seek` (frame) and `mix.rewind` operate on arrangement
transport. Play prepares missing resources before starting; pausing also cancels
pending play intent. These play commands require an advertised output adapter.
Seek/rewind preserve selection and playing/paused state. The existing transport
snapshot's document_id field identifies the arrangement UUID while mixing, and
audio_revision_id identifies its prepared arrangement revision.

Source intervals and instance DSP results feed one immutable mixer used by offline
inspection and device/manual playback. Mixed-rate clips use the shared resampler
off-thread. Instances share immutable buffers; same-rate sources need no PCM copy.
Prepared timeline segments bound active-clip lookup. Graph changes use the existing
bounded audio handoff; the callback allocates, frees, locks and reads files zero
times. Old graphs return to the control thread for destruction. Queries distinguish
current/prepared/requested/applied state while a control change is preparing.

The arrangement clock stays at 48 kHz. Device-rate conversion uses the same rational
phase as sample playback. Finite floating-point headroom is preserved; mixes whose
conservative peak bound exceeds float32 range are rejected before playback. Retained
source, history, clipboard and prepared PCM share the existing 2 GiB budget.

Two overlapping clips on a track use [automatic crossfade envelopes](crossfades.md).
A third simultaneous clip is rejected atomically.

Preparation freshness follows linked document source/processing/tempo dependencies
as well as the arrangement edit revision. mix.get publishes separate required and
prepared render identities; derived bounce/cache refresh is outside edit history.
128 prepared clip voices and64 concurrent tail voices are admitted separately.
Tails hold body-crossfade boundary gain and extend the contributing non-loop end;
loop bounds remain body bounds. The protected512MiB bounce cache and2GiB retained
PCM guard count unique buffers; bounded worker temporaries are separate. See
[linked bounces](verification/linked-bounces/README.md) and [export](mix-export.md).
