# 29: Build the effect stack and native editor framework

**What to build:** Replace the right-hand Edit & Effects/Apply UI with the accepted compact effect stack and provide a reusable native egui viewport shell for per-instance effect editors and their automation/capture.

**Blocked by:** 28 — Stream effects with safe updates and latency alignment; 03 — Drive sessions and inspect captures through MCP.

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Match the accepted mockup: owner name/scope, compact ordered rows with grip, bypass, effect name, small Open controls button and remove/move menu; Add effect, Bypass all and processed output meter. Parameters do not expand inline. The old Gain/Fade/Reverse/Apply controls leave this panel; reverse/undo/redo remain accessible as Edit actions.

- [ ] Rows bind the active waveform document or exactly one selected arrangement clip; no/multiple selections explain unavailability. Add effect discovers implemented entries from the common registry, with all eleven expected by final acceptance.

- [ ] Opening creates/focuses one separate modeless native window per stable owner/effect pair; multiple instance windows can coexist. Titles identify owner/instance. Selection changes do not retarget them; closing leaves processing active, removal closes/cancels, and undo restores effect state without unexpectedly reopening windows.

- [ ] Provide reusable knobs/faders/switches, exact entry/reset, graph input and bounded analyzer/meter snapshots, all bound to core preview/commit transactions. Input text suppresses shortcuts; otherwise transport and shared undo/redo work from editor focus.

- [ ] Window geometry is presentation state, restored safely across DPI/monitor changes. Shared help/value display, focus, missing-owner and hidden-window behavior are defined; hidden visualizations avoid unnecessary work.

- [ ] GUI commands open/focus/close and capture each viewport's actual rendered pixels with window, owner/effect and state revision metadata. Main/effect capture barriers report pending resources. Headless requests return structured capability errors without disabling DSP.

- [ ] Focused gesture/host tests cover row reorder, bypass, no target, native-window lifecycle, focus shortcuts, stable bindings and per-window capture. Complete representative actual-viewport capture with the first effect in 30; final multiwindow/platform visual review belongs to 44.

## Specification and references

Read the [canonical spec](../spec.md) and the approved
[effects/envelope contract](../effects-and-clip-controls.md) before implementation.

[Accepted mockup](../effects-stack-mockup-v1.png) and
[source investigation](../lemon-effects-review.md) supply visual/algorithm context.

## Implementation scope

Read the accepted mockup and generation caveats. Use effect-specific egui content from each effect ticket; toolkit probes may test the generic shell but are not shipping processors/editors.

## Verification

Record commands, reference cases/tolerances, actual results and relevant capture
paths here when implementing. This publication has run no DSP or application
verification and leaves every acceptance criterion unchecked.

