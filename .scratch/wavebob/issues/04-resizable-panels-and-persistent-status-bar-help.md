# 04: Resizable panels and persistent status-bar help

**What to build:** Resize/collapse either sidebar, toggle both with backtick,
and read selection state and contextual help below the transport in both modes.

**Blocked by:** 01 — Command-driven session and headless runner.

**Status:** done

- [x] Inner-edge dragging resizes panels; independent chevrons retain restore
  affordances and expanded widths. The browser/library split also resizes.
- [x] Backtick hides both when either is visible and restores both when both are
  hidden; text input and modal focus suppress global shortcuts.
- [x] Widths, split ratio and visibility survive restart and clamp on narrow windows.
  Presentation commands expose equivalent layout operations.
- [x] Status readouts show actual state or an empty state. Hover/focus help occupies
  a separate region, clears correctly and explains disabled controls. Replace
  the scaffold's floating tooltip behavior; progress/errors remain readable.
- [x] Interaction tests cover chevrons, backtick, drag resize and tooltip absence.
- [x] Match the accepted WaveBob mockups' layout, spacing, typography hierarchy,
  colors and controls, adding the status bar and chevrons. Use those mockups as
  the visual reference rather than treating Artcraft apps as the styling target.

## Specification

[WaveBob editor specification](../spec.md).

## Implementation and verification

Completed with shared layout commands, separately persisted preferences and a
status strip below the centered transport. Both side panels and the library split
resize; collapsed rails restore remembered widths. Layout changes advance only
presentation revision and are available through MCP. Empty readouts replace the
scaffold's fabricated selection/transport times. Hover/focus help, disabled reasons,
progress and persistent errors occupy separate status regions without popups.

All 29 GUI-enabled checks (including three native GUI/MCP tests), all 16 GUI-free
checks, both Clippy configurations, formatting and all release builds passed on
Windows. The live layout test also passed against release executables; MCP images
cover both modes expanded, hidden and at 800×600, plus independent hiding, restore
and profile restart. See [verification and captures](../../../docs/verification/ticket04/README.md)
and [layout commands/preferences](../../../docs/layout.md).

The common shell follows the accepted mockup. Ticket 22 after 05 remains the
dedicated closer waveform/arrangement visual match using real audio. Native
macOS/Linux verification remains deferred until a remote exists. Initial
implementation commit: `de182a8`.
