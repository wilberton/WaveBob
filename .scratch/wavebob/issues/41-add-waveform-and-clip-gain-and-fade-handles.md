# 41: Add waveform and clip gain and fade handles

**What to build:** Expose the core gain/fade envelopes through waveform and arrangement-clip handles, keeping structural edits separate from effects and preserving existing selection, move, trim and crossfade gestures.

**Blocked by:** 29 — Build the effect stack and native editor framework; 25 — Adjust waveform selection edges; 26 — Drag the crossfade endpoint.

**Status:** done

## Acceptance criteria

- [x] Document and clip bodies expose top-left/right fade triangles and a top-center gain square; fade endpoints and ramp overlays follow edits. Stereo shares one envelope; controls bind true body bounds rather than current selection or viewport edges.

- [x] Drag gain at 0.25 dB/logical pixel with ten-times-finer adjustment; drag range is -60 to +12 dB, exact input accepts -120 to +60 dB plus mute. Drag inward to change fade duration; exact numeric/keyboard input and double-click reset are available.

- [x] Hover/focus/drag show local fade-time or gain values and mirrored status help, including exact fade frames. Offscreen or colliding handles have an exact envelope inspector/context route; minimum hit targets are 12 logical pixels without inventing new body edges.

- [x] Envelope preview changes audible output continuously without moving cursor/selection; release commits one edit, Escape restores state/audio, and no-ops do not enter history. Moving/duplicating/trimming clips carries or independently copies envelopes with edge anchoring and duration clamping.

- [x] Source peaks/overlays reflect envelope gain/fades and identify the display as source plus envelope rather than final wet audio. Gain readout is distinct from processed output peak and waveform zoom.

- [x] Upper-edge handles coexist with lower/body trim/crossfade handles and selection edges. Tests cover click-versus-drag, precedence, tiny/zoomed/offscreen clips, stereo, overlapping fades, one-frame audio, gain/mute limits, undo/redo, source isolation and cancelled gestures.

- [x] Inspect actual waveform and multitrack captures against the reference at normal/high DPI and narrow widths; exact commands achieve the same envelope/audio results without GUI input.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

The processing/evaluation rules are established in 28; this ticket completes their native interactions in both modes. It is independently blocked on existing selection-edge and crossfade-endpoint work so regressions are checked explicitly.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.


## Implementation evidence — 24 October 2026

Shared owner-bound envelope controls now serve stereo document bodies and trimmed
clips. Preview/release/Escape use core processing transactions; focus loss and
target/history changes cancel unfinished GUI gestures. Source PCM, cursor and
selection remain separate. Gain/fade guides and source peaks follow the effective
envelope; output metering stays distinct. Exact entry handles hidden/colliding
controls. Duplicate clip copies values into fresh owner/effect identities; mixer
retargeting checks owner IDs before adopting an existing voice's state.

See [interaction contract](../../../docs/envelope-controls.md) and
[inspected native capture evidence](../../../docs/verification/envelopes/README.md).
Six actual Windows captures cover waveform/multitrack at normal scale, 1.5× DPI
and 800×600 narrow width, with owner/render metadata. The arrangement includes an
overlap and the pre-existing crossfade control. The ruler label remains readable
at narrow width. Platform release verification remains owned by ticket 21.

Validation passed:

- `cargo test --offline --all-targets`: 224 passed, 26 optional native/device/
  performance tests ignored; full public/adapter/WAV/MP3 workflows included
  (`.tools/test41-allgui.log`). After adding the focus-loss regression, all 50
  application GUI tests passed (`.tools/test41-gui-final.log`).
- `cargo test --offline --no-default-features --all-targets`: 115 passed, including
  duplication/history/clamping, preview-owner navigation, processing/manual audio
  and both direct/headless/MCP workflows (`.tools/test41-full.log`).
- `cargo test --offline --test gui_attachment native_envelope_waveform -- --ignored --nocapture`:
  native capture batch passed (`.tools/test41-native-final.log`).
- `cargo test --offline --test mcp attached_stdio_client_retrieves_fresh_pngs -- --ignored --nocapture`:
  real two-window MCP discovery, schemas, PNG retrieval/isolation and cleanup passed
  (`.tools/test41-native-mcp.log`). Its stale catalogue-count assertion was updated
  to the current 93 native commands; GUI-free discovery has 72.
- GUI and GUI-free all-target Clippy with `-D warnings`, plus rustfmt checks passed.

Focused GUI checks cover fine gain dragging, both exact gain limits and mute,
independent/overlapping fades, reset, click/drag precedence, offscreen/colliding
controls, a drag crossing another handle, tiny/one-frame clips, cancellation,
undo/redo and live manual-output preview before release. Existing selection-edge,
clip move/trim and crossfade gesture regressions also passed. The focus-loss test
first reproduced a retained preview, then passed after adding explicit cancellation.
No subjective listening or unavailable physical platform checks are claimed.
