# 01: Command-driven session and headless runner

**What to build:** Toggle loop and inspect session state from either the
existing GUI or a headless JSON session using the same core implementation.
This makes the architectural separation executable before adding audio edits.

**Blocked by:** None (can start immediately).

**Status:** done

- [x] GUI loop state is read from a core snapshot; its button dispatches a command.
- [x] A GUI-free build starts without graphics or an audio device, accepts framed
  JSON commands/queries, and returns request IDs and session revisions.
- [x] A discoverable schema registry describes the initial capabilities. Unknown
  commands, malformed parameters and stale mutations fail without changing state.
- [x] A command-level test demonstrates GUI/headless equivalent loop state. The
  existing shell still launches and no audio capability is falsely advertised.
- [x] Establish formatting, lint and build checks, generate the dependency lock,
  and verify Windows locally. Native macOS/Linux CI is deferred until a remote
  exists, as explicitly authorized by the user on 23 October 2026.

## Specification

[WaveBob editor specification](../spec.md).

## Implementation and verification

Implemented on 23 October 2026. The shared core owns session identity, revision
and loop state; typed GUI requests and the newline-delimited JSON runner use the
same dispatcher. The registry generates command discovery and schemas from the
typed command definitions. Only session inspection/discovery and loop set/toggle
are advertised. No audio capability, MCP attachment or screenshot implementation
is claimed by this slice.

Verified locally on Windows x86-64 with Rust 1.99.1 and the MSVC toolchain:

- Formatting check passed.
- Clippy with warnings denied passed with default features and with GUI disabled.
- All 11 tests passed with default features; all 10 applicable tests passed with
  GUI disabled. Tests include actual subprocess stdin/stdout exchange before EOF,
  malformed-frame recovery, strict parameters, stale writes, no-op revisions,
  discovery schemas and GUI/headless command equivalence.
- Locked release builds passed for the standalone headless runner and GUI.
- The release runner successfully discovered four commands, set loop state and
  queried the resulting revision. The release GUI created a responding native
  window and closed cleanly during its startup smoke check.
- The normal dependency graph with GUI disabled contains no egui/eframe/windowing
  or audio dependencies. The committed lockfile pins resolved dependencies.
- CI now runs lint, tests and release builds for GUI and headless configurations
  on Windows, macOS and Linux.

The user explicitly deferred macOS/Linux CI until a remote exists and authorized
completion of this ticket. Native macOS/Linux checks and GitHub Actions remain
unrun; the configured matrix is retained for that future remote. The implementation
and Windows verification are complete.
