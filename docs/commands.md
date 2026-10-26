# Shared commands and headless execution

Tickets 01–19 and 22 implement shared session commands, portable projects, audio import/output, selection, editing/history, arrangement/clip processing and mixing/crossfades/export, playback, browsing, GUI attachment, presentation
commands, actual framebuffer PNG capture and a [stdio MCP adapter](mcp.md).
See [editing and history](editing.md) and [playback commands](transport.md). All adapters share command
definitions and core behavior. Snapshots are owned read models; views cannot
change core state directly.

## Run

```sh
cargo run --locked
cargo run --locked --no-default-features --bin wavebob-headless
```

The second command builds without egui, eframe, windowing or an audio device.
By default, every process starts a separate session. Use explicit GUI attachment
below to control a running window from the GUI-free runner.

For a release executable:

```sh
cargo build --locked --release --no-default-features --bin wavebob-headless
```

Run `wavebob-headless --help` for usage. Unknown command-line arguments return
exit code 2 and write diagnostics to stderr. Normal protocol use prints no banner.

## Attach to a running GUI

Start the GUI with an optional display name:

```sh
cargo run --locked -- --name Editing
```

In another terminal, list live instances, then attach using the exact session UUID
returned by discovery. Names are display labels and need not be unique.

```sh
cargo run --locked --no-default-features --bin wavebob-headless -- --list-instances
cargo run --locked --no-default-features --bin wavebob-headless -- --attach SESSION_UUID
```

Send the normal JSON requests on stdin after attachment. These operate on the
selected window's session, including its real loop state and active mode. The
client verifies session identity on every response; no instance is silently chosen.
The MCP adapter uses the same explicit attachment mechanism.

The registry defaults to the current user's runtime/cache directory. For isolated
workspaces/tests, pass `--runtime-dir DIRECTORY` to both executables or set
`WAVEBOB_RUNTIME_DIR`. Discovery lists only authenticated live endpoints; normal
shutdown removes the instance record, and stale records after a forced exit are
excluded. Windows uses named pipes; Unix uses local sockets. No network port is opened.

GUI `--hidden` starts a real renderer-backed window without showing it. Commands
can still request painting and capture; this is distinct from a GUI-free session,
which never initializes a renderer. Failed startup or unavailable transport exits
the client with an stderr diagnostic rather than fabricating a core response.

## Framing and requests

Send one UTF-8 JSON object per line on stdin. Each nonblank frame produces one
JSON response line on stdout, flushed immediately without waiting for EOF. Both
LF and CRLF are accepted; the last frame may end at EOF without a newline. Blank
lines are ignored. Malformed JSON/UTF-8 or rejected commands produce errors and
leave the stream open for subsequent requests. I/O failure exits unsuccessfully;
a downstream closed pipe exits quietly. Application errors are protocol responses,
not process failures. There is no persistence across process restarts yet.

```json
{"id":"discover","command":"session.capabilities"}
{"id":"before","command":"session.get"}
{"id":"set","command":"transport.loop.set","params":{"enabled":true},"expected_revision":0}
{"id":"after","command":"session.get"}
{"id":"toggle","command":"transport.loop.toggle","expected_revision":1}
```

- `id` is a required nonblank string echoed in the response. Callers choose it;
  correlation IDs do not provide replay deduplication.
- `protocol_version` defaults to `1`; any other version is rejected.
- `command` is a registered wire name. Discovery lists only implemented commands.
- `params` defaults to `{}`. It must be an object matching that command's schema;
  unknown fields and incorrect types are rejected.
- `expected_revision` is an optional unsigned integer. It applies to mutations;
  queries ignore it and return the latest snapshot. A mismatched mutation fails
  before changing state, even if its desired value already matches current state.
- Unknown envelope fields, missing required fields and invalid field types fail.

## Available commands

| Command | Params | Mutates | Successful result |
| --- | --- | --- | --- |
| `session.get` | `{}` | No | Session snapshot |
| `session.capabilities` | `{}` | No | Catalogue and JSON schemas |
| `transport.loop.set` | `{"enabled":true}` | Yes | Session snapshot |
| `transport.loop.toggle` | `{}` | Yes | Session snapshot |
| `documents.open` | `{"path":"/path/sample.wav"}` | Job submission | Import job |
| `documents.get` | `{"document_id":"UUID"}` | No | Document metadata, revision, cursor and selection |
| `documents.activate` | `{"document_id":"UUID"}` | Yes | Active document |
| `documents.close` | Document ID, optional `discard_changes` | Yes | Closed document metadata |
| `documents.cursor.set` | Document ID and frame | Navigation | Document state |
| `documents.selection.set` | Document ID and start/end frames | Navigation | Document state |
| `documents.selection.all` / `.clear` | Document ID | Navigation | Document state |
| `documents.samples` | Document ID, optional revision ID, start frame, 1–4096 frames | No | Exact decoded channel samples |
| `documents.waveform` | Document ID, optional revision ID, interval, 1–2048 bins | No | Exact minimum/maximum per channel and bin |
| `jobs.get` | `{"job_id":"UUID"}` | No | Progress, status, result or structured error |
| `jobs.cancel` | `{"job_id":"UUID"}` | Job cancellation | Job status |
| `presentation.get` | `{}` | No | Presentation snapshot; GUI only |
| `presentation.set_mode` | `{"mode":"multitrack"}` | Presentation | Presentation snapshot; GUI only |
| `presentation.viewport.set` | Document ID, visible start/end and optional presentation revision guard | Presentation | Presentation snapshot; GUI only |
| `presentation.layout.set` | Optional widths, visibility, library fraction and revision guard | Presentation | Presentation snapshot; GUI only |
| `presentation.panels.toggle` | Optional presentation revision guard | Presentation | Presentation snapshot; GUI only |
| `presentation.barrier` | Minimum revisions and timeout | No | Render metadata; GUI only |
| `capture.png` | Destination, overwrite intent, minimum revisions and timeout | No | PNG artifact and render metadata; GUI only |

Discovery reports protocol version, capability names, the request/response
envelope schemas and per-command descriptions, mutation classification, input
schemas and result schemas. These schemas use JSON Schema Draft 2020-12. A
command's output schema describes `result`, not the entire response envelope.
The typed parameter/result models generate the schemas used by the registry;
adapters do not maintain another catalogue.

The independent GUI-free session advertises sixteen commands. GUI
attachment advertises all twenty-three. Sending a known GUI-only command to an independent
session returns `missing_capability`, leaving its state unchanged. See [panels and
status-bar help](layout.md) for layout commands, preferences and keyboard behavior.
See [waveform controls](waveform.md) for precise selection and viewport semantics.

## Presentation and capture

Imports run off the UI/command thread and return a job immediately. Poll `jobs.get`
until `succeeded`, `failed` or `cancelled`; successful results contain the new
document and revision UUIDs. `session.get` also lists documents, jobs and the active
document. Only completed imports publish audio and advance the session revision.
Cancellation before publication discards the result; terminal cancellation is
idempotent. The importer caps decoded audio at 512 MiB per file and runs at most
two imports at once (`busy` otherwise). Sources
are never modified. Empty, corrupt, missing, inaccessible and unsupported files
fail the job with a stable error and leave existing documents usable. All specified
WAV encodings and CBR/VBR MP3 are supported; see [audio details](audio.md) for
formats, delay/padding handling and measured resource limits.

Waveform extrema use a revision-local multiresolution cache and exact edge samples;
small sample queries return decoded values rather than display peaks. Both accept
an optional revision guard. A stale audio revision returns `stale_revision`.
Frame ranges are half-open. A screenshot can wait for the successful import's
committed revision; `pending_resources` lists import jobs that are still running.

`presentation.get` returns `mode` (`waveform` or `multitrack`) and its own
`revision`. Set mode with `presentation.set_mode`; optional
`expected_presentation_revision` rejects stale view changes. Re-selecting the same
mode is a no-op. Presentation changes do not advance the core session revision
and do not create audio undo history. Envelope `expected_revision` still checks
the core revision for mutations; use the presentation field to guard view changes.

For example, after querying the relevant revisions:

```json
{"id":"mode","command":"presentation.set_mode","params":{"mode":"multitrack","expected_presentation_revision":0}}
{"id":"frame","command":"presentation.barrier","params":{"after_revision":0,"after_presentation_revision":1,"timeout_ms":10000}}
{"id":"image","command":"capture.png","params":{"path":"multitrack.png","overwrite":false,"after_revision":0,"after_presentation_revision":1,"timeout_ms":10000}}
```

Both render commands accept `after_revision` and `after_presentation_revision` as
minimums; omitted values mean the state at command acceptance. They wait until a
frame representing those revisions is actually read back from egui. The reported
revision can be newer than the requested minimum. Requests time out explicitly;
future revision numbers do not create artificial state changes.

`timeout_ms` defaults to 10000, with a valid range of 1–60000. The IPC client allows
an additional two seconds for transport. Transport failure is distinct from a
GUI-render timeout, which is a structured command error.

`presentation.barrier` returns render metadata after actual readback, without
writing an image. `capture.png` exports the complete client area: all currently
implemented panels, central view, transport and status bar. Native window chrome
and dialogs are outside capture.

Successful capture returns `path`, `mime_type: "image/png"` and `metadata` containing
session ID, rendered core revision, presentation revision, active mode, frame
number, actual pixel width/height, scale and pending resources. The envelope's
revision refers to the captured state, even if later commands have already run.

The host fences state changes through framebuffer readback. PNG encoding and file
publication run on a worker after the pixels arrive, so later commands cannot
change the image or its metadata. File publication is atomic; an existing path is
preserved unless `overwrite` is explicitly true. Failed/timed-out output cleans
its temporary file. Custom parent directories must exist. With no `path`, capture
creates a unique PNG under the selected runtime directory's `captures` folder.
Relative paths resolve against the GUI process's working directory.

IPC frames are limited to 2 MiB. PNG bytes are stored in the reported artifact,
not embedded in command JSON. The MCP adapter additionally delivers the artifact's
PNG bytes as image content with the same response metadata.

## Responses and state

Every response includes `protocol_version`, `id`, `session_id`, `revision` and
`status`. Success has `status: "ok"` and `result`. Failure has `status: "error"`
and `error` containing `code`, `message` and optional structured `details`.
If a valid string ID cannot be recovered from a malformed frame, `id` is null.

A snapshot contains `session_id`, `revision` and `loop_enabled`. Each session has
a new UUID, revision zero and looping disabled initially. Queries never advance
the revision. Setting an unchanged value succeeds without advancing it; each
actual state change advances it once. Revision exhaustion fails before mutation.
No-op sets and queries do not create fictional edit-history entries.

For example, a stale mutation reports:

```json
{"protocol_version":1,"id":"stale","session_id":"<session UUID>","revision":1,"status":"error","error":{"code":"stale_revision","message":"Session changed since the expected revision","details":{"expected_revision":0,"actual_revision":1}}}
```

Stable error codes are `invalid_json`, `invalid_request`, `unknown_command`,
`invalid_params`, `unsupported_protocol`, `stale_revision` and
`revision_exhausted`. Messages are explanatory and may evolve; branch on codes.
Desktop commands also use `missing_capability`, `timeout`, `render_failed`,
`output_exists` and `io_error`. Errors leave core/presentation state unchanged.

## Verify

```sh
cargo fmt --all --check
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo test --locked --no-default-features --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

Command tests cover adapter equivalence, no-op revision semantics, stale writes,
strict validation, capabilities and snapshot isolation. Process tests exercise
real stdin/stdout exchange and require a response while stdin remains open.
The GUI action test compares its command dispatch with headless state changes.
CI is configured for these checks and release builds on Windows, macOS and Linux.
Native macOS/Linux verification is deferred until a remote exists, as requested.

Run the real two-instance GUI integration test explicitly on a workstation with
a native renderer:

```sh
cargo test --locked --test gui_attachment -- --ignored --nocapture
```

It verifies isolation, mode changes, actual PNG pixels, matching revision metadata,
render barriers, timeout recovery, overwrite protection, stale endpoints and a
concurrent mode/capture race. Set `WAVEBOB_TEST_ARTIFACT_DIR` to retain captures for
inspection. The test is excluded from normal display-free test runs. Restricted
Windows sandboxes can block temporary-file attribute changes during atomic PNG
publication; native verification must allow these normal file operations.

The [ticket 02 verification record](verification/ticket02/README.md) includes
inspected PNGs and metadata from the release GUI controlled by the GUI-free runner.

Live effects are queried through processing.catalogue/get and changed through
processing.edit or preview begin/update/commit/cancel. Explicit document/clip owners,
processing edit revisions and audible render identities prevent accidental retargets.
All11 catalogue entries ship DSP and native editors. effects.editor.open/close/focus/
geometry and capture.png editor_id operate on stable instance bindings. processing.render
and processing.samples inspect committed wet PCM without changing source audio.

mix.get exposes required/prepared render identities and protected bounce-cache
bytes/entries. mix.prepare, play and export share current linked full/trim preparation.
processing.make_independent accepts explicit keep_prior_source:true when refresh
fails. Source/processing snapshots remain independent. Project schema2 saves current
lossless assets and editable stacks; audio.export/documents.save render sound.
Output channels:1 explicitly averages processed stereo, channels:2 retains stereo;
mix.export include_tail:true includes the current contributing audible end. See
[live processing](live-processing.md), [sample output](export.md), [projects](projects.md)
and [complete workflow evidence](verification/live-effects-workflow/README.md).
