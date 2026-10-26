# Native effect editors

The Effects panel targets the whole active waveform document or exactly one
selected arrangement clip. Ordered compact rows provide bypass, Open controls,
move and remove actions. Drag a grip to reorder; the menu offers the same moves.
Add effect lists constructors registered by the algorithm tickets. Parameters
never expand inside rows. Reverse, Undo and Redo remain in Edit; exact clip source
bounds are in the arrangement's Clip source menu.

Choosing Add effect opens the newly created instance's editor immediately for
both document and selected-clip stacks. Closing it keeps processing active; undo
and redo do not open editors automatically.

`effects.editor.open`, `effects.editor.focus` and `effects.editor.close` take an
`owner` (`{"kind":"document","id":"…"}` or clip) and `instance_id`.
`presentation.get.editor_windows` returns stable editor IDs. Opening the same
pair focuses its existing native window; up to 32 windows can coexist. Main-view
selection does not retarget windows. Closing leaves committed processing active
and cancels an unfinished gesture registered to that window. Removing an instance
closes its editor; undo restores processing without reopening presentation state.
Headless editor requests return `missing_capability`.

Native editors share knobs, faders, switches, exact numeric values, double-click
reset and a bounded graph input surface. Shift gives fine knob adjustment; arrow
keys operate focused knobs. A gesture uses core preview/update/commit commands and
creates one history entry. Escape restores committed values. Editing another
control resolves the preceding gesture; stale/expired previews are replaced safely.
Each native window supplies contextual help and shared transport/history shortcuts.
Text editing and modals suppress global shortcuts.
Space plays/pauses, Enter or comma rewinds, and slash toggles looping even after
adjusting a focused control. Click a knob's value or press F2 on the focused knob
to enter an exact value; Enter accepts that entry without rewinding.

Editor geometry is saved through `effects.editor.geometry` in UI preferences,
outside audio history/projects. Sizes use logical points and clamp to the current
monitor; invalid/oversized geometry is rejected. Positions use the outer window
origin (including its frame), while sizes use the client area. Positions are
clamped only when restored; moving an open editor preserves its native position,
including negative coordinates and secondary monitors. A backend without window
coordinates still retains the size.
Minimized/occluded editors skip controls and visualization work except when a
capture explicitly requests paint. No FFT or visualization work enters the callback.

`transport.get.analyzer` contains at most 128 recent actual stereo output frames.
A preallocated ring and atomic snapshot transfer keep callback allocations, frees,
locks and I/O at zero. Snapshot reads allocate only on the control/UI thread. The
native display explicitly labels this shared monitor as Transport output; it does
not claim to measure an inactive instance. Per-effect analyzer presentations use
the shared bounded display/input surface as their implementation is introduced.

`capture.png` and `presentation.barrier` accept optional `editor_id`; omission selects
the main viewport. Readback is requested on the selected viewport's egui context.
Metadata includes editor/owner/instance IDs, committed processing revision, effective
processing identity, preview ID, session/presentation revisions and pending
resources. The effective identity distinguishes intermediate preview values.
Captures freeze mutations through framebuffer readback and publish PNGs atomically.
A closed target fails explicitly. A main capture never substitutes for a native
editor capture. Actual production-editor screenshots and visual inspection begin
with Soft Clipper (ticket 30); multiwindow/platform review is ticket 44.

Native windows use eframe's deferred viewport renderer and share the serialized UI
model through a GUI-only mutex. The bundled eframe immediate-window path paints
but does not complete screenshot actions; the first production-effect regression
exposed this. Deferred rendering delivers the actual child framebuffer readback.
Capture input locking retains normal control opacity, so capture requests do not
visually dim the editor. The real native regression covers open, changed controls,
window reuse, close and refusal to capture a closed target.
