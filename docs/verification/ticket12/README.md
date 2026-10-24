# Ticket 12 verification

24 October 2026, Windows release app. The native MCP check imports a tagged
stereo MP3, chooses visible reverb parameters through `presentation.effects.set`,
applies the effect through the core command, then retrieves
[the rendered result and parameter panel](reverb.png).
[Metadata](reverb.json) records the effect presentation, document and capture
revisions. The image was inspected for parameter labels, selected effect,
truncated-tail policy, waveform and the mockup shell.

Three public-command reverb tests check deterministic impulse response, isolation
from audio preceding a selection, unchanged outside frames/length, truncated
tail, wet/dry linearity, decay-energy behavior, silence, parameter limits,
cancellation and history. Existing competing-edit and playback handoff tests
exercise the shared publication path. The GUI test selects Reverb without an
audio mutation and verifies visible controls/policy at wide and narrow sizes.
Presentation-command tests cover validation, stale revisions and no-op settings
without affecting audio/history.

79 ordinary GUI-enabled tests and 54 GUI-free tests pass. Both Clippy variants
pass with warnings denied; formatting, release build and native MCP verification
pass. Native macOS/Linux checks remain deferred as requested.
