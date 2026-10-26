# Playback and the command seam

The GUI opens the default output device asynchronously using CPAL 0.18.2. A
device-free session still supports editing, cursor movement and `transport.get`;
it advertises playback commands only when an output adapter is configured.

Space toggles play/pause. Enter or comma rewinds without changing play state.
Slash toggles looping. A selection supplies loop bounds; otherwise the entire
sample loops. Starting outside those bounds begins at the loop start. Selection
does not limit ordinary playback. Playing again at the end restarts at zero.
Changing document or view pauses playback. Pause retains fractional resampling
position, and rewind retains the selection.

`transport.play` and `transport.play_pause` take `document_id`; `transport.pause`
takes no parameters. `transport.seek` takes `document_id` and an integer `frame`;
`transport.rewind` takes `document_id`. Seeking preserves the selection, unlike
placing the editing cursor. All mutations support expected session revisions.

`transport.get` returns output configuration/error, requested state, applied
state, handoff generation, pending status, callback count and the last block's
unclipped peak level. Requested `frame: null` means preserve the callback's exact
position. Applied position updates the visible cursor without creating undo or
session revisions every audio block. Wait until `pending` is false after pausing
before capturing a stable playback cursor. Capture metadata reports pending
transport handoffs.

A fixed-capacity SPSC queue transfers prepared immutable state to the renderer.
Replaced plans return through a second bounded queue for destruction on the
control thread. Under pressure, the control side retains the latest pending
request. The callback performs no filesystem work, blocking locks, allocation or
buffer destruction. Native and manually clocked output use the same renderer.
Rational integer phase steps avoid cumulative clock drift; linear interpolation
converts source rates. Mono duplicates to stereo; stereo channels are preserved.
Device conversion clamps output to [-1, 1], leaving working PCM untouched.

Initialization and recoverable device failures appear persistently in the status
area. `transport.output.retry` reopens the default device off the UI thread.
Playback never resumes automatically after a device error. Native dependencies
are CPAL's platform backends (Windows WASAPI, macOS CoreAudio, Linux ALSA); Linux
builds need ALSA development headers. No codec executable is used for playback.

Live document/clip processing precedes resampling, with compatible continuous
control slew, prepared20ms topology transitions and worker-side bounded context.
transport.get.processing reports requested/applied processing generations,
preparation errors, latency and active/capped tails. Pause preserves DSP state;
loop wrap reuses state and body bounds. A reset offline render does not reproduce
accumulated loop history or transient live smoothing. Multitrack mode entry pauses
and resolves unfinished previews, then requests current shared bounces. Play/export
also enforce freshness in the core. Mix edit revisions and required/prepared audible
render identities are distinct. [Renderer](processing-renderer.md) and
[final measurements](verification/live-effects-workflow/README.md).
