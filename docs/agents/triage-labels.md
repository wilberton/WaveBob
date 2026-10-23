# Triage labels

The default canonical role strings are used unchanged in local Markdown.

| Canonical role | Local status | Meaning |
| --- | --- | --- |
| `needs-triage` | `needs-triage` | Maintainer evaluation required |
| `needs-info` | `needs-info` | More information required |
| `ready-for-agent` | `ready-for-agent` | Specified for agent implementation, subject to blockers |
| `ready-for-human` | `ready-for-human` | Requires human implementation |
| `wontfix` | `wontfix` | Will not be actioned |

The tracker conventions separately define `in-progress` and `done` for execution.
When a skill requests a triage role, use its exact string from this table.
