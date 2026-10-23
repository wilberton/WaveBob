# Shared commands and headless execution

Ticket 01 implements session inspection, command discovery and loop state only.
Playback, file editing, MCP attachment and screenshots arrive in later tickets.
The GUI loop button and the headless runner dispatch through the same `Session`.
Snapshots are owned read models; views cannot change core state directly.

## Run

```sh
cargo run --locked
cargo run --locked --no-default-features --bin wavebob-headless
```

The second command builds without egui, eframe, windowing or audio dependencies.
Every process starts a separate session. Attaching to the GUI belongs to ticket
02; launching a headless process does not change an already running window.

For a release executable:

```sh
cargo build --locked --release --no-default-features --bin wavebob-headless
```

Run `wavebob-headless --help` for usage. Unknown command-line arguments return
exit code 2 and write diagnostics to stderr. Normal protocol use prints no banner.

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

Discovery reports protocol version, capability names, the request/response
envelope schemas and per-command descriptions, mutation classification, input
schemas and result schemas. These schemas use JSON Schema Draft 2020-12. A
command's output schema describes `result`, not the entire response envelope.
The typed parameter/result models generate the schemas used by the registry;
adapters do not maintain another catalogue.

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
Errors leave state and revision unchanged.

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
CI performs these checks and release builds on Windows, macOS and Linux.
