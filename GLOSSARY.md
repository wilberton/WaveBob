# WaveBob

Waveform editing and arrangement use distinct document, audio and view identities.

## Language

**Audio document**: An open sample with its own identity, working audio revision,
cursor and selection; its source file is a separate object.
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

**History entry**: One committed audio edit with the before and after states
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
source interval, positioned on a track in project frames. Closing or editing the
source document leaves the instance intact.

**Project frame**: An integer arrangement time boundary at the project sample
rate, initially 48 kHz. Source-to-project duration conversion rounds once to the
nearest frame, with a minimum of one for a nonempty source interval.
