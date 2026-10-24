# File drops and continuous waveform rendering

Verified on Windows on 24 October 2026. Implementation: `783f6cc`.

Native file events now enqueue paths through `documents.open`; the shared core
owns decoding and publication. WAV/MP3 batches obey the two-worker limit, retain
errors while continuing, and consume each event once. Hover feedback and help
use the existing theme and status bar. The open-file panel offers cancellation
of unstarted paths. Dialogs and captures defer imports; workspace close clears
unstarted paths.

The supplied dotted-waveform problem was reproduced with flat PCM represented
by two-frame extrema bins: the old renderer's filled mesh had zero area and the
trace disappeared at column 22. The equivalent individual-sample trace passed.
Antialiased outlines around the measured extrema fix this without changing
samples, peak queries or their positions. Both regression cases pass.

## Native renderer and MCP

The QA-only `gui-review` executable uses the production app and renderer with
deterministic `egui::RawInput` file events. No test input channel is present in
the shipped GUI or command API. This exercises the native window's egui event
boundary; it does not automate a physical Explorer drag gesture. Windows drag
and drop is explicitly enabled on the production viewport.

At 100% and 150% scale, a hover capture left the session empty. A single batch
containing three WAVs (including an uppercase extension and Unicode path) and
one MP3, dropped from multitrack mode, opened four documents exactly once and
created no clips. The test checked the running-worker limit and MP3 metadata.
Actual PNGs came back as MCP image content and matched the captured files byte
for byte. Both stereo channels were inspected at whole, intermediate and
individual-sample zoom:

| Scale | Hover | Whole sample | Intermediate | Individual samples |
| --- | --- | --- | --- | --- |
| 100% | [PNG](scale100/file-drop-hover.png) | [PNG](scale100/waveform-whole.png) | [PNG](scale100/waveform-intermediate.png) | [PNG](scale100/waveform-samples.png) |
| 150% | [PNG](scale150/file-drop-hover.png) | [PNG](scale150/waveform-whole.png) | [PNG](scale150/waveform-intermediate.png) | [PNG](scale150/waveform-samples.png) |

The [100%](scale100/file-drop-waveforms.json) and
[150%](scale150/file-drop-waveforms.json) records include session/job results,
rendered revisions, dimensions, scale and pending resources.

Focused egui tests also cover unsupported and missing files, existing-document
preservation, one-shot input, modal deferral, queued-path cancellation on close,
hover removal and capture deferral. The capture test sends a real IPC request,
checks that the frozen frame retains the drop without mutating the session,
then verifies import resumes after the painter-only harness's capture timeout.

The isolated committed source passes `cargo fmt --all --check`,
`cargo clippy --locked --offline --all-targets -- -D warnings` and
`cargo test --locked --offline --all-targets`: 131 distinct ordinary tests pass,
with the 37 GUI tests also passing in the example harness. Renderer/codec tests
marked ignored retain their explicit opt-in requirements; the new native file
drop test was run explicitly at both scales.

The rebuilt production release GUI also passes the native MCP exact-selection,
zoom and sample-detail test. Its [zoomed stereo capture](release/waveform-zoomed.png),
[sample detail](release/waveform-samples.png) and
[single frame](release/waveform-single-frame.png) are renderer captures from
`target\release\wavebob.exe`, built from the isolated `783f6cc` source snapshot.
The executable's SHA-256 is
`ef0dc2897734b047287ccc02a6917497dcbdf58dab1a3bb1420d36787dad390b`.

## Reproduce

Build the `gui-review` example, then set `WAVEBOB_GUI_TEST_BIN` to its executable
and `WAVEBOB_MCP_TEST_BIN` to `wavebob-mcp`. Run:

```powershell
cargo test --locked --test mcp native_file_drop_batch -- --ignored --nocapture
```

Set `WAVEBOB_DROP_TEST_SCALE=1.5` for the second scale and optionally set
`WAVEBOB_TEST_ARTIFACT_DIR` to retain the images and metadata. The existing
versioned distribution ZIP predates this change; launch the rebuilt local
`target\release\wavebob.exe` for these improvements.
