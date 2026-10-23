# 01: Command-driven session and headless runner

**What to build:** Toggle loop and inspect session state from either the
existing GUI or a headless JSON session using the same core implementation.
This makes the architectural separation executable before adding audio edits.

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

- [ ] GUI loop state is read from a core snapshot; its button dispatches a command.
- [ ] A GUI-free build starts without graphics or an audio device, accepts framed
  JSON commands/queries, and returns request IDs and session revisions.
- [ ] A discoverable schema registry describes the initial capabilities. Unknown
  commands, malformed parameters and stale mutations fail without changing state.
- [ ] A command-level test demonstrates GUI/headless equivalent loop state. The
  existing shell still launches and no audio capability is falsely advertised.
- [ ] Establish formatting, lint and build checks, generate the dependency lock,
  and obtain green Windows/macOS/Linux compilation in CI.

## Specification

[WaveBob editor specification](../spec.md).
