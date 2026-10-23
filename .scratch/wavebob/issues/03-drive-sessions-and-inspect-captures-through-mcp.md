# 03: Drive sessions and inspect captures through MCP

**What to build:** An MCP client discovers commands, operates a headless or
attached GUI session, queries results and receives inspectable screenshots.

**Blocked by:** 02 — Attach to the GUI and capture its actual pixels.

**Status:** done

- [x] Stdio MCP tools expose catalogue, execution, queries, job results and capture;
  diagnostics do not corrupt protocol output.
- [x] Shared command schemas supply tool descriptions and validation; additional
  registered editing capabilities require no duplicate editing implementation.
- [x] Session identity is explicit. Errors retain their structured meaning, and
  captures return image content or a readable resource plus revision metadata.
- [x] Contract tests compare a loop/mode scenario with direct and headless adapters,
  and an MCP client can view a fresh capture of the attached GUI.

## Specification

[WaveBob editor specification](../spec.md).

## Implementation and verification

Completed on 23 October 2026. `wavebob-mcp` uses the official Rust MCP SDK for
stdio transport and protocol negotiation. Each registered command supplies a tool
with shared descriptions/schemas and dispatches through the same application
interface. The process owns an independent headless session or attaches to one
explicit GUI UUID, with an optional per-call identity guard. Results retain the
shared response envelope and structured errors; successful captures also deliver
PNG image content. Registered job queries/results use this same generic path
when added to the core; no unimplemented audio job tools are advertised.

Formatting, both warning-free Clippy configurations, all 18 ordinary GUI-enabled
tests and all 16 GUI-free tests passed on Windows x86-64 with Rust 1.99.1/MSVC.
Release builds passed for the GUI, headless runner and GUI-free MCP server.
Contract tests exercised both the 2025-11-25 and 2026-07-28 MCP lifecycles. The
native MCP test passed against two running development GUIs and again with the
release GUI and GUI-free release MCP executable. Direct IPC, headless CLI and MCP
queries agreed; the untouched instance remained isolated. Both fresh 1440×900
screenshots were retrieved from MCP image blocks, decoded, compared with captured
PNG bytes and inspected visually. Timeout/error recovery, overwrite refusal and
target termination were also verified.

See [MCP setup and tools](../../../docs/mcp.md) and the
[verification record with retrieved screenshots](../../../docs/verification/ticket03/README.md).
Implementation commit: `237cf11`. Native macOS/Linux checks remain deferred until
a remote exists, as authorized by the user. Inline image delivery is limited to
8 MiB; a delivery error retains the saved capture reference and metadata.
