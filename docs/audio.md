# Audio import and waveform data

Open files with the native Open action, the library's `+`, or `documents.open`.
Click a document row to activate its waveform; `×` or `documents.close` closes it.
Imports do not alter their sources. Empty, invalid, non-finite or unsupported
audio fails as a structured job error and leaves other documents usable.

Supported encodings:

| Source | Rates/channels | Working samples |
| --- | --- | --- |
| WAV signed PCM 16/24/32-bit | Source rate, mono/stereo | Float32, normalized by 2^(bits−1) |
| WAV float32 | Source rate, mono/stereo | Finite values retained, including values outside ±1 |
| MPEG Layer III MP3 | MPEG-1/2/2.5 rates, mono/stereo, CBR/VBR | Float32 with metadata-directed delay/padding removal |

WAV uses locked Hound 3.5.1. MP3 uses locked
[Symphonia 0.6.1](https://github.com/pdeljanov/Symphonia) with its MP3 and ID3
features. Symphonia is pure Rust under MPL 2.0; no external codec executable,
runtime DLL download or audio output device is needed. Cargo.lock records all
build inputs for desktop, GUI-free headless and MCP builds. Its
[decoding guide](https://github.com/pdeljanov/Symphonia/blob/main/GETTING_STARTED.md)
describes gapless decoding, enabled by default in this version.

Document metadata exposes original encoding, actual decoded frame count/rate,
channels and `source_details`: detected bitrate mode, constant bitrate when
available, and optional decoder-adjusted encoder delay/padding. These values come
from stream headers. Tagged fixtures recover the original 8192 frames exactly;
without gapless metadata all 10368 decoded frames remain. Silence is never guessed
away. Delay values include the decoder adjustment used by the backend.

A bounded structural MP3 pass checks complete MPEG frame geometry and available
Xing/Info/VBRI frame counts before decoding. This prevents a demuxer's normal EOF
handling from silently accepting a partial last frame. ID3 sizes follow the
[ID3v2 structure](https://id3.org/id3v2.4.0-structure). Free-format MP3, rate/channel
changes within one stream and unsupported channels are explicit errors. A stream
cut exactly at a complete frame without duration metadata is indistinguishable
from a shorter valid stream; no missing duration is invented.

Jobs decode and build multiresolution extrema on worker threads. Only completed
jobs publish a document/revision on the serialized command thread. Progress
notifications are coalesced; cancellation discards results before publication.
At most two imports run concurrently. Decoded PCM is capped at 512 MiB per import
and 2 GiB per session; close documents to release their current revisions/caches.
These are PCM limits, not total-process memory promises: decoding, immutable
publication, peak caches, renderer and font storage add overhead.

Waveform queries use immutable revision-local min/max pyramids plus exact boundary
samples, with 1–2048 bins over a half-open interval. Queries for individual frames
return actual samples; the renderer connects sample points at sufficient detail.
Interactive selection/viewport gestures are ticket 07. Closing/cancelling and
reopening cannot attach old peaks to a new document, because identities are never
reused. `documents.samples` reads at most 4096 actual frames per call. JSON parsing
enables exact float round trips so direct, headless and MCP results agree.

[Owned fixtures](../tests/fixtures/audio/README.md) include independent FFmpeg
decode references and a reproducible generator. FFmpeg is test tooling only;
checked-in tests need no FFmpeg installation. See
[ticket 06 verification](verification/ticket06/README.md) for measured results.
