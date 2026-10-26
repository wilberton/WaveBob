# WaveBob editor specification

Approved import extension, 26 October 2026: [ticket 62](issues/62-load-ogg-vorbis-audio.md)
adds mono/stereo Ogg Vorbis (`.ogg`) loading and browser audition through the same
GUI, headless and MCP workflows as WAV/MP3. Working audio and project assets stay
lossless. Ogg sources use explicit WAV/MP3 Save As; Ogg export is outside scope.
Single logical Vorbis streams are supported; chained/multiplexed Ogg and other
Ogg codecs are rejected explicitly.

Approved follow-up, 26 October 2026: [ticket 61](issues/61-unify-knob-dragging-and-compact-effect-windows.md)
supersedes the original knob sizes and gestures across every effect. The whole
knob uses vertical-only dragging with a shared approximately 180-point normalized
sweep, including nonlinear controls; dials are 60% of the old default size and
effect windows compact accordingly. Preserve typed entry, keyboard, reset,
preview/undo/cancel, every parameter mapping and the shared GPU renderer.

Approved rendering prerequisite, 25 October 2026:
[shared effect visualization rendering](effect-visualization-rendering.md) is
part of this specification. Execute tickets 56–59 before the nine effect-window
refinements in 47–55. Pool all glowing shapes into one emission image per native
window, GPU-blur that image and additively composite it before sharp foreground
detail. Later ticket 60 adds/tunes multiple whole-image blur scales from the same
pooled source; it does not assign blur radii to individual elements or block the
first UI pass. Use one GPU implementation with no fallback rendering path. The
renderer preserves all DSP, parameter and gesture contracts.

Approved visual refinement, 25 October 2026: implement the nine remaining
effect windows from the [mockup gallery](../../docs/design/effects-suite/README.md)
under the [effect-window refinement contract](effect-window-refinements.md)
and tickets 47–55. Reuse the Reverb/Distortion panel controls and luminous blue
visual styling. Each ticket embeds its screenshot for close visual comparison.
Keep the current graph-led EQ with selected-band controls: horizontal node
dragging edits frequency, and Shift + vertical node dragging edits bandwidth.
The proposed seven-column stacked-dial alternative was withdrawn. Preserve all
existing DSP and parameter behavior.

Naming update, 25 October 2026: the effect previously displayed as Fast
Distortion is now **Distortion**. Its stable command/project kind remains
`fast_distortion`. The requested [Distortion visual concept](../../docs/design/distortion/distortion-mockup-v1.png)
is approved for implementation in [ticket 46](issues/46-refine-the-distortion-window.md).
Reuse the Reverb panel controls and preserve the blue glow on the actual
transfer curve. The DSP and reference parameter mappings remain unchanged.

Approved refinement, 25 October 2026: Reverb's GUI follows the
[six-dial mockup](../../docs/design/reverb/approved-six-dial.png) and
[ticket 45](issues/45-refine-the-six-dial-reverb-window.md), superseding its older
reference-room layout. The six controls are Size, Decay, Pre-delay, Damping,
Width and Mix, plus Bypass. The underlying reference DSP and full command
parameter set remain available.

**Status:** ready-for-agent

Approved and published to the local Markdown tracker on 23 October 2026. The
command test seam and ticket breakdown are approved. Version 1 includes MP3
import and export. The generated WaveBob mockups are the visual reference;
Artcraft remains architectural background rather than a styling target.

[Accepted UI mockups](../../docs/ui-concept-01.png) | [Implementation tickets](README.md)

Approved extension, 24 October 2026: [Live effect stacks, waveform envelopes and
linked arrangement bounces](effects-and-clip-controls.md) is part of this canonical
specification. It defines all eleven built-in effects, reference-based WaveBob
implementations, separate egui control windows, clip envelopes and linked bounces.
The [accepted effects mockup](effects-stack-mockup-v1.png) governs the changed
panel/window layout. Tickets 27–44 implement this extension; older completed
tickets retain their historical acceptance records.

## Problem Statement

Sound designers need a focused, cross-platform tool for editing individual audio
files and arranging clips across tracks. They need to move quickly between their
filesystem, waveform editing, effects and an arrangement without losing context.
An AI must be able to perform the same editing work through structured commands,
inspect the resulting state and audio, and capture the actual editor interface
to check its work. Editing logic duplicated inside widgets or automation tools
would make those workflows inconsistent and difficult to test.

## Solution

Build WaveBob in Rust with egui, matching the accepted WaveBob UI mockups.
Provide waveform and multitrack modes inside a common shell: open
files and a browser at left, effects at right, centered transport below the main
view, and a persistent status bar at the very bottom. Both side panels resize
by dragging, collapse independently using small edge arrows, and toggle together
with the unmodified backtick key.

Separate structural edits from nondestructive gain/fade envelopes and live effect
stacks. The right panel holds compact effect rows; each effect opens a separate
native egui control window. Render document processing into lossless bounces for
all linked arrangement clips; preserve tails without moving later clips.

Put editing, transport and project behavior behind a shared command interface.
The egui frontend, headless runner and MCP adapter all use that interface. Add
structured state inspection, offline audio rendering and command-driven PNG
capture of a running GUI, allowing AI-driven changes to be checked both aurally
and visually. Headless editing must run without a window or graphics device.

## User Stories

1. As an editor, I want the same application on Windows, macOS and Linux, so that I can use my preferred workstation.
2. As an editor, I want the interface to match the accepted WaveBob mockups, so that the agreed visual design is carried through into the application.
3. As an editor, I want to switch between waveform and multitrack modes, so that I can edit details and arrange sounds in one workspace.
4. As an editor, I want all open audio documents listed at left, so that I can switch quickly between samples.
5. As an editor, I want clicking an open document to display its waveform, so that document navigation is predictable.
6. As an editor, I want to browse folders and audio files beneath the open-file list, so that I can find material without another application.
7. As an editor, I want to audition browser files without opening documents, so that searching does not clutter my workspace.
8. As an editor, I want double-clicking a browser file to open it, so that useful material is immediately editable.
9. As an editor, I want to drag open files onto tracks, so that arranging requires little setup.
10. As an editor, I want to resize either side panel, so that filenames, parameters and audio can receive the space they need.
11. As an editor, I want a small arrow to hide or restore each panel, so that I can reclaim editing space.
12. As an editor, I want backtick to toggle both panels, so that I can quickly focus on the audio.
13. As an editor, I want panel sizes and visibility remembered, so that the application reopens in a useful layout.
14. As an editor, I want a status bar showing cursor and selection state, so that I can see precise timing while working.
15. As an editor, I want hover help in the status bar instead of floating tooltips, so that help does not obscure the waveform.
16. As a keyboard user, I want focused controls to provide the same help, so that essential guidance does not depend on a mouse.
17. As an editor, I want a full-sample waveform overview above the main waveform, so that I retain context while zoomed in.
18. As an editor, I want an overview rectangle showing the visible interval, so that I know which portion I am editing.
19. As an editor, I want to drag the overview rectangle to pan, so that I can navigate long samples quickly.
20. As an editor, I want to resize the overview rectangle by its edges, so that I can change the zoom directly.
21. As an editor, I want mouse and trackpad zoom and pan, so that navigation feels natural on each platform.
22. As an editor, I want a click to place the cursor and a drag to select time across channels, so that editing and playback have precise targets.
23. As an editor, I want exact selection start, end and duration readouts, so that I can verify an edit before applying it.
24. As an editor, I want cut, copy, paste, delete and trim, so that I can perform basic waveform editing.
25. As an editor, I want gain/fade handles on waveforms and clips, structural reverse, and eleven built-in effects, so that editing and sound shaping have appropriate controls.
26. As an editor, I want a live effect stack for the whole document or one selected clip, so that I can hear parameter changes without Apply.
27. As an editor, I want compact stack rows that open separate effect control windows, so that detailed controls remain accessible without filling the side panel.
28. As an editor, I want undo and redo, so that I can experiment without losing previous work.
29. As an editor, I want Space to play or pause from the current cursor, so that playback is quick to control.
30. As an editor, I want Enter or comma to rewind to the start, so that I can restart playback consistently.
31. As an editor, I want slash to toggle looping, so that I can repeatedly inspect a sound or time selection.
32. As an editor, I want transport controls centered below the editing area, so that both modes have a familiar playback location.
33. As an arranger, I want multiple named tracks, so that I can organize independent sound layers.
34. As an arranger, I want track mute, solo, volume and pan, so that I can balance the arrangement.
35. As an arranger, I want to position and move waveform clips in time and between tracks, so that I can build an arrangement visually.
36. As an arranger, I want two clips overlapping on a track to crossfade automatically, so that transitions do not require manual setup.
37. As an arranger, I want overlap regions and clip positions to be visible, so that the resulting mix is understandable.
38. As an arranger, I want clip-specific effects to leave other instances intact, so that shared material can be reused safely.
39. As an editor, I want to save samples, save and reopen projects, and export a mix, so that the results survive outside the session.
40. As an editor, I want clear progress, cancellation and recoverable errors, so that large files and failed operations do not freeze or corrupt my work.
41. As an AI operator, I want a discoverable command catalogue with typed parameters and results, so that I can edit without guessing UI gestures.
42. As an AI operator, I want stable document, track and clip identifiers, so that commands act on the intended objects.
43. As an AI operator, I want queries for documents, selections, tracks, clips, transport and job state, so that I can plan and verify each operation.
44. As an AI operator, I want headless editing and rendering, so that automation can run without a desktop session.
45. As an AI operator, I want MCP access to the same commands, so that my client can discover and drive the editor.
46. As an AI operator, I want to attach to an explicit running GUI session, so that commands and screenshots refer to the same workspace.
47. As an AI operator, I want a screenshot command returning an actual rendered PNG and state revision, so that I can inspect the visible outcome of my work.
48. As an AI operator, I want to choose the active mode, document, viewport and panel layout through commands, so that screenshots show the relevant work.
49. As a developer, I want GUI, headless and MCP operations to have equivalent outcomes, so that behavior can be maintained in one place.
50. As a developer, I want deterministic audio fixtures and observable command results, so that correctness can be verified without relying on screenshots alone.
51. As an editor, I want to import and audition MP3 files in version 1, so that I can edit and arrange existing compressed audio.
52. As an editor, I want to export edited samples and arrangements as MP3 in version 1, so that I can share compressed audio directly from WaveBob.
53. As an arranger, I want document changes bounced to all linked clips when returning to multitrack, so that the arrangement uses my current edited sound.
54. As an arranger, I want preserved effect tails without moving later clips, so that decay remains natural and timing stays fixed.
55. As an editor, I want projects to retain editable stacks and sources while exports contain rendered audio, so that I can revise my work later.

## Implementation Decisions

### Confirmed direction and version 1 defaults

The two modes, side-panel contents, waveform overview interactions, transport
shortcuts, structural editing, automatic crossfades, Rust/egui,
desktop platforms, command-driven architecture, headless/MCP access, screenshots,
status-bar help and panel controls come from the user. Match the generated
WaveBob mockups, adding the requested status bar and panel arrows. Their layout,
spacing, typography hierarchy, waveform/clip colors and control styling are the
visual acceptance reference. The accepted effect-stack/window mockup governs the
updated right panel and separate editors. MP3 import and export remain required.

The remaining decisions below define the first-release defaults. In particular:
file support is mono/stereo WAV, MP3 and Ogg Vorbis; edits are reversible; new clips link to
their source documents and retain immutable bounces; clip effects affect only
that instance; loop bounds follow the body selection; and screenshots require a
GUI renderer. The detailed effects contract defines parameters, limits and
reference provenance. Pre-alpha project/command migration is not required.

### Domain and ownership

- An **audio document** is an open sample with a stable identifier, metadata,
  immutable audio revisions, a working revision, an edit cursor and an optional
  time selection, plus a nondestructive gain/fade envelope and effect stack.
  Opening a file does not overwrite it.
- An **audio revision** is immutable decoded audio plus its sample rate and
  channel layout. Structural edits and rendered bounces produce audio revisions;
  envelope/stack edits change processing state and render identity without
  replacing source PCM. History can share retained immutable revisions.
- A **selection** is a half-open sample-frame interval within one document. It
  spans all channels in the initial release. Empty ranges resolve to no selection;
  nonempty ranges contain at least one frame. Cursor, selection and visible
  interval are separate state.
  In the main waveform, dragging either visible selection edge adjusts that
  boundary with the opposite edge fixed. Crossing it normalizes the interval;
  Escape cancels the preview. Hovering an edge shows a horizontal resize cursor.
- An **arrangement** contains tracks, clips, a timeline cursor and an optional
  time selection. A **track** has a stable identifier, name, mute, solo, volume
  and pan. A **clip** refers to an audio revision, source interval, track and
  timeline position, plus instance-specific processing.
- New clips retain an explicit document link and their last successful lossless
  bounce. Returning to multitrack or rendering requires a current bounce and
  refreshes all linked instances atomically, preserving placement/instance state.
  Make independent retains a pinned source snapshot. Closing a waveform tab does
  not delete a document source still referenced by linked clips.
- Clip body/source intervals define trim and crossfade geometry; effect tails
  extend audible duration separately. Render identities include source revision,
  envelope, ordered stack/bypass, effect tempo, tail policy and DSP version.
- Document coordinates use integer sample frames. The arrangement uses integer
  frames at its project rate, defaulting to 48 kHz; conversion and rounding rules
  are centralized. Mixed-rate sources are resampled for mixing without rewriting
  their original revisions. Clip positions cannot be negative.
- Layout preferences, hovered control and active view belong to presentation
  state. They do not enter audio undo history. Sample and arrangement edits share
  chronological session undo/redo; selection, navigation and transport do not.

### Shared command interface and module separation

- The core editing module owns domain state, validation, history and operation
  orchestration. Its interface accepts commands and queries and returns typed
  results, immutable snapshots and events. It has no egui types or windowing
  dependency. A headless executable must compile and run without the GUI module.
- The audio module hides decoding, immutable audio storage, DSP, waveform peak
  generation, resampling, mixing, offline rendering and device playback. The core
  coordinates these capabilities through a small interface rather than exposing
  DSP internals to callers.
- The egui module renders snapshots, maintains short-lived gesture previews and
  translates committed gestures into commands. It contains no alternate editing
  implementation. A drag becomes one undoable edit on release; cancellation
  discards its preview. Playback can use a preview graph without recording every
  pointer movement in history.
- CLI/headless and MCP are adapters over the same command interface. Formatting,
  transport framing and tool discovery belong in adapters; validation and editing
  semantics remain in the core. Both can target their own headless session or an
  explicitly selected running GUI instance.
- A desktop host provides presentation commands and screenshot capture through
  the same command dispatch entry point. These capabilities are separate from
  editing and return a structured capability error in a host without a renderer.
- Group commands by capability: session/query, documents/edit/history, selection,
  arrangement/tracks/clips, effects, transport/audition, jobs/render, persistence,
  presentation and capture. Provide command names, descriptions, versioned input
  and output schemas and capability discovery from one registry. Exact operation
  spelling is chosen during implementation; schemas must not depend on widget
  structure or pixel coordinates.
- Requests carry a request identifier and explicit object identifiers. Mutations
  support an expected session revision to reject stale writes. Results report
  success or stable error codes, affected identifiers and the committed revision.
  Queries expose audio metadata, history availability and effective mix state as
  well as object and selection state.
- Stack/envelope commands accept explicit document/clip owners and expected edit
  revisions; selection does not retarget them. Structural edits use explicit
  ranges or whole-document targets. All clients share preview/update/commit/
  cancel transactions, stable effect/parameter IDs and validated typed metadata.
- Committed mutations are serialized and atomic. Invalid identifiers, stale
  revisions, out-of-range coordinates and invalid parameters fail without partial
  changes or new undo entries. Long operations return a job identifier, expose
  progress/completion/failure/cancellation, and publish results only at commit.
  A job based on a superseded revision fails as stale rather than overwriting a
  newer edit. Cancellation preserves the pre-operation state.
- Filesystem work, graph/kernel preparation, offline DSP and peak generation run
  away from UI rendering. The audio callback executes bounded streaming DSP using
  prepared state and preallocated buffers; it performs no file access, allocation,
  buffer destruction or blocking locks. Queries distinguish requested/applied
  transport and processing generations. Compensate processor latency, including
  dry/bypassed paths; preserve intentional predelay/echo timing.

### Headless, MCP and screenshots

- The headless runner accepts newline-delimited JSON requests on standard input
  and emits machine-readable responses/events on standard output. Diagnostics use
  standard error. Offline editing, save and mix rendering require no audio output
  device. Device playback is an optional advertised capability.
- The initial MCP adapter uses stdio and exposes catalogue, execute, query and
  job/capture results with discoverable schemas. It reuses existing implementations
  when editing commands are added; it does not maintain a second handwritten
  editing catalogue. MCP framing stays confined to its adapter.
- GUI attachment uses explicit instance identity and local IPC. Session discovery
  identifies instances without silently choosing the first. Requests and captures
  must resolve to the same session. Remote network hosting is outside this release.
- Presentation commands select mode/document, set visible interval, set panel
  sizes/visibility and report layout state. Commands can request a render barrier
  after relevant jobs complete. Pure headless sessions still report unavailable
  rendering rather than fabricating an image.
- Presentation commands also open/focus/close effect control viewports and capture
  each viewport's actual pixels with owner/effect identity and state revision.
  Capturing the main window alone does not capture separate native windows.
- Capture waits for a GUI frame reflecting at least the requested committed
  revision and presentation state; the result reports the revision actually
  rendered. Pending resources/jobs are reported. A timeout produces an explicit
  failure. Playback may be paused for deterministic captures by a separate command.
- Capture exports the complete client area as PNG, including panels, transport
  and status bar, at known pixel dimensions and scale. It reports session identity,
  rendered revision, active mode, dimensions and an output artifact reference.
  Callers may supply an output destination; existing files require explicit
  overwrite. MCP returns image content or a readable image resource in addition
  to metadata so clients can inspect the pixels.
- Native file dialogs and OS window chrome are outside client-area capture. GPU
  or window initialization failures are reported. GPU-free headless screenshot
  rendering is deferred; normal headless editing must remain available.

### Shell, panels and status bar

- Match the accepted WaveBob mockups: dark surfaces, fine borders, blue waveform
  and selection accents, track colors, compact controls, spacing and typography
  hierarchy. Compare actual screenshots to the mockups in both modes, allowing
  the requested status bar and panel controls. Initial font and helper-library
  choices must respect their licenses and the chosen egui version. Craft helper
  libraries are optional implementation aids and must not redefine the visuals
  or dictate the core architecture.
- Place transport below the central work area and the status bar at the absolute
  bottom, across the application width. Both remain visible when panels collapse.
- Resize the left and right panel from their inner edges. Persist widths in
  logical pixels and clamp them to preserve a usable center when the window
  shrinks. Defaults: left 250, right 240, minimum expanded width 180,
  minimum center width 400. Restore the last expanded width when reopening.
- Each panel has a small chevron pointing in its collapse direction. Collapsed
  panels retain a slim edge affordance to restore them. The left panel also has a
  draggable vertical divider between open documents and browser; persist its ratio.
- Unmodified backtick hides both panels if either is visible; when both are
  hidden it restores both at their remembered widths. Independent chevrons affect
  only their panel. This behavior works in both modes and via presentation commands.
- Keep selection start/end/duration and cursor visible in a dedicated status-bar
  region. With no selection, explicitly show that state. In arrangement mode show
  timeline selection and selected-clip count; an empty workspace has an empty-state
  readout rather than fabricated timings.
- Show contextual control help and shortcuts in a separate status-bar region on
  hover, or keyboard focus when nothing is hovered. Clear stale help immediately
  when the target disappears or the pointer leaves. Disabled controls explain why
  they are unavailable. Waveform and timeline regions describe their gestures.
  General help stays in the status bar. Gain/fade handles additionally show a
  compact local numeric value label on hover/focus/drag; explanatory tooltips
  remain excluded. Effect windows provide their own local status/value help.
- Progress and persistent errors have their own visible treatment and remain
  available when hover help changes. At narrow widths preserve selection state
  and truncate help first. Keyboard shortcuts are suppressed while typing, in
  native dialogs, or when an open modal owns the input; supported actions remain
  available through buttons/menus and commands.

### Files, waveform navigation and editing

- Version 1 decoding and writing cover mono/stereo WAV (integer PCM 16/24/32 bit
  and 32-bit float) and MP3. MP3 import supports CBR and VBR material; decoding
  produces the same immutable floating-point audio revisions as WAV. Opening,
  browser audition, waveform edits and arrangement insertion all accept MP3.
  Corrupt streams, unsupported files, missing files, empty audio and access errors
  return actionable failures without partial document publication.
- Honor available MP3 encoder-delay and end-padding metadata when deriving the
  editable decoded interval; do not infer silence trimming when it is absent.
  Expose decoded frame count/rate, source encoding and available bitrate mode
  separately. Editing coordinates refer to decoded audio, never compressed frames.
- MP3 export is available for individual documents, selected ranges and arrangement
  mixes through GUI, headless and MCP. Version 1 offers CBR 128/192/256/320 kbit/s,
  default 192 kbit/s; VBR export is optional. Preserve mono/stereo layout. Offer
  output rates 32/44.1/48 kHz, preserving the source/project rate when listed and
  otherwise defaulting to 44.1 kHz, with explicit resampling in the export summary.
  Show that export is lossy, use explicit overwrite intent, and write delay/padding
  metadata so a capable decoder can recover the intended interval.
- Both MP3 decoding and encoding must work in the shipped builds on all three
  platforms and in device-free headless sessions. Users must not need to install
  an external codec executable. Choose and document the backend, reproducible build
  inputs and redistribution/license requirements during implementation; dependency
  choice is not fixed by this spec. MP3 codec jobs run off the UI/audio callback.
- Open via browser double-click, an Open action or native file drops onto the
  editor. Batch drops queue WAV/MP3 imports through the shared commands within
  worker limits; rejected paths/errors remain visible without blocking other files.
  Drop hover feedback identifies audio loading and does not place clips implicitly.
  Clicking the open-document
  list activates its waveform mode. Show sample rate, channels, encoding, duration
  and dirty state. Closing the active document chooses a remaining document or
  an empty view. Unsaved edits require save/discard/cancel in the GUI and an
  explicit policy from automation.
- Browser navigation lists parent/folders/audio files without blocking the UI;
  errors leave navigation usable. Selecting a file exposes audition controls;
  audition is explicit and does not create an audio document. Double-click opens
  the same file without racing a queued audition. Audition policy: one
  preview at a time, pause editor playback when preview starts, stop preview when
  editor playback starts, and never resume either automatically.
- Cache multiresolution waveform peaks per immutable revision in background jobs.
  Render appropriate detail for the visible span; zoomed views align stereo
  channels and their selections on the same time axis. At individual-frame zoom,
  display actual sample positions. The overview always spans the complete document.
  Peak envelopes must retain a visible continuous trace for flat or subpixel-width
  amplitude bands at intermediate zoom; do not rely on zero-area filled triangles.
- A primary click places the cursor and clears the selection; primary drag sets
  a selection, with its start as edit cursor. Normalize reversed drags and clamp
  at sample bounds. Provide commands to set exact frames, select all and clear.
  Selection persists while navigating and panning.
- Gestures: horizontal scroll/two-finger horizontal motion pans time;
  Ctrl/Cmd plus vertical scroll and pinch zoom around the pointer; plain vertical
  scroll pans time in waveform mode and tracks in multitrack mode; middle-button
  drag pans time. Controls and focus help document these gestures on each OS.
  Provide zoom-in/out and fit actions so gestures are not the only access path.
- Drag the overview viewport body to pan without changing its duration. Drag an
  edge with the opposite edge anchored to resize the visible interval. Clamp all
  navigation to the sample; minimum viewport span is one sample frame. Main and
  overview representations update together, including a correctly zoomed ruler.
- Basic edits include cut/copy of a selection, delete, trim-to-selection and paste
  of an internal audio clipboard at the cursor, replacing an active selection.
  Paste converts to the destination rate; initial channel conversion supports
  mono/stereo with documented duplication/downmix rules. Invalid/empty targets
  have explicit behavior. Copy and navigation do not dirty audio.
- Structural reverse remains an Edit action with an explicit editing target.
  Copy/cut use source PCM without baking the live envelope/stack; range export
  provides processed audio. Paste does not import source processing state.
- Document and clip gain/fades are nondestructive envelopes, adjusted by the
  top-left/right fade triangles and top-center gain square. Fades use linear
  amplitude and source-frame durations; overlapping ramps multiply, and the
  single-frame convention is unity. Handles ignore the current selection and
  refer to true body edges, with exact numeric/keyboard access when offscreen.
- Compact stack rows open separate modeless native egui editors. All eleven
  effects are required: Parametric EQ, Limiter, Reverb, Delay, Chorus, Fast
  Distortion, Soft Clipper, Balance, Stereo Enhancer, Stereo Shaper and Transient
  Processor. Use the reference algorithms in WaveBob-owned code and reproduce
  useful source UI layouts. Advanced EQ quality modes, external sidechains and
  Stereo Shaper difference-output routing are deferred. Plugin hosting and
  upstream binary/project-state compatibility are excluded.
- Stacks process their entire owner: active document or exactly one selected
  arrangement clip. Parameters preview live without Apply; one completed gesture
  is one undo step and Escape reverts it. Open editors stay attached to stable
  owner/effect IDs when selection changes; closing the window leaves sound active.
- Document order is source -> envelope -> stack -> bounce. Arrangement order is
  bounce interval -> clip envelope -> clip stack -> crossfade -> track -> master.
  Bake document processing once. Preserve tails with the detailed contract's
  threshold/hold and 60-second cap, recording capped-tail truncation explicitly.
  Mono sources may produce stereo processed audio; mono export downmix is explicit.
  Tempo sync uses a visible project Effect tempo (initially 120 BPM), shared by
  live/offline rendering; free time remains the default and no beat grid is added.
- The [detailed effects contract](effects-and-clip-controls.md) is authoritative
  for effect scope, gestures, smoothing, tails, reference validation and limits.
- Keep internal processing in floating point. Report overloads; clamp or quantize
  only as specified by export encoding. Undo restores audio and relevant edit
  selection/cursor; redo reapplies the same result. Length-changing edits remap
  selection/cursor into the result and clamp the visible interval.

### Playback and arrangement

- Space and the play/pause button use the active document or arrangement cursor.
  Pause retains the current position. Enter/comma seeks to absolute start without
  changing the existing playing/paused state. Slash and the loop button toggle
  looping without changing the selection.
- With loop enabled, loop the nonempty selection, otherwise the entire active
  document/arrangement. Starting outside an active loop begins at its start.
  Empty arrangements do not play. Without looping, playback drains contributing
  effect tails and stops at the audible end; playing again starts at the beginning.
  Selection controls loop bounds but does not limit ordinary non-looped playback.
- Prepared graph changes use safe block handoffs and smooth transitions while
  preserving transport position. Seek resets effects with prepared context;
  pause retains state and loop wraps preserve it. Mode/document switches pause.
  Entering multitrack prepares stale linked bounces before playback/export;
  progress/cancellation and failures remain visible. Track/device failures recover.
- Create, rename, reorder and delete tracks. Place an open document onto a track
  by dragging; the clip initially links to the document and spans its source body.
  Its rendered tail extends separately. Display
  a drop preview and commit once on release. Drag existing clips to reposition or
  change track, preserving their source interval. Off-track drops cancel.
- First-release arrangement timing uses seconds and sample frames, with no
  mandatory grid snapping or tempo model. Moving clips and trimming their ends
  use the same command validation as exact numeric automation operations.
- Tracks expose mute, solo, volume in dB and stereo pan. When any track is soloed,
  only soloed, unmuted tracks contribute. Mute takes precedence over solo. Default
  pan law: equal-power for mono, stereo balance preserving stereo channels. Mix
  to stereo and expose master level/overload state.
- Derive automatic crossfade envelopes from current clip geometry; never rewrite
  source audio. For ordinary staggered overlaps, fade the earlier clip out and
  later clip in over the full intersection using equal-power curves. Repositioning,
  trimming or undoing recomputes the region, shown as an X overlay.
  A visible handle at the right endpoint trims the earlier clip's end to adjust
  the crossfade length. For containment it trims the contained clip; coincident
  clips use insertion order. The handle takes precedence over clip bodies,
  shows a resize cursor and status help, and uses source-bounded trim preview,
  one undoable commit on release and Escape cancellation.
- Crossfade geometry uses bodies, not tails. Tail overlap does not move later clips
  or create crossfades. Apply crossfade envelopes to processed output and hold the
  boundary value into a tail. Tail voices may overlap more than two bodies' voices
  within explicit resource limits; the two-body placement restriction remains.
- The bounded first release supports at most two simultaneous clips on one
  track. Reject a third simultaneous overlap atomically, with a visible placement
  explanation. For equal-start clips use stable insertion order for transition
  ordering. For a fully contained clip, dip the containing clip and fade the inner
  clip in/out over the two halves of the contained interval, returning to the
  containing clip afterward. Define one-frame degeneracies without NaNs and
  verify resulting audio, not only the overlay.
  Shared-start/shared-end pairs of unequal lengths use containment; fully
  coincident clips use stable insertion ordering for the ordinary transition.
  Containment of one/two project frames shares both voices at 1/√2. Longer
  containment uses sin(πt) for the inner voice and |cos(πt)| for the outer voice.
- Offline export and live playback share source interval, document bounce, clip
  envelope/stack, latency compensation, resampling, crossfade, track gain/pan,
  mute/solo and master semantics. Arrangement end includes contributing tails.
  Export is device-free and waits for coherent current linked bounces.

### Persistence, errors and platform completion

- Save samples through explicit save/save-as/export actions; retain original
  WAV encoding on Save and offer explicit WAV/MP3 export choices. The initial
  default for new exports is 24-bit PCM WAV. Saving an edited MP3 requires an
  explicit encoding summary/confirmation in the GUI, or explicit MP3 format,
  bitrate and overwrite intent from automation; never silently re-encode it.
  Offer lossless WAV Save As as an alternative. MP3 export preserves the current
  lossless working revision and marks that revision saved only for a document
  save, rather than replacing it with a re-decoded lossy result. Save writes
  atomically and never marks a newer revision saved when an older job completes.
- Save/export renders the current envelope/stack and tails, while retaining the
  editable source/processing in session. Saved render identities are separate
  from source revisions; envelope-only edits dirty the sample and project.
- Save projects as a versioned manifest with a managed asset directory containing
  referenced immutable audio revisions, arrangement state and clip processing.
  Source paths are provenance, not a requirement for reopening a saved project.
  Persist MP3-origin working audio as lossless managed assets; project saves must
  not introduce additional MP3 encoding generations.
  Reopen restores editable audio and arrangement state; undo history need not
  survive restart. Detect missing assets and unsupported schema versions without
  silently discarding state. UI layout preferences are stored separately.
- Persist envelopes, all eleven effect types/parameters/order/bypass, explicit
  links, effect tempo, render identities, tail policy and lossless bounce assets.
  Reopen must work without original sources or an external cache. Replace/version
  the pre-alpha schema as needed; old projects/presets/commands need no migration.
  Record reference provenance and applicable notices with distribution sources.
- Project save/export jobs report progress, clean temporary output after failure
  or cancellation, and replace an existing destination only with explicit intent.
  The GUI prompts before discarding dirty work. External commands specify policy
  instead of waiting for an invisible dialog.
- Exercise Windows, macOS and Linux builds. Verify native paths, high-DPI layout,
  wheel/trackpad behavior, resize/collapse affordances, keyboard handling and audio
  device errors. Linux covers X11 and Wayland smoke checks where available.
  Provide launchable build artifacts and documented runtime prerequisites; signing,
  notarization, installers and automatic update distribution are follow-on work.

## Testing Decisions

- Approved primary test seam: the public application command/query interface.
  Most behavior tests send commands, await jobs and assert resulting snapshots,
  history, saved/reloaded state and rendered audio. The headless adapter is the
  simplest harness; assertions do not inspect private fields or module structure.
  The user has confirmed that this seam matches their expectation.
- Extend the existing command/query and adapter tests, replacing obsolete
  destructive-effect assumptions with live state/envelope/stack behavior.
- Use generated impulses, ramps, known stereo frames, sines and silent intervals
  as deterministic audio fixtures. Check exact frame counts and unchanged regions;
  use documented numerical tolerances for resampling, reverb and mixing. Verify
  fade boundaries, gain conversion, channel mapping and crossfade endpoints.
- MP3 fixtures cover mono/stereo, CBR/VBR import, tag presence/absence, corrupt or
  truncated input and exports at every offered bitrate/rate. Test with an independent
  decoder as well as the chosen backend. Compare alignment, intended duration,
  channel layout and signal error using codec-appropriate tolerances, not exact
  PCM equality. Validate delay/padding metadata and explicitly account for streams
  without it. Exercise sample and arrangement MP3 export from GUI/headless/MCP,
  including cancellation, overwrite refusal and reimport of edited exports.
- Test document editing, history, arrangement mixing and persistence through the
  shared seam. Include no-op and invalid commands, cancellation, stale revisions,
  missing assets, failed writes, overlap conflicts and job completion ordering.
  No failed mutation may partially change audio or history.
- A small adapter contract suite drives equivalent scenarios via in-process
  commands, headless JSON and MCP, comparing public state and audio outcomes.
  It also verifies catalogue/schema consistency and structured error conversion.
- Use focused egui interaction tests for behaviors only gestures can demonstrate:
  click-versus-drag, overview move/resize, zoom anchoring, clip drop, panel drags,
  chevrons, backtick, focus guards and status-bar help without tooltip popups.
  These tests verify commands and visible outcomes rather than widget internals.
- Verify all eleven same-algorithm implementations against pinned reference
  renders, with declared numerical tolerances and documented intentional
  differences. Verify callback allocation/load, parameter smoothing, latency,
  tail termination, channel policy, tempo and seeded modulation. Each effect
  ticket includes DSP, its egui editor and public-seam validation.
- Capture actual rendered PNGs after a state/render barrier for both modes, panels
  hidden/restored, narrow/high-DPI layouts and status help. Assert dimensions,
  revision metadata and representative visible content; inspect representative
  images visually. Avoid one pixel-perfect baseline across all platforms.
  Compare representative captures against the accepted WaveBob mockups, including
  spacing, proportions, colors and control styling, plus the added status bar and
  chevrons. Matching Artcraft applications is not an alternative visual criterion.
- Test playback timing and looping against a controllable output adapter, and
  compare its mixed frames with offline export. Hardware smoke tests verify audio
  initialization, pause/seek, device failure and callback operation on each OS.
- Include an end-to-end AI workflow: import a fixture, set waveform handles,
  change/reorder live effects in separate windows, undo/redo, switch to multitrack
  and refresh all linked clips, edit one instance, inspect tails/crossfades, save/
  reopen, export WAV/MP3 and capture both main/effect windows. Repeat the domain
  operations headlessly and through MCP; unsupported capture fails explicitly.
- Keep a separate large-file responsiveness check: background import, peak build,
  effect and export jobs must leave UI interaction responsive and must not perform
  blocking work in the callback. Establish measured memory/latency baselines during
  implementation instead of promising an unmeasured maximum file duration.

## Out of Scope

- Recording audio, MIDI, virtual instruments, tempo/beat grids, automation lanes,
  time stretching, pitch correction and third-party effect plug-in hosting.
- More than stereo output, per-channel editing, surround workflows and initial
  import/export codecs beyond the explicitly listed WAV variants, MP3 and Ogg Vorbis import.
- More than two simultaneous clip bodies per track, arbitrary overlap graphs and a
  user-editable crossfade-curve editor in the first release.
- Selection-only live effects, track/master stacks, external sidechain routing,
  Stereo Shaper extra outputs, advanced EQ linear-phase/HQ modes and old pre-alpha
  project/preset/command compatibility. Effect BPM is not a tempo/beat-grid feature.
- Cloud storage, collaboration, remote MCP servers, autonomous AI planning inside
  WaveBob, and natural-language commands interpreted by the editing core.
- GPU-free raster capture, recording the desktop, and capture of native dialogs.
- Persisted undo across restarts, crash recovery/autosave, signed installers,
  automatic updates, and importing/exporting other DAWs' project formats.

## Further Notes

The user requested a dedicated visual refinement pass immediately after ticket
05. Ticket 22 implements that step without renumbering published tickets. It uses
actual captures to match both modes to the accepted mockups as closely as possible,
with the requested status bar and panel controls retained. Later editor work is
gated on this pass. Native macOS/Linux CI is deferred until a remote exists.

This specification supersedes unresolved behavior notes in the earlier concept
brief. The concept image remains a visual reference; it does not accurately show
the zoomed ruler relationship and predates the requested status bar and chevrons.
The original publication began from a scaffold. Ticket verification records now
describe delivered baseline capabilities; the effects extension remains work to
implement. Publication alone is never evidence of runtime support.

The user approved the command seam and numbered ticket breakdown, chose a local
Markdown tracker, required MP3 import/export in version 1 and clarified that the
WaveBob mockups govern appearance. Tickets are published individually with
`ready-for-agent` status and explicit blocking edges; only tickets whose blockers
are complete are available to start. No implementation is claimed by publication.

MP3 behavior references: the [LAME usage documentation](https://github.com/lameproject/lame/blob/master/USAGE)
describes bitrate modes and decode-delay handling, while the
[LAME encoder implementation](https://android.googlesource.com/platform/external/lame/+/c4cbe47910d2ece1617411206c6b5e4ffbe09360/libmp3lame/lame.c)
records encoder delay and end padding. These support codec test considerations;
they do not select WaveBob's codec backend.
