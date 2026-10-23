# Mockup refinement

Compared the [accepted design](../../ui-concept-01.png) with actual Windows client
captures retrieved through MCP on 23 October 2026. The
[ticket 05 waveform](../ticket05/real-waveform.png) and
[arrangement](../ticket05/real-multitrack.png) are the before images.
[Iteration 01](iteration-01/real-multitrack.png) records the first refinement;
subsequent inspection corrected narrow track labels, clipped clip titles to their
boxes, separated channel labels from the waveform and replaced the proportional
font with bundled Inter. The font's source, hash and license are in
[assets/fonts](../../../assets/fonts/README.md).

| Detail | Before | Refined |
| --- | --- | --- |
| Panels | 250/240 px, sparse document list | 240/200 px, full-width selected rows and vector file/folder icons |
| Header | Open button, default type | File menu, compact centered mode tabs, embedded Inter |
| Waveforms | Unframed bright blue shapes | Framed overview, edge handles, time ruler, fine grid, L/R labels and softer blue |
| Track lanes | Full-row synthetic bars | Bordered teal/blue/gold clips, names, compact controls, timeline and overlap X |
| Browser | Loose text rows | Breadcrumb, folder/audio icons, compact rows and fixed preview footer |
| Effects | Loose labels and short slider | Aligned target, full-width gain, paired buttons and Apply |
| Transport | Text buttons | Vector start/play/loop icons, centered time and responsive shortcut hints |

Final captures, all inspected:

- Wide 1440×900: [waveform](wide/real-waveform.png), [multitrack](wide/real-multitrack.png).
- Compact 1440×560, closest to the design's aspect ratio:
  [waveform](compact/real-waveform.png), [multitrack](compact/real-multitrack.png).
- Narrow 800×600: [waveform](narrow/real-waveform.png), [multitrack](narrow/real-multitrack.png).
- 150% UI scale: [waveform](high-dpi/real-waveform.png), [multitrack](high-dpi/real-multitrack.png).

Each folder's `audio-metadata.json` records session identity, rendered revision,
scale and document metadata. These are actual imported mono/stereo waveforms;
the arrangement is explicitly labeled DEMO. PNG image content delivered by MCP
matches the saved file bytes. Background job publication remains fenced during
framebuffer capture.

Intentional differences: the full-width status bar and resize/collapse handles
are requested additions. The sample starts with no selection and a full-sample
viewport, rather than fabricating the design's selection. Track editing, browser
audition and effects remain visibly disabled until their tickets. The current
fixture is 12 seconds rather than the concept's 2:14 recording; its actual sample
bounds determine the waveform. Taller windows give channels and track lanes more
height, and narrow clip names are clipped to their boxes. The design's OS window
chrome is outside client-area screenshots.

Validation includes the complete 33-test GUI-enabled suite, Clippy with warnings
denied, real egui panel/divider drags, independent chevrons, backtick and focus
guards, persistent hover/focus help without popups, document switching, and native
attachment, capture-fence and preference-restart regressions. Final font/layout
changes also passed the seven GUI interaction tests and live import/capture tests
at each size. Native macOS/Linux checks remain deferred until a remote exists.

Reproduce a capture using the ignored `real_audio_import_switching_and_pixels_through_mcp`
test in `tests/mcp.rs`. Set `WAVEBOB_TEST_ARTIFACT_DIR` to the output folder,
`WAVEBOB_AUDIO_TEST_WINDOW_SIZE` to e.g. `800x600`, and
`WAVEBOB_AUDIO_TEST_UI_SCALE` to e.g. `1.5`. The GUI accepts `--ui-scale 0.75..3`
for reproducible scale checks without changing OS display preferences.
