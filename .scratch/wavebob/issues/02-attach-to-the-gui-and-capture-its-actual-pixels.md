# 02: Attach to the GUI and capture its actual pixels

**What to build:** An automation client selects a running GUI instance,
switches mode through commands and captures the resulting client area as PNG.

**Blocked by:** 01 — Command-driven session and headless runner.

**Status:** done

- [x] Local instance discovery and explicit attachment target one named session;
  attached commands affect the visible session rather than a second core.
- [x] View selection/query and a render barrier allow a screenshot after a requested
  state and presentation revision. Timeout or renderer failure returns an error.
- [x] Capture includes the complete client area and reports PNG location/resource,
  pixel dimensions, scale, session, active mode and rendered revision.
- [x] Capture respects explicit overwrite intent; GUI-free sessions report missing
  capability. Verify both mode images and a command-then-capture race scenario.

## Specification

[WaveBob editor specification](../spec.md).

## Implementation and verification

Completed on 23 October 2026. A private local registry discovers named running
instances; clients explicitly attach by session UUID over named pipes on Windows
or local sockets on Unix. Attached requests use the GUI's existing session.
Presentation commands have a separate revision; render barriers and PNG capture
fence actual framebuffer readback before allowing further state changes. PNG
publication is atomic and requires explicit overwrite intent.

Verified on Windows x86-64 with Rust 1.99.1/MSVC: formatting, Clippy with warnings
denied, all 15 ordinary GUI-enabled tests and all 13 GUI-free tests passed. The
opt-in native integration test also passed against two running GUI instances,
including isolation, both modes, a concurrent mode/capture race, revision metadata,
timeouts, error recovery and overwrite protection. Release builds passed; the
GUI-free release executable successfully discovered, attached to and captured
both modes from the running release GUI. Both 1440×900 images were inspected.

See the [verification record and saved PNGs](../../../docs/verification/ticket02/README.md)
and [command protocol](../../../docs/commands.md) for reproducible commands and
metadata. Native macOS/Linux verification remains deferred until a remote exists,
as authorized by the user. MCP delivery is ticket 03; the mockup-matching UI pass
is ticket 22 immediately after 05. Captures currently show the demo-data shell.
