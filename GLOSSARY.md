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
