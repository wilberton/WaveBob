# Ticket 03 verification

Verified on 23 October 2026 on Windows x86-64 with Rust 1.99.1/MSVC and the
actual egui OpenGL renderer. Implementation: commit `237cf11`.

## MCP client against the running app

The test starts two renderer-backed GUI instances with different names in an
isolated registry. An official Rust SDK MCP client launches `wavebob-mcp` as a
separate stdio subprocess, explicitly attached to the first session UUID. The
test ran against development executables and again against the release GUI and
the GUI-free release MCP executable.

The client discovered eight tools generated from the application's command
registry, enabled looping, switched to multitrack and then waveform mode, waited
for rendered revisions, and called `capture.png`. Its results contained both the
shared structured command response and an MCP `image/png` content block. The test
decoded that base64 image content, checked that its bytes matched the just-saved
capture, decoded the PNG and compared dimensions with revision metadata. These
retained images were written from the MCP-delivered image bytes and inspected:

- [Waveform image](mcp-waveform.png)
- [Multitrack image](mcp-multitrack.png)
- [Capture metadata and delivered byte counts](mcp-metadata.json)

Both images are 1440×900 at scale 1.0. They show the current demo-data shell;
audio editing and the mockup-matching pass are later tickets. The second GUI
remained unchanged. Session/loop and presentation queries through direct local
IPC, the attached headless CLI and MCP agreed. Existing-file capture returned
`output_exists` without an image, an unreachable future revision timed out, a
subsequent query succeeded, and terminating the target returned a structured
transport error without switching to the remaining GUI or a new headless session.

## Checks

All checks below passed using locked dependencies (`--offline` was also used
after fetching dependencies):

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --no-default-features --all-targets
cargo test --locked --test mcp -- --include-ignored --nocapture
cargo build --locked --release --bin wavebob
cargo build --locked --release --no-default-features --bin wavebob-mcp --bin wavebob-headless
```

The default suite passed 18 ordinary tests; the GUI-free suite passed all 16
applicable tests. The separately invoked native MCP integration test also passed.
The pre-existing ticket 02 native test remains opt-in; its verification record
is [here](../ticket02/README.md).

MCP contract tests exercise both the 2025-11-25 handshake and 2026-07-28 discovery
lifecycle using the official client SDK. They compare equivalent loop changes,
no-op and stale writes against direct typed commands and the headless JSON
adapter; check generated catalogue/schema consistency and local schema
references; verify missing renderer capability, parameter errors, target guards,
error recovery and stdout protocol framing. Invalid CLI arguments or attachment
targets fail on stderr without starting a replacement session.

To repeat the live test with release executables in PowerShell, after building:

```powershell
$env:WAVEBOB_MCP_TEST_BIN = Join-Path $PWD 'target\release\wavebob-mcp.exe'
$env:WAVEBOB_GUI_TEST_BIN = Join-Path $PWD 'target\release\wavebob.exe'
$env:WAVEBOB_TEST_ARTIFACT_DIR = Join-Path $PWD '.tools\ticket03-release-captures'
cargo test --locked --test mcp -- --include-ignored --nocapture
```

Ordinary tests need no renderer. The opt-in native test needs a graphics device
and normal Windows permissions for atomic PNG publication. Native macOS/Linux
checks and remote CI remain deferred until a remote exists, as requested. CI is
configured to build both GUI-free runners when that remote is available.

See [MCP setup, tools and error behavior](../../mcp.md). New registered commands,
including future job queries, automatically become tools; no audio jobs or MCP
Tasks capability are advertised in this milestone. Inline PNG delivery currently
has an 8 MiB limit, with the completed artifact retained if delivery fails.
