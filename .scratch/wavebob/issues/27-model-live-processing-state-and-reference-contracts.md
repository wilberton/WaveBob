# 27: Model live processing state and reference contracts

**What to build:** Introduce nondestructive document/clip envelopes, ordered effect instances, explicit document links and render identities behind the existing command seam. Establish the algorithm-reference and per-effect parameter contracts used by the eleven effect tickets.

**Blocked by:** 10 — Undoable gain, fades and reverse on the editing target; 15 — Trim and process an individual arrangement clip; 19 — Save and reopen portable editing projects.

**Status:** done

## Acceptance criteria

- [x] Public commands/queries expose stable owner/effect IDs, supported parameter metadata (type, units, default, range), ordered add/remove/reorder/bypass, envelope get/set, explicit linked/independent sources, and a project Effect tempo initially 120 BPM. Unsupported processors are reported as unavailable until implemented rather than silently passing audio.

- [x] Render identity includes source revision, envelope, ordered parameters/bypass, effect tempo, tail policy and DSP version. Navigation/selection/window state does not invalidate it. Committed processing state has an edit revision distinct from source PCM.

- [x] Begin/update/commit/cancel preview transactions share validation across GUI/headless/MCP, keep transient state queryable, coalesce updates, commit one undo entry, and handle stale owners, no-ops, target changes, cancelled/abandoned previews and concurrent mutations atomically.

- [x] Document/clip ownership is isolated. New clip intent records explicit document linkage and source ranges; Make independent preserves a rendered snapshot. Closing a waveform tab retains a project source still used by linked clips. Full-document versus explicitly trimmed clip intent is retained for later refresh.

- [x] Record algorithm/parameter source references at the pinned upstream commit and define the reference-render fixture/provenance procedure before writing effect DSP. Production modules are WaveBob-owned and do not import upstream engine/plugin/UI/host crates. Retain applicable notices; do not claim clean-room independence from inspected source.

- [x] Replace obsolete destructive effect command/model assumptions coherently while preserving structural edit/reverse access. Update public catalogue/schema and adapter tests; no old-project, preset or command compatibility layer is required.

- [x] Public-seam tests verify metadata/validation, preview history, source/render identity changes, dirty intent, isolation and GUI-free/headless/MCP catalogue parity. Document the new state contract and update relevant domain terms.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

This ticket establishes state and reference infrastructure, not placeholder implementations of eleven effects. Actual signal algorithms and editor-specific controls belong to 30–40; linked render publication belongs to 42 and final project persistence to 43.

## Verification

Completed 24 October 2026. The owner-scoped processing contract, explicit source
links/tab retention, preview conflict handling, render identities, envelope validation,
shared BPM and chronological history are implemented. The common catalogue reports
all eleven processors as unavailable until their algorithm tickets enable them;
there is no identity-processor substitute. Parameter value/schema types and ordered
stack operations are shared core state, ready for each effect's parameter metadata.

`cargo test --offline --no-default-features` passes the complete suite (including
headless/MCP catalogue/schema parity and the direct/headless/MCP WAV/MP3 workflow).
The four focused `--test processing` cases pass after the final additions. They cover
preview validation/coalescing/cancel/no-op/single-step undo, stale writes and abandoned
transactions, unchanged PCM, render identity/navigation/tempo, instance isolation,
linked source retention and obsolete command rejection. `cargo check --offline
--all-targets` and Clippy with warnings denied pass. Tests use `.tools/test-temp`
because Windows sandbox package temp paths deny atomic output publication.

The old destructive command spellings are removed; explicit source transformations
use `edit.process` / `clips.pcm.process`, distinct from `processing.edit`. Current
clients/docs were updated together. The existing GUI tool selector is replaced by
29, as scoped; streaming/envelope playback is 28, bounce publication 42 and final
rendered-save integration 43. No effect audio or native editor verification is
claimed by this state ticket.

See [state contract](../../../docs/live-processing.md),
[reference contract](../../../docs/effect-reference-contract.md) and
[pinned source hashes](../reference-manifest.json). No upstream DSP was built or
rendered for this ticket; those measured comparisons belong to 30�40.
