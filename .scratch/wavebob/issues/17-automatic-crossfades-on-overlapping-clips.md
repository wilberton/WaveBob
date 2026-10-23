# 17: Automatic crossfades on overlapping clips

**What to build:** Overlap two clips on one track to hear an automatic
transition and see its extent, with updates as clips are moved or trimmed.

**Blocked by:** 16 — Hear and inspect a multitrack mix.

**Status:** ready-for-agent

- [ ] Pairwise overlaps derive equal-power envelopes from geometry and draw an
  accurate X region; removing the overlap removes its transition.
- [ ] Specify/test partial, same-start, contained and single-frame overlaps using
  the agreed rules; source audio remains immutable.
- [ ] Third-simultaneous-clip placements fail atomically with a visible reason in
  this release. Adjacent non-overlapping clips remain valid.
- [ ] Commands, playback and offline rendering agree on envelopes; fixture tests
  verify audible-frame behavior and reposition/trim/undo/redo recomputation.

## Specification

[WaveBob editor specification](../spec.md).
