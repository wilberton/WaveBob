# 41: Add waveform and clip gain and fade handles

**What to build:** Expose the core gain/fade envelopes through waveform and arrangement-clip handles, keeping structural edits separate from effects and preserving existing selection, move, trim and crossfade gestures.

**Blocked by:** 29 — Build the effect stack and native editor framework; 25 — Adjust waveform selection edges; 26 — Drag the crossfade endpoint.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Document and clip bodies expose top-left/right fade triangles and a top-center gain square; fade endpoints and ramp overlays follow edits. Stereo shares one envelope; controls bind true body bounds rather than current selection or viewport edges.

- [ ] Drag gain at 0.25 dB/logical pixel with ten-times-finer adjustment; drag range is -60 to +12 dB, exact input accepts -120 to +60 dB plus mute. Drag inward to change fade duration; exact numeric/keyboard input and double-click reset are available.

- [ ] Hover/focus/drag show local fade-time or gain values and mirrored status help, including exact fade frames. Offscreen or colliding handles have an exact envelope inspector/context route; minimum hit targets are 12 logical pixels without inventing new body edges.

- [ ] Envelope preview changes audible output continuously without moving cursor/selection; release commits one edit, Escape restores state/audio, and no-ops do not enter history. Moving/duplicating/trimming clips carries or independently copies envelopes with edge anchoring and duration clamping.

- [ ] Source peaks/overlays reflect envelope gain/fades and identify the display as source plus envelope rather than final wet audio. Gain readout is distinct from processed output peak and waveform zoom.

- [ ] Upper-edge handles coexist with lower/body trim/crossfade handles and selection edges. Tests cover click-versus-drag, precedence, tiny/zoomed/offscreen clips, stereo, overlapping fades, one-frame audio, gain/mute limits, undo/redo, source isolation and cancelled gestures.

- [ ] Inspect actual waveform and multitrack captures against the reference at normal/high DPI and narrow widths; exact commands achieve the same envelope/audio results without GUI input.

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

