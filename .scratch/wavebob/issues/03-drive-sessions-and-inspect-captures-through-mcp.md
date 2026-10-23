# 03: Drive sessions and inspect captures through MCP

**What to build:** An MCP client discovers commands, operates a headless or
attached GUI session, queries results and receives inspectable screenshots.

**Blocked by:** 02 — Attach to the GUI and capture its actual pixels.

**Status:** ready-for-agent

- [ ] Stdio MCP tools expose catalogue, execution, queries, job results and capture;
  diagnostics do not corrupt protocol output.
- [ ] Shared command schemas supply tool descriptions and validation; additional
  registered editing capabilities require no duplicate editing implementation.
- [ ] Session identity is explicit. Errors retain their structured meaning, and
  captures return image content or a readable resource plus revision metadata.
- [ ] Contract tests compare a loop/mode scenario with direct and headless adapters,
  and an MCP client can view a fresh capture of the attached GUI.

## Specification

[WaveBob editor specification](../spec.md).
