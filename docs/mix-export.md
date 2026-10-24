# Arrangement output

In multitrack mode, choose File → Export audio. Choose the timeline selection or
complete arrangement, WAV encoding or MP3 bitrate, output rate and destination.
The dialog shows exact project frame bounds, time, converted frame count and
stereo layout. New exports default to 24-bit WAV. Overwrite and clipping require
explicit intent, using the same controls and encoder as sample output.

`mix.export` takes revision_id, start_frame, end_frame, path, optional format and
sample_rate, overwrite and overload. Bounds are a nonempty half-open project-frame
interval within the 24 hour timeline. They may include silent time. The command
captures the requested arrangement and its immutable source/processed revisions,
returning a background job without requiring a window or audio device.

Source resampling, instance processing, equal-power crossfades, mute-over-solo,
mono pan/stereo balance and track/master gain use the same prepared mixer as live
playback and `mix.samples`. Output is always stereo. The shared resampler converts
the final project-clock mix to the requested file rate, without an intermediate
waveform/peak cache. Mixed output is bounded to 512 MiB of PCM; choose a shorter
range for larger output. Retained session/preparation/output PCM obeys the existing
2 GiB budget.

The selected interval ends exactly at end_frame. Reverb resets/ends within each
clip; no tail extends beyond a clip or the export interval. Output duration rounds
to the nearest output-rate frame, with a one-frame minimum for a nonempty range.
Floating-point headroom is preserved by float WAV. Integer WAV/MP3 rejects overloads
unless overload is explicitly `clip`. Open documents' backing paths are protected
from plain sample/mix export; use document Save to replace a source intentionally.

The job uses the shared atomic temporary-file publication/cancellation mechanism.
Later arrangement edits do not alter the captured render and do not mark a newer
arrangement saved. This snapshot-export policy differs from document Save, which
rejects a stale source before publication. Cancellation before publication cleans
temporary output; once publication starts, cancellation reports busy. Existing
destinations require overwrite true.

`jobs.get.output` has kind audio_export. Its details report the captured
source_revision_id, source_start_frame/source_end_frame, rates, intended/output
frames, settings, bytes, lossiness, overload/clipping counts and MP3 delay/padding.
MP3 reuses the bundled encoder: 128/192/256/320 kbit/s (default 192), and 32/44.1/48
kHz. No external codec executable is needed by the application.

See [sample output](export.md), [mixing](mixing.md), [crossfades](crossfades.md) and
[encoder/build/license details](mp3-encoder.md).
