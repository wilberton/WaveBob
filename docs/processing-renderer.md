# Streaming processing renderer

`processing.render` asynchronously renders an explicit owner from committed state.
It rejects an active owner preview and can guard an expected render identity.
The completed job reports `processing_rendered` metadata: source body frames,
sample rate/layout, computational latency, audible tail frames, truncation and
render identity. `processing.samples` reads up to 4096 frames from the returned
immutable audio revision. These inspection renders are bounded to eight retained
results. They do not create documents or enter history.

Playback and offline rendering use the same `Chain` processor path at source rate.
Processing precedes the playback resampler. The document envelope feeds the ordered
stack. Mono reaches the stereo processor boundary as dual mono; output layout
declares whether stereo must be retained. Browser audition stays dry.

The processor contract declares frame processing, computational latency, stereo
generation, state activity, storage, reset and continuous-control state transfer.
Preparation creates all storage. A processor can retain unchanged state and adopt
new continuous targets without allocating in the callback; topology changes use
a 20 ms old/new transition. Overall envelope gain slews over 10 ms. A reset render
starts at its exact fixed settings, without baking transition smoothing into PCM.

The streaming cache processes each source frame once, then interpolates to the
device clock. Pause preserves that cache, fractional clock and processor state;
a 1 ms output ramp avoids an abrupt stop. Loop wrap supplies the next loop input
to existing state. A seek prepares reset context off the callback, bounded by the
60-second state lifetime, then removes computational latency once. Intentional
echo/predelay remains audible. Playback can drain a tail and resume a paused tail.

Heavy preparation has one worker and one coalescing task/result mailbox. Superseded
work checks cancellation every 1024 context frames. Continuous envelope gain can
reuse the applied topology immediately. Moving-transport preparation catches up
on the worker; callback catch-up is limited to 4096 source frames. Excess lag requests
another preparation and retains the previous audible graph. Preparation failure
is queryable in `transport.get.processing`; it does not replace the valid graph.
Requested/applied generations and pending/error, latency and tail status are explicit.

The callback uses bounded handoff/retirement queues of two packets each, at most
one outgoing transition, and preallocated processor storage. Buffers and graphs are
retired to the control thread. It performs no file access, blocking locks, allocation
or buffer destruction. Each chain allows 64 MiB of delay/state storage and at most
one second of computational latency. A shared 256 MiB lease budget bounds prepared
processing across graphs/sessions; each active preparation worker can temporarily construct one extra
chain before acquiring its lease. The session retains its existing 2 GiB audio limit;
an offline body plus tail must fit the 512 MiB PCM limit. Final arrangement body/tail
voice admission and protected bounce-cache eviction are completed by ticket 42.

Tail termination observes output and retained state below -100 dBFS for 100 ms.
An entirely silent path acquires no tail. Combined tails cap at 60 seconds after
latency removal and taper over their final 10 ms. Metadata reports capped tails.
Processors must bound retained-state activity, including silence inside a delay;
output amplitude alone cannot establish that a predelay has drained. Nonfinite
output/state becomes a visible render error and silence, never device NaNs.

All eleven production processors remain unavailable until their algorithm/editor
tickets register a verified constructor and parameter metadata. Test-only delay,
feedback and continuous-control probes are never advertised as shipping effects.
Actual-effect load/input-latency/dropout measurements are required by ticket 44.
The final linked arrangement refresh and sample/project export integration belong
to tickets 42 and 43; this foundation does not claim those workflows complete.

During this implementation checkpoint, sample Save/export explicitly rejects a
non-unity live processing state. It cannot silently write dry PCM or mark the
processed document saved. Use `processing.render`/`processing.samples` for current
render inspection. Rendered WAV/MP3 export is integrated by ticket 43. Sample and
project saves require previews to be committed or cancelled first.

## Arrangement streaming

Prepared mix graphs retain immutable source intervals and at most 128 clip voices.
Each processed voice runs the shared source-rate chain before project-clock
interpolation, with computational latency removed once per path. Trimmed ranges
use local envelope coordinates. Untouched mixed-rate clips reuse the existing
prepared sinc conversion. Owner processing does not require a whole-file wet
render for each preview update. A gain-only replacement that switches between the
static conversion and streaming path transitions over 10 ms; other topology
changes use 20 ms. Compatible voices transfer state and ramp their continuous
targets. Track/master controls remain outside owner processing.

Clip previews produce an audible graph identity without publishing an arrangement
edit or history entry. Superseded mix jobs are cancelled and still count against
the two-job admission limit until they retire. Preparation and streaming-context
catch-up occur off the callback. Mix inspection and export use the same streaming
reader and reject unresolved previews. Export may materialize its explicit interval
(up to 512 MiB) for high-quality output-rate conversion; live updates do not.
Closing a preview restores committed sound through the same handoff path.

Processing generation/error, maximum path latency and capped-tail reporting cover
mix streams as well as document playback. Final tail geometry and linked-document
bounce freshness remain ticket 42's responsibilities.
