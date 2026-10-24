# Envelope control verification — 24 October 2026

Windows native captures were produced by the real desktop executable through
`capture.png`, then inspected at 1440×900/1.0 scale, 1440×900/1.5 scale and
800×600/1.0 scale. Each configuration includes stereo waveform and multitrack
views with -6 dB, 8000-frame fade-in and 12000-frame fade-out on a 48000-frame
body. The second arrangement instance overlaps the selected original, retaining
the automatic crossfade and lower-edge endpoint control. Metadata sidecars bind
the images to explicit document/clip owners and processing render identities.

The inspected controls track their fade endpoints, gain guide and true body
bounds. The narrow views retain the exact inspector; high-DPI controls retain
logical hit sizes. Peaks reflect the envelope and the display identifies itself
as Source + envelope. The output meter is separate. These are actual rendered
application views, not mockups. Native platform/audio-device acceptance remains
tracked separately in ticket 21; no physical listening claim is made here.

| Layout | Waveform | Arrangement | Metadata |
| --- | --- | --- | --- |
| Normal | [PNG](waveform-normal.png) | [PNG](clips-normal.png) | [JSON](capture-normal.json) |
| High DPI | [PNG](waveform-high-dpi.png) | [PNG](clips-high-dpi.png) | [JSON](capture-high-dpi.json) |
| Narrow | [PNG](waveform-narrow.png) | [PNG](clips-narrow.png) | [JSON](capture-narrow.json) |

Reproduce native captures with `WAVEBOB_TEST_ARTIFACT_DIR` pointing here and:

```text
cargo test --offline --test gui_attachment native_envelope_waveform -- --ignored --nocapture
```

Focused real egui input tests cover preview/release/Escape/focus loss, undo/redo, source and
navigation isolation, double-click reset, exact gain/mute, overlapping fades,
offscreen endpoints, collisions and a fade dragging across the gain handle,
Shift sensitivity, clip move/trim precedence and a one-frame clip inspector.
The manual-output test clocks audible preview samples before release and checks
restored unity output after Escape. Public-command tests cover independent clip
duplication/history/trim clamping and owner-preserving navigation. Existing
selection-edge/crossfade tests run with the full GUI suite. Core/manual/offline
processing and direct/headless/MCP workflow suites verify the shared audio path.

Validation commands and results are recorded in ticket 41. Numeric reference
processing behavior is also documented in `docs/processing-renderer.md`.
