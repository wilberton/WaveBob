# 08: Waveform playback, seeking and loop transport

**What to build:** Hear the active sample from its cursor using the transport
or commands, pause/seek, and loop the selected range or whole sample.

**Blocked by:** 07 — Precise waveform selection, overview zoom and pan.

**Status:** done

- [x] Space/play, Enter/comma/start and slash/loop implement the specified behavior,
  including selection loops, cursor outside the loop and end-of-sample restart.
- [x] Playback position is visible and queryable; requested versus applied changes
  are distinguishable. Pausing and changing active document behave predictably.
- [x] Audio initialization/device errors are recoverable. The callback consumes
  prepared state without blocking I/O, locks or allocation.
- [x] Deterministic output tests verify exact loop/seek frames; hardware smoke
  checks confirm sound on each platform. Headless editing needs no device.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

[Windows callback, deterministic output and MCP capture record](../../../docs/verification/ticket08/README.md).
Windows native output was verified. The user's macOS/Linux deferral still applies;
physical listening and those platform checks remain release verification items.
