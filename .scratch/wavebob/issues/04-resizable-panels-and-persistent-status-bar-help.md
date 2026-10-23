# 04: Resizable panels and persistent status-bar help

**What to build:** Resize/collapse either sidebar, toggle both with backtick,
and read selection state and contextual help below the transport in both modes.

**Blocked by:** 01 — Command-driven session and headless runner.

**Status:** in-progress

- [ ] Inner-edge dragging resizes panels; independent chevrons retain restore
  affordances and expanded widths. The browser/library split also resizes.
- [ ] Backtick hides both when either is visible and restores both when both are
  hidden; text input and modal focus suppress global shortcuts.
- [ ] Widths, split ratio and visibility survive restart and clamp on narrow windows.
  Presentation commands expose equivalent layout operations.
- [ ] Status readouts show actual state or an empty state. Hover/focus help occupies
  a separate region, clears correctly and explains disabled controls. Replace
  the scaffold's floating tooltip behavior; progress/errors remain readable.
- [ ] Interaction tests cover chevrons, backtick, drag resize and tooltip absence.
- [ ] Match the accepted WaveBob mockups' layout, spacing, typography hierarchy,
  colors and controls, adding the status bar and chevrons. Use those mockups as
  the visual reference rather than treating Artcraft apps as the styling target.

## Specification

[WaveBob editor specification](../spec.md).
