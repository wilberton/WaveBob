# Panels and status-bar help

Resize the file and effects panels from their inner edges. Each chevron collapses
its panel; the collapsed rail retains a restore chevron. Backtick hides both when
either is visible and restores both when both are hidden. The divider between
open files and the browser changes their height ratio. Both lists scroll when
their share is short.

Defaults are 240 logical pixels on the left, 200 on the right and a 0.18 library
height fraction. Expanded panels have a minimum width of 180; collapsed rails use
24. Narrow windows shrink rendered widths proportionally to preserve a 400-pixel
center, retaining preferred widths for restoration. The window minimum is 800×520.

`--window-size WIDTHxHEIGHT` and `--ui-scale SCALE` (0.75–3) allow reproducible
layout checks. Scale is applied on top of the OS display scale and is reported in
capture metadata. Inter is embedded for consistent proportional text across OSes.

## Commands and persistence

`presentation.get` reports `layout` and preference-save status in addition to mode
and presentation revision. `presentation.layout.set` accepts an atomic patch:

```json
{"id":"layout","command":"presentation.layout.set","params":{"left_width":300,"right_width":260,"left_visible":true,"right_visible":false,"library_fraction":0.4,"expected_presentation_revision":0}}
{"id":"panels","command":"presentation.panels.toggle"}
```

All fields are optional. Widths must be finite and at least 180; the library
fraction must be between 0.15 and 0.85. Invalid patches leave the entire layout
unchanged. Actual changes advance presentation revision once, leaving core revision
and audio history unchanged. No-op patches do not advance revisions. Both commands
accept `expected_presentation_revision`; envelope `expected_revision` still guards
the core revision. MCP exposes these same commands as tools, with their fields
inside the tool argument's `params` object. GUI-free sessions report missing capability.

Expanded widths, visibility and split ratio use a versioned JSON file under the
per-user configuration directory, separate from audio projects. GUI `--preferences
FILE` or `WAVEBOB_PREFERENCES_PATH` selects an isolated profile. Saving runs on a
worker, coalesces rapid drag updates and replaces the file atomically. Normal
shutdown drains pending saves. Before forcing a restart in automation, poll
`presentation.get` until `preferences.status` is `saved`. Other states are `saving`,
`error` (with a diagnostic) and `memory_only` for a host without a preference store.

Missing files use defaults. Invalid files fall back to defaults with a visible
error; changing modes alone does not overwrite them. An explicit layout change
retries saving. Failed saves leave the live layout usable and retain the previous
complete file. Logical layout values use JSON-compatible double precision;
conversion to egui coordinates happens only when drawing.

GUI `--window-size WIDTHxHEIGHT`, for example `--window-size 800x600`, controls the
initial logical size for native checks. Tests use isolated preference files and
do not modify the normal user profile.

## Status and shortcuts

The status strip sits at the absolute bottom across the whole client width, below
the centered transport. State readouts and contextual help have separate regions.
Waveform mode reports the actual source cursor/selection; arrangement mode reports
its timeline cursor/selection and selected clip count. Empty states show no document
or selection. Readouts never fabricate source times.

Hover help takes precedence over keyboard-focus help. Moving away, leaving the
window or removing a control clears stale help. Disabled controls explain why
they cannot run. There are no floating control tooltips. Long help truncates
before the state readout; save progress and persistent errors have separate space.

Backtick and slash (loop) run on fresh, unmodified presses. Text editing, native
loss of focus, open popups and modals suppress shortcuts; ordinary button focus
does not. Playback controls depend on available output state; device errors remain visible.

The common shell follows the accepted mockup's dark surfaces, fine borders,
compact typography, blue controls and centered tabs/transport. Full waveform/clip
fidelity and remaining visual alignment are refined in ticket 22 after ticket 05.
