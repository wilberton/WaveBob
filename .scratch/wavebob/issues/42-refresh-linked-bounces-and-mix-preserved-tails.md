# 42: Refresh linked bounces and mix preserved tails

**What to build:** Render document processing into current lossless bounces and atomically refresh all linked clips, with preserved tails, instance processing, cache reuse and correct arrangement timing.

**Blocked by:** 17 — Automatic crossfades on overlapping clips; 30 — Implement Soft Clipper and its egui editor; 31 — Implement Reverb and its egui editor; 32 — Implement Parametric EQ and its egui editor; 33 — Implement Fast Distortion and its egui editor; 34 — Implement Delay and its egui editor; 35 — Implement Chorus and its egui editor; 36 — Implement Limiter and its egui editor; 37 — Implement Balance and its egui editor; 38 — Implement Stereo Enhancer and its egui editor; 39 — Implement Stereo Shaper and its egui editor; 40 — Implement Transient Processor and its egui editor; 41 — Add waveform and clip gain and fade handles.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Waveform-to-multitrack transition pauses playback and resolves unfinished gestures, then prepares a current document render identity. Insert, play, export and device-free preparation share a core freshness barrier; stale bounces are never silently presented as current.

- [ ] Share one full-document bounce across all matching linked instances; trimmed instances render source-context-aware interval results, feeding silence at the excluded interval end. Cache keys include all processing dependencies. Source PCM, editable stack and original files remain intact; no extra visible document or MP3 generation is created.

- [ ] Publish all affected references atomically against captured document/clip revisions. Cancel/failure/stale jobs retain editable work and prior assets, surface pending/error state and support retry or explicit keep-prior-source choice. Rapid switching/undo coalesces or invalidates work rather than overwriting newer edits.

- [ ] Preserve clip IDs, positions/tracks, trims, instance envelope/stack, selection and insertion order. Document envelope/stack is baked once; instance processing occurs afterward. Refresh is derived work outside edit history; undo/redo changes freshness without another bounce history step.

- [ ] Full-document clips follow source body duration; trimmed offsets are retained/clamped after structural edits. Empty ranges or newly invalid two-body overlaps block the complete refresh with affected IDs; nothing is silently deleted, moved or rippled.

- [ ] Mix and draw body and audible-tail extents separately. Body-only geometry determines placement limits and equal-power/containment crossfades; crossfade boundary gain holds into the tail. Tail overlap alone creates no crossfade and never shifts later clips.

- [ ] Allow separately budgeted tail voices beyond the two-body overlap rule, with explicit resource errors rather than silent cuts. Contributing tails extend non-loop playback/export; muted/unsoloed paths do not extend the end. Respect latency/channel/tempo policy, loop body bounds and reported 60-second cap.

- [ ] Protect referenced bounces from eviction and bound cache memory. Public-seam/manual-output tests verify all eleven processors in linked/independent and mixed-rate cases, structural conflicts, cancellation/stale completion, tail cap, crossfades, cache reuse, no double processing and unchanged later clip positions.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

Each blocker effect ticket must deliver its actual DSP/editor, not only metadata. Test the all-linked-clips rule with multiple instances having different trims and independent clip processing.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

