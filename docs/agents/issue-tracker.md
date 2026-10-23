# Issue tracker: Local Markdown

Specs and implementation tickets live in `.scratch/` and should be versioned
alongside the project. The user selected this tracker and authorized publication
on 23 October 2026. No external issue service is involved.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`.
- The canonical spec is `.scratch/<feature-slug>/spec.md`.
- One implementation ticket per file at
  `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01` in dependency
  order initially. Keep identifiers stable after publication: append new IDs and
  use explicit blockers and the index's execution order for inserted work.
- The feature `README.md` indexes the spec and tickets. It is navigation; ticket
  files are authoritative for their status, blockers and acceptance criteria.
- Each ticket has a title, **What to build**, **Blocked by**, **Status** and
  acceptance checkboxes. Blockers include ticket numbers and titles.
- Spec and ticket triage roles use the mapping in `triage-labels.md`. Approved
  implementation tickets begin as `ready-for-agent`, even when blocked.
- Publish by writing the spec or individual ticket files here. Fetch by reading
  the referenced file in full, including any `## Comments` appended at the bottom.
- Preserve comments and acceptance history when updating a ticket. Update affected
  blocker references and the index when changing a title or dependency.

## Execution and frontier

For implementation tickets, use `in-progress` when claimed and `done` only after
all acceptance criteria have been met and relevant verification is recorded.
These are execution states, separate from the five triage roles. An unchecked
criterion or unresolved required verification means the ticket is not done.

A ticket may start only if its status is `ready-for-agent` and every ticket in its
**Blocked by** field is `done`. Read the actual blocker files, not the index, to
  determine this. Among unblocked tickets, follow the feature index's execution
  order (ticket 22 intentionally follows 05). Set status to
`in-progress` before work, and record validation with completion. Never interpret
publication or `ready-for-agent` as evidence that blockers are complete.

## Current feature

[WaveBob editor](../../.scratch/wavebob/README.md) is the initial feature. Its spec
and original 21 implementation tickets were approved for publication with MP3
import/export required in version 1 and the generated WaveBob mockups as the
visual reference. Ticket 22 adds an explicit refinement pass after ticket 05.
