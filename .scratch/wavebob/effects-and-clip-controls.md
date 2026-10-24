# Live effect stacks, waveform envelopes and linked arrangement bounces

**Status:** ready-for-agent

Approved for publication on 24 October 2026, including the mockup, all eleven
effects and the limitations below. This document is the detailed effects/envelope
section of the [canonical specification](spec.md). Tickets 27–44 implement it;
publication does not claim runtime support or start implementation.

[Supplied clip-controls reference](effects-clip-controls-reference.png).

[Effect stack and separate control-window mockup](effects-stack-mockup-v1.png) |
[Lemon Studio effects investigation](lemon-effects-review.md).

WaveBob is pre-alpha with no existing projects, so project migration and legacy
sound/command compatibility are not requirements. Effect parameters belong in
separate native control windows; stack rows contain compact management controls.
Write WaveBob-owned implementations of all eleven effect algorithms, using Lemon
Studio's implementation as reference, and recreate the editors with egui. The
instruments and their internal synthesis effects are excluded.

## Requested behavior and confirmed decisions

- Replace the right-hand Edit & Effects tool selector and Apply workflow with an
  ordered effect stack. Hear parameter changes during playback immediately.
- Remove gain and fade tools from that panel. Provide fade-in and fade-out
  triangles at the waveform's top corners and a gain square at its top center,
  following the supplied reference image's interaction rather than its styling.
- Provide the same envelope controls on arrangement clips. Hover reveals the
  current fade duration or gain.
- Returning from waveform to multitrack prepares a bounced copy for arrangement
  playback, baking in the document's processing while retaining its editable
  source and stack.
- Confirmed in this discussion: update **all clips linked to the document**.
- Confirmed in this discussion: **preserve reverb tails without moving later
  clips**.

The defaults below are the accepted implementation contract, including signal
order, whole-owner processing and how tails interact with crossfades.

## Reference-based implementation and effect scope

Use the reviewed Lemon Studio commit
`0af860dfd31c7ee9cbf8af6747617e031a1a7742` as the algorithm and parameter reference.
Write focused WaveBob DSP modules and egui editors containing the required parts.
The production app does not depend on Lemon Studio's engine, plugin crates,
NIH-plug, native Canvas/window adapters, FL Studio state formats or host-link code.
Upstream may be used in an isolated reference-render tool during verification.

For each effect, record the algorithm, parameter mapping and selected source
references in implementation notes, including intentional differences for
real-time updates, supported controls and channel/latency/tail behavior. Preserve
provenance and applicable copyright/license notices for referenced/adapted work;
rewriting from inspected source is not a claim of clean-room independence or an
automatic exemption from the upstream GPL terms. WaveBob already declares GPL v3.

All eleven effects are required. Their functional scope is:

| Effect | Included algorithm/control family | First-release boundary |
| --- | --- | --- |
| Parametric EQ | Seven bands, shapes/slopes, draggable response and spectrum, standard minimum-phase/IIR processing | Advanced linear-phase and HQ/Enhanced modes are deferred; controls are omitted until their kernels and latency behavior are supported. |
| Limiter | Limiting, compression and gate modes, lookahead/timing, gain/ceiling/curves, saturation and level/gain-reduction display | Internal detector only; external sidechain input/routing is deferred. |
| Reverb | Tank/plate-style network, input filtering/M-S, predelay, size/diffusion/modulation, bass/decay/crossover/damping, dry/early/wet and separation, room visual | Same useful control groups as the mockup; bounded tail and prepared room-size changes. |
| Delay | Mono/stereo/ping-pong models, time/sync/pitch behavior, feedback filtering/distortion, modulation/diffusion, dry/wet | Supported stereo path only; sustained feedback obeys the common tail cap. |
| Chorus | Modulated delay, depth/stereo, three LFO speeds/waveforms, crossover/filtering and dry/wet or wet-only | Repeatable seeded initialization and defined seek/loop behavior. |
| Fast Distortion | Reference table/curve waveshaping, preamp, threshold, A/B type, mix, post gain and transfer graph | Preserve reference sound; oversampling/antialiasing changes are separate future choices. |
| Soft Clipper | Exponential soft-clipping transfer, threshold/post gain and transfer graph | Standard reference algorithm. |
| Balance | Reference folding balance matrix and volume | Included despite overlap with envelope gain/track pan; do not substitute the track pan law. |
| Stereo Enhancer | Separation/pan, phase offset, pre/post placement, channel inversion and volume | Standard stereo path; explicit mono promotion/downmix. |
| Stereo Shaper | Four-coefficient stereo matrix, channel delay/all-pass phase and pre/post placement | Main stereo output only; extra difference-output routing is deferred. |
| Transient Processor | Attack/release amounts/shapes, split frequency/balance, drive/gain and processing toggle | Standard stereo detector/processing path. |

### Per-effect delivery contract

Every effect ticket includes a working DSP processor and its own usable egui
editor, not only a registration or placeholder. Use the common native-window and
parameter infrastructure. Publish stable IDs, defaults, ranges, units and exact
entry/reset behavior for every supported control from the pinned reference.

Define deterministic reference cases and numerical tolerances before judging the
candidate: impulses, steps, multitone/stereo audio and supported parameter
extremes as appropriate. Establish same-algorithm agreement against upstream
reference renders, with justified tolerances and documented intentional
differences; a merely similar-sounding replacement is insufficient. Reference
tools/fixtures stay separate from production dependencies. Listen to representative
renders during implementation and record what was measured versus auditioned.

Through public commands verify live/manual-output/offline parity for fixed
settings, state reset/seek/loop policy, bypass/reorder, parameter cancellation,
single-step undo, instance isolation, and callback safety under live changes.
Take and inspect an actual editor-viewport capture against the source layout;
mockup text never overrides real parameter units. GUI-free rendering and generic
headless/MCP discovery must work with the same parameters and algorithm. Final
cross-effect/project/export coverage is owned by tickets 43–44.

## Editing model and signal order

Separate structural edits, envelopes and effects:

| Concern | Operations | Ownership and behavior |
| --- | --- | --- |
| Structural edits | Cut, copy, paste, delete, trim, reverse | Existing Edit menu/toolbar and shortcuts; committing changes source audio revisions. Reverse retains selection-or-whole-document targeting. |
| Document envelope | Overall gain, fade-in, fade-out | Nondestructive state on the audio document, independent of its selection. |
| Effect stack | Ordered, editable built-in effect instances | Nondestructive state on a document or one selected arrangement clip; whole owner scope. |
| Clip envelope | Overall gain, fade-in, fade-out | Independent state on each arrangement clip; never changes the source document or another instance. |
| Bounce | Rendered document audio and its tail | Derived lossless audio consumed by linked clips; never replaces the editable document. |

Document render order:

`source audio after structural edits -> document gain/fades -> document stack -> bounce`

Arrangement render order:

`bounce source interval -> clip gain/fades -> clip stack -> automatic crossfade envelope -> track gain/pan -> master`

Document gain/fades are baked once into the bounce. Clip controls are additional
instance processing. Each boundary has distinct ownership so neither envelopes
nor stacks are applied twice. Applying fades before reverb lets a faded input
produce a natural decay. Track volume remains an output balance control.

Selections continue to govern structural edits, playback loops and range export.
They do not silently retarget a live stack or the top-edge handles. Processing
only a time selection with a live effect is deferred; isolate material in a
document or clip instead. No selection-dependent DSP state changes while playing.

Copy/cut retain source-audio clipboard semantics: they copy the structurally
edited source interval, without baking the live envelope or stack. Explain this
in command/help text. Rendered range export provides processed audio. Paste does
not import the source document's envelope or stack, avoiding double processing.
Length-changing edits preserve envelope durations where possible, clamp them to
the new body duration, and retain the stack. Undo restores all affected state.

## Effect stack interface

- Rename the right panel to Effects. Keep its resize/collapse behavior and show
  the owner's name clearly: active document in waveform mode, exactly one
  selected clip in multitrack mode. No/multiple selected clips show an explanation;
  there is no implicit track or master stack in this change.
- Show compact ordered rows with a reorder grip, enable/bypass, effect name, a
  small Open controls button and a menu for remove/move actions. Provide
  keyboard-accessible move-up/down controls and Add effect. Parameters never
  expand into the panel. Include a whole-stack bypass for quick comparison;
  bypass is persisted and undoable. An open-window indicator is presentation state.
- Offer all eleven effects in the scope table as their tickets complete. Deliver
  Soft Clipper and Reverb first to prove the shared DSP/editor contract, then
  standard EQ and Fast Distortion, then the remaining effects. All eleven are
  required for feature completion; the rollout order does not reduce scope.
- Multiple instances are allowed, with at most 32 entries per owner. Use stable
  WaveBob effect/parameter IDs and explicit validated parameters, rather than
  plugin opaque state. The old Gain, Fade in/out and Reverse tools remain absent
  from Add effect; Balance's own volume is an independent utility parameter.
  Third-party plugin scanning, VST/CLAP hosting and plugin installation are outside
  scope; the desired effects ship with WaveBob.
- Adding, removing, reordering, bypassing or changing parameters affects the
  audible chain without Apply and without restarting the transport. Keep Undo
  and Redo available in the Edit menu/toolbar and their current shortcuts.
- Slider drags preview continuously and commit one history entry on release.
  Escape restores the pre-drag state. Numeric edits commit once on acceptance;
  a no-op creates no history. Changing a panel's target cancels its unfinished
  gesture; a separate effect window remains bound to its original owner.
- Adapt effect defaults, ranges and units from the chosen upstream DSP and its
  parameter definitions; do not pretend Lemon Reverb is the old three-parameter
  reverb. A disabled stack is an explicit identity chain after latency handling.
- Display processed output peak/overload. Retain float headroom; never silently
  normalize or limit the result. The gain handle reads the envelope gain, not the
  output peak. Browser audition remains an independent dry audition path.

## Separate effect control windows

- Open controls creates or focuses a separate modeless native desktop window
  using egui viewports, keyed by the stable owner/effect IDs. It stays bound to
  that instance when the active document or selected clip changes. Its title
  includes effect and document/clip names. Multiple effects may have windows open.
- Closing a control window leaves the effect active. Bypass does not close it.
  Removing its effect/owner closes the window and cancels unfinished gestures.
  Undoing removal restores effect state without unexpectedly reopening windows.
- Recreate each editor's useful layout, graph interactions, knob/fader groupings,
  control labels, numeric entry and meters in egui, matching the source interface
  as closely as practical within WaveBob's shell and DPI/accessibility rules.
  The upstream custom Canvas/platform editor is reference material, not a native
  plugin window to embed. Decorative visuals must not compete with audio work.
- The mockup uses the Reverb room visualization and INPUT/ROOM/DECAY/MIX groups as
  a concrete example. Image values are illustrative: source units/parameter
  definitions govern actual controls. Tempo is a toggle, even though the concept
  image renders that position as a dial. The stack's small window button is the
  important interaction under review.
- UI, automation and preset changes use the same core preview/commit path and
  update every view. Meter/analyzer data flows back through bounded snapshots;
  opening an editor is never necessary for an effect to process audio.
- Space and transport shortcuts remain usable while an editor is focused, except
  when editing text or a modal owns input. Undo/redo uses shared chronological
  history. Native windows show their own contextual help/value readouts.
- Remember window geometry as UI preferences; recover offscreen windows after
  monitor/DPI changes. Window opening/closing/moving is outside audio history.
- Extend GUI automation to open/focus/close and capture each effect viewport's
  actual pixels with owner/effect IDs and state revisions. A main-window capture
  alone cannot include a separate native window. Headless DSP remains usable
  without windows; editor/capture requests return explicit capability errors.

## Waveform and clip envelope gestures

- At zero fade, show inward-pointing triangles at the true left/right top edges.
  Drag the left triangle right to set fade-in duration; drag the right triangle
  left to set fade-out duration. The handle follows its fade endpoint and an
  overlay shows the complete ramp and its source-edge anchor.
- The gain square starts at the body's top center. Vertical dragging changes
  overall gain: down attenuates, up increases. Sensitivity is 0.25 dB per
  logical pixel, with a fine-adjust modifier dividing that by ten. Drag range is
  -60 to +12 dB; exact entry supports the existing -120 to +60 dB validation range
  plus explicit mute (-infinity). Double-click resets gain to 0 dB or a fade to 0.
- Hover and keyboard focus show `Fade in: 125 ms`, `Fade out: 1.250 s`, or
  `Gain: -6.0 dB`. Show a small value label beside the active handle and mirror it
  in the status bar, including exact frame counts for fades. These value labels
  are an intentional exception to the existing prohibition on floating tooltips;
  explanatory help stays in the status bar.
- Clicking a handle opens exact numeric entry; dragging engages only after the
  normal movement threshold. Allow focus, arrow-key adjustment, fine adjustment,
  reset and Escape. Keyboard editing and automation can reach every value.
- All channels share one envelope. Stereo does not acquire separate handles.
  Draw the gain/fade overlay and scale visible source peaks by the envelope;
  display effect output level separately. Rebuilding wet waveform peaks on every
  parameter movement is unnecessary. Label the main display as source plus
  envelope so it does not claim to depict the final wet signal.
- Fade durations are integer source frames, clamped independently to the current
  body length. Zero duration is unity. Otherwise use linear amplitude ramps with
  the existing single-frame unity convention. Intersecting fade-in/out ramps
  multiply, allowing an intentional dip. They do not shorten one another.
- Document handles refer to the full source body, regardless of selection or
  viewport. A clipped-off edge is not a new sample edge: omit an offscreen handle,
  and offer exact values/reset in an envelope inspector/context action. Never
  silently anchor fades at a viewport boundary.
- On arrangement clips, the body is the trimmed source interval. Moving a clip
  carries its envelope; trimming keeps each fade anchored to its corresponding
  current edge, preserves its duration and clamps it if necessary. Gain is
  unaffected. Duplicate copies envelope and stack state into independent owners.
- Handles are visibly small but have at least a 12-logical-pixel hit target.
  Show them on selected/hovered clips to avoid crowding. If handles collide on a
  tiny clip, use the envelope inspector rather than choosing a hidden target.
- Reserve the upper edge for envelope handles and the lower edge/body boundary
  for trim/crossfade controls. Exact handle hit targets take priority over body
  move and selection gestures; hovered trim/crossfade controls retain their own
  designated area. Gain/fade dragging must not move, trim or select the clip.
- During a drag, update audio and overlays continuously. Release commits one
  undoable change; Escape reverts both. Selection and cursor remain unchanged.

## Real-time processing contract

The current transport reads prepared PCM or a prepared mix, and its reverb
processes a whole target offline. Full-file rerendering after every slider
movement cannot satisfy this feature. Implement the reference algorithms behind
a WaveBob streaming processor interface shared by live and offline rendering.
Use the investigation's allocation/reset findings to avoid reproducing those
problems in the new code; algorithm fidelity alone does not establish real-time
safety.

- Allocate delay storage and prepare graph changes off the audio thread. The
  callback performs bounded DSP using prepared buffers, with no allocation,
  blocking locks, file access or buffer destruction.
- Smooth continuous parameters, including gain and wet mix, over approximately
  10 ms. Topology-changing reverb controls, bypass and reorder use an approximately
  20 ms crossfade between prepared paths. Preserve unaffected processor state;
  do not clear a reverb delay line on every slider event.
- Coalesce superseded changes and retire buffers off the callback. Bound the
  simultaneous old/new graphs and reject resource-exceeding requests cleanly.
  Expose requested and audible processing generations so pending changes are
  observable. Keep the previous valid chain audible if preparation fails.
- At 48 kHz with 256-frame blocks, target no more than 50 ms from parameter input
  to the start of audible change after initial preparation. Measure and report
  latency, callback load and dropouts on a named reference machine. Longer graph
  preparation shows pending status; it never freezes the UI or restarts playback.
- Processors declare latency and its changes. Compensate different arrangement
  paths, align dry/bypassed paths for comparison, and remove processing delay from
  offline/bounce timestamps. Lookahead and linear-phase EQ modes must not shift
  clips, drift crossfades or comb-filter parallel paths. Tail duration and latency
  are separate metadata. Prepare latency changes away from the callback.
- Define mono-to-stereo behavior explicitly: upstream wrappers are stereo. Permit
  stereo expansion where an effect requires it and preserve the resulting channel
  layout through bounce/export; never discard one channel silently. Establish
  headless timing context for tempo-synced controls and seeded modulation so
  offline renders do not depend on an open editor or a host transport object.
- Channel default: promote mono to dual mono at the processor boundary,
  retain stereo output for a stereo-generating chain, and require explicit
  downmix for mono export. Keep the editable source's channel layout intact.
- Timing default: use free milliseconds/Hz unless sync is enabled. For
  sync expose a project Effect tempo, initially a visible 120 BPM, shared by
  waveform, arrangement and offline/headless/MCP rendering. It does not imply a
  beat grid or detected tempo. Changing BPM invalidates affected render identities.
- Audit effect-specific concerns during each effect's implementation: feedback and
  tail limits, external sidechain requirements, linear-phase/HQ CPU and memory,
  analyzer traffic, distortion aliasing, denormals/nonfinite output and any
  history-clearing/allocation inside parameter changes. Do not offer a control
  that the built-in adapter cannot faithfully support.
- Pause preserves effect state for resume. Stop/rewind and explicit seeks reset
  state, then prepare bounded preroll from preceding source audio off the callback
  so a seek does not inherit unrelated reverb. Publish readiness before resuming.
  Loop wraps preserve streaming state, allowing natural reverb across iterations.
- Offline renders start from reset state and render chronological audio. A range
  export processes preceding source as context before extracting the range, then
  includes the range's declared tail. Repeated loop playback has accumulated
  state and is not expected to equal a single reset render.
- For fixed settings and reset initial state, manual playback output, offline
  sample export and bounce use the same DSP and agree within declared numerical
  tolerance after rate conversion. Live transition smoothing is transient state,
  not something permanently baked into the saved stack.

## Linked clips and switching modes

A linked clip retains a document identity and source interval as well as its
last successfully rendered bounce. A pinned clip retains an independent source
snapshot. This extends the current rule that all clips are pinned.

- Newly inserted documents create linked clips by default. The UI identifies
  linked clips and offers Make independent, preserving the last rendered sound.
  Closing a document's waveform tab does not delete its linked project source;
  reopening it for editing restores its source, envelope and stack.
- Pre-alpha project compatibility is out of scope. Store explicit document links
  in newly saved projects rather than inferring ownership from a filename.
- Track document render identity using source audio revision, envelope, ordered
  stack parameters/bypass, effect tempo, tail policy and DSP version. Navigation,
  selection and panel changes do not invalidate it. Reuse a successful bounce for the
  same identity; bypassed/empty processing may reuse source audio.
- Switching waveform to multitrack pauses playback as today, commits any finished
  control edit, and prepares one shared document bounce for all its linked clips.
  If stale, show progress and cancellation. The multitrack view may appear while
  preparing, but arrangement play/export must wait or fail explicitly as pending;
  they may not silently play the previous bounce as if it were current.
- Publish the bounce and all affected clip references atomically, only if the
  captured document state and affected clip identities remain current. Undo,
  further edits, relinking or project load invalidate stale jobs. Rapid switches
  coalesce work. Failure/cancellation keeps the previous arrangement playable
  only after the user explicitly elects to keep its prior source version, and
  leaves a visible pending/error state. Retry never discards editable work.
- Inserting a processed document while already in multitrack, opening a project,
  and arrangement export/headless rendering require the same freshness barrier.
  The core owns this behavior; a GUI mode-switch handler is not the sole trigger.
- Refresh all linked instances, retaining clip IDs, track positions, trims,
  instance envelopes/stacks, selection and insertion order. Ordinary parameter
  edits change no clip-body geometry. A refresh is derived work and creates no
  extra undo step: undoing a document change restores its render identity and
  makes the corresponding bounce current or rebuildable.
- For structural edits, retain source-frame trim offsets and clamp to the new
  source duration. Full-document clips continue to span the full body. Validate
  all new lengths and the two-body overlap rule before publishing. If a range
  becomes empty or a new body creates an invalid overlap, stop refresh atomically
  and identify the clips requiring trim, move or Make independent. Never delete
  clips, ripple later positions or repair invalid geometry silently.
- Treat bounce audio as managed lossless float assets/cache. Do not create another
  visible document, overwrite the original WAV/MP3, or encode MP3 during handoff.
  Retain reusable bounces within a bounded cache; protect anything referenced by
  a clip, history entry, render job or saved project from eviction.

## Tails, clip boundaries and crossfades

- A body interval and audible extent are distinct. Preserve a separate rendered
  tail after the source body; show it as a shaded extension with its duration.
  Fade handles and trim edges stay anchored to the body, not the tail endpoint.
  Whole-sample save/export includes the audible tail without adding silence to
  the editable source body. Empty/bypassed stacks have zero tail.
- For trimmed linked clips, render document processing with the preceding source
  context, feed silence at that instance's source end and drain its tail. Do not
  read later, excluded source material as a tail. Full-body instances share the
  common bounce; distinct trim intervals can share derived interval renders keyed
  by document render identity and source interval. Instance processing follows.
- Stop tail rendering only after output and retained effect state are below
  -100 dBFS for 100 ms. Use a deterministic maximum of 60 seconds for the combined
  chain tail. If the cap is reached, taper its last 10 ms and report truncation
  visibly and in render metadata. Silent/wet-zero paths do not acquire a tail.
  Live non-looped playback uses the same termination rule and remains playing
  through its tail. Explicit pause/stop remains immediate with a short ramp.
- A document fade-out shapes the signal feeding its stack, so reverb can continue
  beyond that fade. A clip fade-out shapes its bounced input before its own stack;
  it can suppress a document tail. Make that consequence visible in help.
- Later clips keep their timeline starts. Tail overlap alone does not create or
  resize an automatic crossfade. Derive crossfades from clip bodies as today;
  apply their envelopes to processed output, holding the boundary value into the
  tail. An outgoing equal-power crossfade can therefore suppress its outgoing
  tail; that is an intentional mix envelope, distinct from bounce truncation.
- Keep the current maximum of two overlapping clip bodies per track. Multiple
  tail voices may overlap these bodies; account for them separately with bounded
  processing resources and explicit capacity errors. Tail generation must not
  fail solely because it introduces a third audible voice, nor silently cut it.
- Arrangement playback/export ends after the latest contributing audible tail.
  Noncontributing muted/unsoloed tracks do not extend it. Arrangement loop bounds
  follow selected/body intervals, with effect state continuing across wraps.
  The UI and queries expose body duration and audible duration separately.

## Persistence, history and automation

- Replace/version the pre-alpha project schema as needed. Persist editable source
  audio, document envelope and stack, stable effect IDs, clip linkage, source
  ranges, clip envelope/stack, effect tempo,
  render identity, tail policy and managed bounce assets. A project must reopen
  without original source files or an external cache, preserving both editability
  and the last rendered arrangement sound. Future/unknown effects fail explicitly.
- No schema-1 migration, frozen legacy processing stage, old preset compatibility
  or old command compatibility layer is required. Reject unsupported project
  schemas clearly and update fixtures/command clients to the new model.
- Envelope/stack commits dirty the document's rendered sample state and project,
  even if the source PCM revision is unchanged. Record saved render identities
  separately from saved source revisions. Project save preserves stack editability;
  standalone sample Save/export renders its sound and retains the in-session dry
  source/stack. A later reimport is flattened audio, not a recoverable stack.
- MP3 import and WAV/MP3 sample/arrangement export remain required in GUI,
  headless and MCP. Existing MP3 encoding/overwrite confirmations still apply.
- Project save and export capture coherent committed state. In-flight gestures
  must commit or cancel first. Saving/exporting a stale captured state cannot mark
  newer edits saved. Undo/redo restores source, envelope, stack and linkage as one
  consistent state; cache generation and view switching do not enter edit history.
- Extend the shared command catalogue for stack query/add/remove/reorder/bypass/
  parameters, document/clip envelope get/set, linked/pinned source policy and
  bounce freshness/prepare/wait. Return owner/effect IDs, expected edit revisions,
  body/tail bounds, requested/applied generation and job failures. Precise API
  spellings can follow existing conventions during implementation.
- Expose begin/update/commit/cancel preview transactions through the same core
  seam used by GUI drags. Serialize conflicts; stale owners and abandoned previews
  cannot overwrite committed work. Queries distinguish committed and preview state.
  Device-free offline rendering uses committed state unless explicitly requested.
- Replace obsolete destructive effect commands where appropriate and update the
  shared catalogue and adapter tests together. Structural reverse remains a
  command-driven edit; stack operations have explicit nondestructive semantics.

## Review points beyond the requested controls

- **Selection scope:** the stack and edge controls affect their whole owner.
  Selection-only fades/effects are no longer implicit GUI operations.
- **Processing order:** envelope before effects preserves document reverb decay;
  clip envelopes can further shape or suppress the bounced document tail.
- **Reference provenance:** record the algorithm/source origin and applicable
  notices while writing focused WaveBob implementations; see the reference policy.
- **Structural changes:** waveform length changes can invalidate existing trims
  or overlap limits. Report a blocked refresh with affected clip IDs; no ripple.
- **Saving:** projects retain editable stacks; exported samples contain rendered
  sound. Reimporting an exported WAV/MP3 cannot recover its stack.
- **Tail boundaries:** audible tails extend export/playback duration and may add
  voices beyond the two-body overlap limit. Crossfade envelopes still shape them;
  the deterministic 60-second cap is a resource bound reported if reached.

## Published delivery and acceptance

[Tickets 27–44](README.md#live-effects-and-envelope-work) are published as
`ready-for-agent` with explicit blocking edges. The index orders foundations
(27–29), all eleven effects (30–40), envelope gestures (41), linked bounces/mixing
(42), project and export integration (43), and workflow verification (44).
Read actual blocker files before starting; publication does not mark work begun.

Use public commands and manually clocked audio for deterministic signal checks,
with focused GUI gesture tests and actual screenshots for visual behavior. Match
the WaveBob visual language while checking handle placement against the supplied
reference. Include Windows high-DPI/narrow layouts and the established platform
checks. No new test seam or screenshot-only audio validation is required.

Required end-to-end scenario: import a sound, insert two linked instances, return
to waveform, set gain/fades and reorder two reverbs while looping, undo/redo,
return to multitrack, wait for one shared bounce, hear both instances updated,
adjust one clip independently, preserve a tail over a later clip without shifting
it, save/reopen the project, then export WAV and MP3 through the shared seam.

## Relationship to the original delivery

The canonical spec incorporates this detailed contract. It replaces explicit
Apply processing with live stacks, extends pinned-only ownership with linked
clips, and replaces truncated-only reverb with bounded preserved tails. The
callback executes bounded prepared DSP; preparation/allocation stay outside it.

Historical tickets 10, 12, 15, 16, 17, 19 and 20 describe delivered baseline
behavior and remain completed with their acceptance history. New tickets should
reference them and this change rather than rewriting their historical results.
Update command/audio/project/crossfade documentation with implementation, and
record measured latency/resource limits before declaring the work complete.
