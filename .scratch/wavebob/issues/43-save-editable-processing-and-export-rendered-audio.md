# 43: Save editable processing and export rendered audio

**What to build:** Persist editable sources, all eleven effect types and linked bounces in portable projects, and render current processed samples/arrangements to WAV and MP3 through all adapters.

**Blocked by:** 42 — Refresh linked bounces and mix preserved tails; 18 — Export an arrangement to WAV or MP3; 19 — Save and reopen portable editing projects.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Version/replace the pre-alpha schema with document/clip envelopes and typed stacks, stable IDs/order/bypass, explicit links/trim intent, Effect tempo, render identities/tail policy and referenced lossless assets. Unsupported old/future schemas and unknown effects fail clearly without replacing the workspace; no migration/legacy stage is built.

- [ ] Saved projects reopen without original WAV/MP3 files or an external bounce cache, retaining editable source/stack and the exact last rendered arrangement. Capture coherent current committed state, resolving/cancelling previews and waiting for required bounces; UI window geometry is stored separately.

- [ ] Source and rendered saved baselines are distinct. Envelope, stack, bypass and BPM edits dirty the right sample/project state even without source PCM changes; undo/redo and stale save completion preserve exact dirty/save semantics.

- [ ] Standalone Save/Save As/range export and arrangement export render the shared algorithm path, include declared tails, compensate computational latency and preserve intentional effect delay. Range export processes source context and then excludes later source input while draining the tail.

- [ ] GUI, headless and MCP import/export MP3 and WAV with existing codec/overwrite protections, explicit processed stereo/mono downmix and rate conversion. Saving an MP3 never replaces editable lossless audio with a lossy decode or introduces MP3 during project/bounce persistence.

- [ ] Atomic writes, managed-asset protection, job progress/cancel/failure cleanup and stale-job checks work for processed output. Parameter defaults/ranges/versioning and provenance notices accompany source/distribution docs; production artifacts contain no upstream plugin host/engine dependency.

- [ ] Round-trip/render tests exercise every effect individually plus combined chains, linked/independent clips, source/cache deletion, tails/channels/latency/tempo, dirty-state baselines, corrupt/missing/unknown state and stale/cancelled jobs. Independent-decoder checks cover offered MP3 exports with codec-appropriate tolerances.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

Keep original completed ticket 19 and release verification history unchanged. Replace obsolete fixtures as needed rather than adding compatibility solely to preserve pre-alpha project files.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

