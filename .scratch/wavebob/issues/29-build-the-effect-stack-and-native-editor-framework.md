# 29: Build the effect stack and native editor framework

**What to build:** Replace the right-hand Edit & Effects/Apply UI with the accepted compact effect stack and provide a reusable native egui viewport shell for per-instance effect editors and their automation/capture.

**Blocked by:** 28 — Stream effects with safe updates and latency alignment; 03 — Drive sessions and inspect captures through MCP.

**Status:** done

## Acceptance criteria

- [x] Match the accepted mockup: owner name/scope, compact ordered rows with grip, bypass, effect name, small Open controls button and remove/move menu; Add effect, Bypass all and processed output meter. Parameters do not expand inline. The old Gain/Fade/Reverse/Apply controls leave this panel; reverse/undo/redo remain accessible as Edit actions.

- [x] Rows bind the active waveform document or exactly one selected arrangement clip; no/multiple selections explain unavailability. Add effect discovers implemented entries from the common registry, with all eleven expected by final acceptance.

- [x] Opening creates/focuses one separate modeless native window per stable owner/effect pair; multiple instance windows can coexist. Titles identify owner/instance. Selection changes do not retarget them; closing leaves processing active, removal closes/cancels, and undo restores effect state without unexpectedly reopening windows.

- [x] Provide reusable knobs/faders/switches, exact entry/reset, graph input and bounded analyzer/meter snapshots, all bound to core preview/commit transactions. Input text suppresses shortcuts; otherwise transport and shared undo/redo work from editor focus.

- [x] Window geometry is presentation state, restored safely across DPI/monitor changes. Shared help/value display, focus, missing-owner and hidden-window behavior are defined; hidden visualizations avoid unnecessary work.

- [x] GUI commands open/focus/close and capture each viewport's actual rendered pixels with window, owner/effect and state revision metadata. Main/effect capture barriers report pending resources. Headless requests return structured capability errors without disabling DSP.

- [x] Focused gesture/host tests cover row reorder, bypass, no target, native-window lifecycle, focus shortcuts, stable bindings and per-window capture. Complete representative actual-viewport capture with the first effect in 30; final multiwindow/platform visual review belongs to 44.

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


## Implementation and verification — 24 October 2026

The compact Effects panel now binds whole-document or single-clip scope, uses
ordered grip/bypass/name/Open/menu rows, discovers only implemented constructors,
and exposes Bypass all and an output meter. Parameters remain in separate native
effect windows. Reverse/Undo/Redo are Edit actions; exact clip source trimming is
retained in the arrangement's Clip source menu.

Stable owner/instance bindings drive `effects.editor.open/focus/close`; duplicate
open focuses an existing window. Selection changes do not retarget it. Closing
leaves committed processing active and cancels its registered unfinished preview;
removal closes windows, while undo restores state without reopening presentation.
Finite bounded geometry persists in UI preferences, uses logical coordinates and
clamps on restoration. Up to 32 windows coexist. Hidden editors skip visualization
work unless a capture requests painting.

Shared controls provide knobs, faders, switches, exact entry/reset, fine adjustment,
keyboard values, parameter-map graph input and core preview/commit/cancel. A
preallocated 128-frame stereo output ring supplies bounded actual-output snapshots
for native displays. These displays identify the shared Transport output monitor;
effect-specific analyzer views are implemented with their respective algorithms.

Capture/barrier requests accept an editor ID and target that viewport's actual
readback context. Metadata identifies the window, owner, effect instance, committed
revision, effective preview identity, session/presentation revisions and pending
resources. Native-window commands return structured capability errors headlessly.
See [native editor contract](../../../docs/native-effect-editors.md).

Verification: the complete GUI-free suite passes, including direct/headless/MCP
WAV/MP3 workflow (`.tools/test29-nogui-full.log`); GUI-enabled library and application
tests pass (`.tools/test29-gui-complete.log`). Focused toolkit tests cover row bypass,
Open and drag reorder (`.tools/test29-rows.log`) and knob drag/fine keyboard input
(`.tools/test29-knob.log`). Host probes cover multiple stable bindings, focus,
close/cancel, removal, undo, geometry validation and missing capability. A viewport
routing probe verifies that root/other windows cannot fulfill an editor capture.
Fifteen manual transport cases include bounded actual stereo analyzer snapshots
and zero callback allocations/frees during document and clip processing changes.
Both all-target Clippy configurations pass with warnings denied; formatting passes.

No production processor is enabled by this framework ticket. As explicitly scoped,
ticket 30 owns the first production effect's inspected actual native-viewport PNG;
ticket 44 owns final multiwindow/platform visual and performance review. Shell
probes do not claim algorithm fidelity or cross-platform native acceptance.
