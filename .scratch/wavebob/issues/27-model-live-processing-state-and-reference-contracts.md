# 27: Model live processing state and reference contracts

**What to build:** Introduce nondestructive document/clip envelopes, ordered effect instances, explicit document links and render identities behind the existing command seam. Establish the algorithm-reference and per-effect parameter contracts used by the eleven effect tickets.

**Blocked by:** 10 — Undoable gain, fades and reverse on the editing target; 15 — Trim and process an individual arrangement clip; 19 — Save and reopen portable editing projects.

**Status:** in-progress

## Acceptance criteria

- [ ] Public commands/queries expose stable owner/effect IDs, supported parameter metadata (type, units, default, range), ordered add/remove/reorder/bypass, envelope get/set, explicit linked/independent sources, and a project Effect tempo initially 120 BPM. Unsupported processors are reported as unavailable until implemented rather than silently passing audio.

- [ ] Render identity includes source revision, envelope, ordered parameters/bypass, effect tempo, tail policy and DSP version. Navigation/selection/window state does not invalidate it. Committed processing state has an edit revision distinct from source PCM.

- [ ] Begin/update/commit/cancel preview transactions share validation across GUI/headless/MCP, keep transient state queryable, coalesce updates, commit one undo entry, and handle stale owners, no-ops, target changes, cancelled/abandoned previews and concurrent mutations atomically.

- [ ] Document/clip ownership is isolated. New clip intent records explicit document linkage and source ranges; Make independent preserves a rendered snapshot. Closing a waveform tab retains a project source still used by linked clips. Full-document versus explicitly trimmed clip intent is retained for later refresh.

- [ ] Record algorithm/parameter source references at the pinned upstream commit and define the reference-render fixture/provenance procedure before writing effect DSP. Production modules are WaveBob-owned and do not import upstream engine/plugin/UI/host crates. Retain applicable notices; do not claim clean-room independence from inspected source.

- [ ] Replace obsolete destructive effect command/model assumptions coherently while preserving structural edit/reverse access. Update public catalogue/schema and adapter tests; no old-project, preset or command compatibility layer is required.

- [ ] Public-seam tests verify metadata/validation, preview history, source/render identity changes, dirty intent, isolation and GUI-free/headless/MCP catalogue parity. Document the new state contract and update relevant domain terms.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

This ticket establishes state and reference infrastructure, not placeholder implementations of eleven effects. Actual signal algorithms and editor-specific controls belong to 30–40; linked render publication belongs to 42 and final project persistence to 43.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.


