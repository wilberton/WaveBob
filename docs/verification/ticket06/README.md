# Ticket 06 verification

Verified on Windows, 23 October 2026. The importer supports mono/stereo PCM
16/24/32-bit and float32 WAV plus CBR/VBR MP3 through the same command layer.
[Backend/build details](../../audio.md) document the pinned pure-Rust decoder,
limits, metadata and gapless policy. [Owned fixtures](../../../tests/fixtures/audio/README.md)
include all four mono/stereo CBR/VBR combinations, MPEG-1/2 rates and metadata-free
streams, with independent FFmpeg decoded references.

Command tests verify every fixture's levels, actual frames, source encoding,
bitrate mode and delay metadata. They reject malformed/truncated input,
non-finite floats, unsupported channels and oversized declared audio without
partial publication. Close/reopen tests prove identities and waveform caches are
not reused. Direct, NDJSON headless and stdio MCP return exactly the same MP3
samples, including through JSON round trips. GUI interaction checks include
opening/switching real documents and the new close buttons.

The release app imported a stereo VBR MP3 through an attached MCP client and
returned an inspected [actual waveform capture](mp3.png), with identical PNG
bytes delivered as image content. [Metadata](mp3-metadata.json) records the job,
document and rendered revision.

## Measured large-file check

[Recorded measurements](large-import.json), using the release GUI and attached
stdio MCP client on this Windows host:

| Measurement | Result |
| --- | --- |
| Source | Ten minutes, 48 kHz stereo PCM16, 28,800,000 frames |
| Source / decoded PCM | 115.2 MB / 230.4 MB |
| Import including peak preparation/publication | 1.44 seconds |
| View-mode changes during import | 413 ms and 267 ms round trips |
| Resident memory before / loaded | 86.5 MiB / 310.3 MiB |
| Observed peak resident memory | 531.6 MiB |
| Full-range 1024-bin waveform query | 1.17 seconds round trip |
| Second import cancellation | Cancelled, existing document preserved |

Round trips include MCP, local IPC, serialization and hidden-window scheduling;
they are not per-frame render times. Process memory includes the renderer and
transient immutable-audio publication. These are measured baselines, not maximum
file-duration or memory guarantees. The test changes the live view while decoding,
observes job progress, queries cached peaks and cancels a second import before
publication. Worker progress is coalesced so long files do not queue one event per
sample. Source reads, decoding and peak construction run away from UI rendering.

Reproduce with the ignored `mp3_document_import_and_capture_through_mcp` and
`large_import_keeps_live_navigation_and_cancellation_responsive` tests in
`tests/mcp.rs`, using release executables. Set `WAVEBOB_GUI_TEST_BIN` and
`WAVEBOB_MCP_TEST_BIN` to those executables, and `WAVEBOB_TEST_ARTIFACT_DIR` to
the output folder. Ordinary tests use the checked-in fixtures and need no FFmpeg.

Interactive viewport/selection gestures follow in ticket 07; bounded waveform
queries already return exact frame-level data and the renderer can draw connected
sample points. Native macOS/Linux execution remains deferred until a remote exists.
