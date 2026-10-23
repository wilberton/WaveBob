# 02: Attach to the GUI and capture its actual pixels

**What to build:** An automation client selects a running GUI instance,
switches mode through commands and captures the resulting client area as PNG.

**Blocked by:** 01 — Command-driven session and headless runner.

**Status:** ready-for-agent

- [ ] Local instance discovery and explicit attachment target one named session;
  attached commands affect the visible session rather than a second core.
- [ ] View selection/query and a render barrier allow a screenshot after a requested
  state and presentation revision. Timeout or renderer failure returns an error.
- [ ] Capture includes the complete client area and reports PNG location/resource,
  pixel dimensions, scale, session, active mode and rendered revision.
- [ ] Capture respects explicit overwrite intent; GUI-free sessions report missing
  capability. Verify both mode images and a command-then-capture race scenario.

## Specification

[WaveBob editor specification](../spec.md).
