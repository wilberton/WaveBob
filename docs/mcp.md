# MCP support

`wavebob-mcp` is a local stdio MCP server. It targets either one independent
GUI-free session or one explicitly selected running GUI. It uses the official
[Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk), pinned to 3.7.0
in the dependency lock, for framing, lifecycle/version negotiation and content
delivery. Contract tests cover the 2025-11-25 initialization handshake and the
2026-07-28 discovery lifecycle. No HTTP server or network listener is started.

## Build and configure

Build an executable without GUI dependencies:

```sh
cargo build --locked --release --no-default-features --bin wavebob-mcp
```

Configure your MCP client to launch the executable with no arguments for a new
independent session. For a running GUI, start WaveBob and discover its session UUID:

```sh
cargo run --locked -- --name Editing
cargo run --locked --no-default-features --bin wavebob-mcp -- --list-instances
```

Then configure the MCP subprocess with `--attach SESSION_UUID`. For example, in a
client accepting an `mcpServers` object (replace the executable path and UUID):

```json
{
  "mcpServers": {
    "wavebob": {
      "command": "C:\\path\\to\\WaveBob\\target\\release\\wavebob-mcp.exe",
      "args": ["--attach", "SESSION_UUID"]
    }
  }
}
```

On macOS/Linux use the absolute path to `wavebob-mcp` without `.exe`. A GUI restart
creates a new UUID; update the attachment argument and restart the MCP subprocess.
The server never silently chooses another instance or falls back to a new session
after a failed attachment. Names are display labels, not attachment identifiers.

`--runtime-dir DIRECTORY` or `WAVEBOB_RUNTIME_DIR` selects a separate local registry;
use the same value for the GUI, discovery and MCP server. `--list-instances` and
`--help` are standalone CLI operations. Normal server mode reserves stdout for MCP
messages; startup diagnostics go to stderr. The [command documentation](commands.md)
describes local IPC and registry behavior. `--hidden` on the GUI is useful for
renderer-backed automation; it still initializes the real renderer.

## Tools and arguments

Each command in the target's shared registry becomes an MCP tool with the same
name and description. Its typed parameter schema is nested under `params`.
The MCP output schema describes the complete shared response envelope, including
session identity, revision, result or error. Schema references remain resolvable
after nesting. No separate editing implementation or handwritten command list
exists in the adapter.

An independent session advertises nineteen tools: session/loop commands, document
open/get/activate/close, cursor/selection, exact samples/waveforms, jobs and
device-free transport inspection/seek/rewind.
An attached GUI also advertises
`presentation.get`, `presentation.set_mode`, `presentation.viewport.set`, `presentation.layout.set`,
`presentation.panels.toggle`, `presentation.barrier` and `capture.png`, plus four
device playback commands (thirty tools in total). Command descriptors explicitly
report `requires_audio_output`. See [playback](transport.md) and
[panel commands and preferences](layout.md).
Use `tools/list` for tool discovery or call `session.capabilities` for the full
application catalogue. There is one catalogue page and no list-change subscription.

MCP `tools/call` arguments use this envelope:

```json
{
  "name": "transport.loop.set",
  "arguments": {
    "params": {"enabled": true},
    "session_id": "SESSION_UUID",
    "expected_revision": 0
  }
}
```

`params` defaults to `{}` and is required by the advertised schema when the
command has required parameters. Unknown fields and invalid parameter types
are rejected by the shared command decoder. `session_id` is an optional extra
target guard; a mismatch rejects the request before dispatch. The fixed session
UUID is also reported in server instructions and every command result.
`expected_revision` has the same mutation guard semantics as the headless runner.
MCP request IDs are represented as strings in the inner command response; MCP's
outer response correlation remains managed by the SDK.

Audio imports return shared application job handles. Poll `jobs.get` and use
`jobs.cancel` for cancellation. Successful imports return document/revision IDs;
then query `documents.samples` or `documents.waveform` and capture the committed
GUI revision. No MCP Tasks capability is advertised; jobs use ordinary tools.
See [the command protocol](commands.md) for format limits and errors.

## Retrieve an inspectable screenshot

Call `presentation.set_mode` with `params: {"mode":"multitrack"}` and read its
presentation revision. Query `session.get` for the core revision, then use those
numbers in `capture.png`. This example assumes core revision 1 and presentation
revision 1:

```json
{
  "name": "capture.png",
  "arguments": {
    "session_id": "SESSION_UUID",
    "params": {
      "after_revision": 1,
      "after_presentation_revision": 1,
      "timeout_ms": 10000
    }
  }
}
```

The result contains a structured WaveBob response and a text copy for clients
that display text results. On successful capture it also contains an MCP image
block with `mimeType: "image/png"` and base64-encoded PNG data. The image bytes
come from the just-completed capture artifact. The structured response retains
the saved path and rendered session/core/presentation revisions, mode, frame
number, dimensions, scale and pending resources. The client can inspect the image
without separately reading the GUI machine's filesystem.

The same `path`, explicit `overwrite`, revision minimums and timeout rules as
the [capture command](commands.md#presentation-and-capture) apply. Omit `path` for
a unique artifact in the runtime directory. A render barrier is also available
without writing an image. Calls are dispatched on a worker and serialized per
MCP target so blocking IPC and image delivery leave the stdio reader available.

Inline PNG delivery is limited to 8 MiB before base64 encoding, keeping responses
below the SDK's default 16 MiB incoming-line limit. If image reading/delivery fails,
the tool reports `io_error` with the completed capture's path and metadata in
`error.details.capture`; the saved PNG remains available. Client-specific image
limits may be lower. The adapter reads only the artifact returned by a successful
capture, and does not expose arbitrary filesystem reads.

## Errors and verification

Domain failures use MCP `isError: true` while preserving the shared response's
stable error code, details, session and revision in structured/text content.
Examples include `stale_revision`, `invalid_params`, `timeout` and `output_exists`.
A known GUI-only command sent to an independent session returns `missing_capability`.
No image block is attached to a failed capture.

Malformed MCP argument envelopes and session-guard mismatches use JSON-RPC
`-32602` with a structured `code` in error data. An unavailable attached GUI uses
`-32603` with `code: "io_error"` and its session UUID, rather than inventing a
revision for an unreachable session. Subsequent valid requests remain usable
after application/parameter errors. Unsupported protocol methods and lifecycle
handling belong to the SDK.

```sh
cargo test --locked --no-default-features --test mcp
cargo test --locked --test mcp -- --include-ignored --nocapture
```

The second command starts two real GUI instances and an official SDK MCP client
subprocess. It checks isolation, equivalent loop/mode state through direct IPC,
headless CLI and MCP, render barriers, PNG delivery/decoding, revision metadata,
overwrite refusal, timeout recovery and loss of the target. Set
`WAVEBOB_TEST_ARTIFACT_DIR` to retain the images decoded from MCP image blocks.
`WAVEBOB_MCP_TEST_BIN` and `WAVEBOB_GUI_TEST_BIN` optionally point these tests at
release executables. Native Windows capture tests need normal file permissions
for atomic PNG publication. Native macOS/Linux checks remain deferred until a
remote exists, as requested.

Protocol references: [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
and [stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports).

The [ticket 03 verification record](verification/ticket03/README.md) contains
screenshots retrieved through MCP from the running release app.
