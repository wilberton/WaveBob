# Automatic crossfades

Two clips overlapping on one track derive an equal-power transition without
rewriting either pinned source or its instance processing. `arrangement.crossfades`
returns the arrangement revision and regions with track/clip IDs, half-open project
frame bounds, kind and optional contained-clip identity. The GUI draws those same
envelopes over the intersection; moving, trimming or undo/redo recomputes them.

For an ordinary staggered intersection of L frames, frame i uses t=i/(L-1), with
earlier gain cos(t·π/2) and later gain sin(t·π/2). First and last overlap frames use
the corresponding endpoint gains. A single-frame intersection shares both voices
at 1/√2, avoiding an undefined denominator.

For containment, inner gain is sin(t·π) and outer gain is |cos(t·π)|. This fades the
inner voice in/out while dipping and restoring the containing voice. Shared-start
or shared-end pairs of different lengths use this containment rule. Fully
coincident clips use the ordinary transition rule, ordered by stable insertion
order. The same insertion order identifies first/second for all shared starts;
the contained-clip ID determines their envelope roles independently. For contained
intervals of one or two frames, both voices share at 1/√2 because a discrete
start/peak/end envelope requires at least three frames.

At most two clip bodies may be simultaneous on a track. Third-overlap insert, move or
trim fails atomically with an explanation; use another track or move the clip.
End boundaries are exclusive, so adjacent clips and adjacent overlap regions are
valid. Source-rate `clips.samples` remains unchanged by geometry; project-rate
`mix.samples`, live playback and arrangement export consume the derived envelopes.

The multitrack GUI prepares current linked bounces after edits. Explicit offline
preparation, play and export use the same freshness barrier. A seek made while resources
are preparing is retained until that graph is adopted.

Audible tails have separate dashed extents and do not participate in body placement
or overlap derivation. The final body-crossfade gain is held through a tail; an
exact-zero boundary suppresses its voice/end. A separate64-concurrent-tail budget
permits tails beyond the two-body rule. Current non-muted/soloed tails extend the
non-loop end while loop bounds remain body bounds. Tail caps are reported. See
[shared bounce evidence](verification/linked-bounces/README.md).
