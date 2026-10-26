# Gain and fade controls

Document controls act on the full source body. Clip controls act on its trimmed
source interval. Every channel shares one envelope, independently of cursor,
selection and waveform zoom. The display is **Source + envelope**; the Effects
panel's processed-output meter measures transport output separately.

The small triangles mark fade endpoints and the square marks envelope gain.
At unity they start at the upper body edge. Attenuation moves the handle level
down at four logical pixels per dB, bounded by the body display. Peak scaling
uses linear amplitude; overlay height is a control guide rather than a wet-waveform
measurement. Fade ramps remain anchored at the true source edges. Intersecting
fades multiply; their individual guides and combined curve remain visible.

Drag gain vertically at 0.25 dB per logical pixel; Shift divides this by ten.
Dragging spans -60 to +12 dB. Fade drags set integer source-frame durations,
independently bounded by body length. The core preview changes audible output
continuously, release commits one edit, and Escape restores committed audio and
state. Source PCM, cursor and selection remain unchanged. A no-op adds no history.

Click a handle for exact entry (after the double-click interval); double-click
resets that control. Focused handles accept arrows: gain steps 0.25 dB (Shift:
0.025), fades step ten frames (Shift: one). Delete/Backspace resets and Enter opens
exact entry. **Gain and fades…** in the Effects panel or body context menu remains
available for offscreen, tiny or colliding handles. Exact entry accepts -120 to
+60 dB, explicit mute and fade durations from zero through the body length.

Hit targets are at least 12 logical pixels. Offscreen handles are omitted rather
than relocated to the viewport boundary. Colliding controls use exact entry.
Selected clip controls take precedence over hovered overlapping clips; ambiguous
controls on equally selected clips are omitted. The lower trim/crossfade areas
retain their own gestures. An active envelope drag keeps ownership when it crosses
another control. Losing window focus, closing/changing its target, a source edit or history
navigation cancels unfinished GUI gestures.

Moving a clip carries its envelope. Trimming retains each edge's fade duration,
clamped to the new body length. **Duplicate clip** and `clips.duplicate` share source
audio while copying processing into fresh clip/processing/effect IDs. Their later
edits and playback states are independent.

Automation uses the same `processing.edit` / `processing.preview.*` commands with
an explicit owner and expected processing revision; `set_envelope` carries
`gain_db` (null means mute), `fade_in_frames` and `fade_out_frames`. There is no
separate GUI envelope or alternate audio path.
