# WaveBob eframe 0.37.0 integration patch

This directory contains the registry eframe 0.37.0 source, with its original
Cargo metadata, upstream provenance and MIT OR Apache-2.0 license declaration.
The original registry package checksum is
`9c5642501a65ca657d05f00c643a5c14ff2346cb3818032bfe23d58e1f8e5f84`.
Its `.cargo_vcs_info.json` identifies upstream commit
`3c6eb8261c3293ad9c7ea1ed58487e038860ebef`, directory `crates/eframe`.
The license files are from that exact commit in emilk/egui. The only changed
upstream source file is documented by `WaveBob-patch.diff`.

One local change is in src/native/glow_integration.rs: an explicit Screenshot or
RequestPaintWhileHidden action may run UI/paint even when the event-loop mode is
LogicOnly. Win32 hidden windows can withhold native redraw events; eframe's
overdue-redraw tick otherwise ignores the very action requesting a hidden paint.
Ordinary logic-only ticks still do no UI/painting, and the action is consumed by
the existing Glow renderer. No other rendering backend or quality path is added.

WaveBob pins this crate using Cargo's patch table. Reassess/remove this patch when
upgrading eframe to a version that fixes this integration behavior upstream.
Native regression: tests/gui_attachment.rs,
native_bloom_pilots_remain_independent_while_alternating_and_reopening, with both
child HWNDs hidden, normal/150% scale, unchanged five-second capture deadlines,
isolated pixels, stable metadata and close/reopen behavior.
