# WaveBob

Waveform editing and arrangement use distinct document, audio and view identities.

## Language

**Project**: A portable snapshot of open documents, their lossless working audio
and the arrangement, saved as a versioned manifest with managed audio assets.
Its saved baseline is distinct from each document's original source-file baseline.

**Managed audio asset**: An immutable lossless float-WAV revision stored beside a
project manifest. It preserves source or processed PCM without an MP3 generation.


**Audio document**: An open sample with its own identity, working audio revision,
cursor and selection, plus an editable gain/fade envelope and effect stack; its
source file is a separate object.
_Avoid_: File (when referring to the open editable document).

**Audio revision**: Immutable decoded audio together with sample rate and channel
layout; clips and history can retain the same revision.
_Avoid_: Version (without specifying audio or session state).

**Sample frame**: One point in time containing one sample per channel; a stereo
frame contains two samples.
_Avoid_: Sample (when counting positions across multiple channels).

**Selection**: A half-open interval of sample frames spanning a document's channels
or an arrangement's timeline; an empty interval means no selection.

**Edit cursor**: A frame boundary marking where a click places editing or playback
start; it may be at the end of a document.

**Viewport**: The interval visible in the main waveform, independent of the edit
cursor and selection. The overview shows this interval within the whole sample.

**Session revision**: The version of committed session state, excluding job progress
and presentation-only changes.

**Presentation revision**: The version of view state, including active mode and
panel layout, separate from audio edit history.

**Requested transport**: The latest playback or seek intent accepted by the
session, which may await the audio output boundary.

**Applied transport**: The playback source and frame position currently adopted
by the output renderer, identified by its handoff generation.

**History entry**: One committed sample or arrangement edit with before and after states
needed for undo and redo. Navigation and playback are outside edit history.

**Overload**: A floating-point audio value whose absolute amplitude exceeds
unity. It is retained during editing and reported before output conversion.

**Audio clipboard**: The session's retained copy of an audio interval, independent
of the operating system's text clipboard and the source document's lifetime.

**Arrangement**: Ordered tracks with clips, a project-rate timeline cursor and
time selection. Its audio revision changes for edits, excluding navigation.

**Track**: A named arrangement lane with stable identity, mute, solo, gain and pan.
Removing it and its clips is one history entry.

**Clip**: An independently identified instance of a pinned audio revision and
source interval, positioned on a track in project frames. Closing the source
document leaves its audio available; subsequent source edits affect it only when
it has a document link.

**Pinned clip**: A clip retaining an independent source snapshot, unaffected by
later edits to the originating document.

**Linked clip**: A clip associated with an editable document whose rendered sound
is refreshed from that document. Its placement, source interval and instance
processing belong to the clip.

**Effect stack**: An ordered set of editable, bypassable effects belonging to a
document or a clip, separate from structural edits and its gain/fade envelope.

**Gain/fade envelope**: An owner's overall gain together with its edge-anchored
fade-in and fade-out, shaping the source body without replacing its audio.

**Bounce**: A rendered audio snapshot of a document's source, envelope and effect
stack, including its tail, used as an arrangement source.

**Clip body**: The source interval placed on the timeline, excluding any trailing
effect decay. Trimming and automatic crossfade geometry use this interval.

**Trim intent**: The requested source interval of a clip, distinct from its
current interval when a shorter linked document temporarily clamps its bounds.

**Envelope intent**: The requested gain and fade durations of a clip, distinct
from its current envelope when a shorter linked source temporarily clamps fades.

**Effect tail**: The decaying output remaining after source input ends, extending
audible duration beyond the source body without moving subsequent clips.

**Render identity**: The identity of a document's audible result, covering its
source revision, envelope, effect stack, effect tempo and rendering policy.

**Sample saved baseline**: The editable source revision, processing settings and
Effect tempo whose rendered sound was last published by standalone Save. Its saved
render identity is separate from editable lossless PCM and the project baseline.

**Effect tempo**: The explicit project BPM used by tempo-synced effects,
independent of waveform selection and timeline ruler units.

**Clip edit revision**: The immutable identity of a clip's placement, trim and
instance processing state. A background edit may publish only against that exact
identity. Derived source refresh may invalidate this token without an edit-history
entry; the processing edit revision and audible render identity remain separate.

**Clip processing**: The instance's gain/fade envelope and ordered effect stack,
applied to its source interval. Its audible result can include a tail beyond the
body and is independent of other instances' processing.

**Prepared mix**: A prepared arrangement rendering plan with clip sources,
processing, latency alignment, tails, track/master controls and active timeline
segments, shared by playback and offline inspection.

**Effective track**: A track's current controls and whether it contributes under
the mute-over-solo rule. Mono pan and stereo balance depend on each clip's layout.

**Crossfade region**: The half-open project-frame intersection of two clips on
one track, with equal-power gains derived from their geometry. A contained clip
dips the containing voice and fades in/out. A third simultaneous body is invalid;
effect-tail overlap is separate from body overlap.

**Insertion order**: The stable ordering assigned when a clip is created, retained
when it moves, trims or returns through history. It resolves equal-start ordering
independently from UUID spelling.

**Project frame**: An integer arrangement time boundary at the project sample
rate, initially 48 kHz. Source-to-project duration conversion rounds once to the
nearest frame, with a minimum of one for a nonempty source interval.
