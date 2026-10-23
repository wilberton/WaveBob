# WaveBob

## Agent skills

### Issue tracker

Specs and tickets live in local Markdown under `.scratch/`. Read
[the tracker conventions](docs/agents/issue-tracker.md) when publishing or updating
work. The [WaveBob tracker](.scratch/wavebob/README.md) links the canonical spec and
individual tickets. Start only tickets whose blockers are complete.

### Triage labels

Use the default role strings in [the label mapping](docs/agents/triage-labels.md).
Execution status and triage readiness are distinct from dependency availability;
see the tracker conventions before claiming or completing a ticket.

### Domain docs

Use a single-context domain layout. Follow [the domain guidance](docs/agents/domain.md)
when exploring or changing domain behavior.

## Implementation references

Before implementing editor behavior, read [the canonical spec](.scratch/wavebob/spec.md)
and the relevant ticket. The [WaveBob UI mockups](docs/ui-concept-01.png) govern
appearance, with the spec's additional status bar and panel controls. Artcraft
is architectural background rather than the visual target. Version 1 requires
MP3 import and export in the GUI, headless and MCP workflows.
