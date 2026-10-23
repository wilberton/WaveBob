# Ticket 04 verification

Verified on Windows x86-64 with Rust 1.99.1/MSVC and the real egui renderer.
The release GUI and GUI-free release MCP server were exercised as separate
processes. These PNGs were decoded from returned MCP image blocks, compared with
the saved artifacts and inspected against the accepted mockup's common shell.

| Layout | Waveform | Multitrack |
| --- | --- | --- |
| Expanded, 1440×900 | [capture](waveform-expanded.png) | [capture](multitrack-expanded.png) |
| Collapsed, 1440×900 | [capture](waveform-hidden.png) | [capture](multitrack-hidden.png) |
| Narrow, 800×600 | [capture](waveform-narrow.png) | [capture](multitrack-narrow.png) |

Also retained: [left panel hidden](waveform-left-hidden.png),
[restored panels](multitrack-restored.png) and [revision/layout metadata](metadata.json).
The status strip remains visible in every layout. Core revision stays zero during
these presentation changes. Restart restores saved widths, split and visibility;
narrow rendering preserves the requested expanded widths.

All 29 default-feature tests passed with native tests included, and all 16 GUI-free
tests passed. Both Clippy configurations passed with warnings denied, formatting
passed and GUI/headless/MCP release builds passed. The native suite includes prior
capture-race, overwrite and two-instance MCP regressions. Gesture tests run real
egui pointer/key events for both edge drags, the library split, independent
chevrons, backtick restoration, text/modal/repeat/modifier guards, focus help and
tooltip absence. They inspect painted status text and preserve empty-state readouts
and persistent errors at narrow widths. Preference tests cover atomic saving,
invalid files, restart, failed writes and continued use after failure.

The full regression caught float formatting differences between direct/JSON/MCP
adapters; logical layout values now retain double precision until the egui boundary.
Screenshot inspection caught unsupported placeholder glyphs, which were replaced
with supported labels. Buttons were centered and the browser made scrollable when
the split leaves little height.

To reproduce the live layout check:

```powershell
$env:WAVEBOB_GUI_TEST_BIN = Join-Path $PWD 'target\release\wavebob.exe'
$env:WAVEBOB_MCP_TEST_BIN = Join-Path $PWD 'target\release\wavebob-mcp.exe'
$env:WAVEBOB_TEST_ARTIFACT_DIR = Join-Path $PWD '.tools\ticket04-release'
cargo test --locked --test mcp layout_controls -- --ignored --nocapture
```

Tests use isolated profiles. The common shell now follows the mockup's dark
surfaces, borders, colors, hierarchy and centered transport, with the additional
status strip and panel controls. The synthetic waveform/arrangement geometry and
remaining visual details are intentionally refined in ticket 22 immediately after
real WAV loading in 05. No audio playback or editing is claimed here. Native
macOS/Linux checks remain deferred until a remote exists, as requested.
