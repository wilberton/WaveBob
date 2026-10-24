# 44: Verify the complete live effects workflow

**What to build:** Verify the full approved live-effects workflow across GUI, headless and MCP, inspect native window/handle captures and record measured responsiveness, audio/reference agreement and remaining platform coverage.

**Blocked by:** 43 — Save editable processing and export rendered audio; 20 — Verify a complete AI editing and visual-review workflow.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] All eleven effects are shipped and discoverable with working DSP and their own egui editors. Verify supported controls/modes against the scope table; advanced EQ quality, external sidechains, difference outputs and plugin hosting remain omitted rather than decorative enabled controls.

- [ ] Run the specified scenario: import, insert multiple linked clips, return to waveform, adjust handles and multiple live effects while looping, reorder/bypass/undo, return to multitrack, wait for atomic shared/trim bounces, edit one clip, preserve tails over a later unmoved clip, save/reopen without sources/cache and export WAV/MP3.

- [ ] Repeat equivalent domain work through direct commands, GUI-attached MCP and device-free headless sessions. Compare fixed-setting reference/manual/offline/bounce/export results at declared tolerances; explicitly account for loop-accumulated state, codec loss and live transitions.

- [ ] Capture actual main-window and several simultaneous effect viewports after state barriers; inspect both editor modes, stable window bindings, open/close/focus/typing shortcuts, lost-window recovery and narrow/high-DPI layouts against the accepted mockup and source control layouts.

- [ ] On a named reference machine at 48 kHz/256-frame blocks, measure input-to-audible-update latency (target <=50 ms after initial preparation), callback allocations/load/dropouts, topology swaps, long-file/large-stack preparation and bounded tail/cache resources. Report observations and unresolved failures rather than claiming unmeasured limits.

- [ ] Run appropriate Rust formatting/lint/GUI-free and GUI test suites, representative hardware listening, source provenance/notice and production dependency checks. Build/package smoke checks cover new effects without requiring plugin files or external codecs.

- [ ] Exercise available native platform checks and document exact unavailable Windows/macOS/Linux/X11/Wayland evidence separately. Existing user deferrals/release constraints from ticket 21 remain in force; do not invent native executions or silently mark outstanding required checks complete.

- [ ] Update command, waveform, transport, crossfade, project and workflow documentation to the delivered behavior, linking reproducible verification, reference-render tolerances, measured resource limits and any remaining release obligations.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

This is integration/release evidence for tickets 27–43. It does not claim the outstanding CI/native-platform acceptance in ticket 21 has completed.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

