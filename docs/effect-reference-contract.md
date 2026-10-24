# Effect algorithm and reference verification contract

Reference: ItsAlphaNeon/Lemon-Studio commit
`0af860dfd31c7ee9cbf8af6747617e031a1a7742`.
The [source investigation](../.scratch/wavebob/lemon-effects-review.md) records
the reviewed algorithm/control families and agreed mode limitations.

Production processors and egui editors are WaveBob-owned modules. No production
dependency imports the upstream engine, plugin framework, host, Canvas or UI crates.
This is reference-based development, not a clean-room independence claim. Files
adapted from inspected upstream source must retain `Copyright (C) 2026 ItsAlphaNeon`
and `SPDX-License-Identifier: GPL-3.0-or-later`, identify the upstream path and pinned
commit, and describe WaveBob modifications. The root GPL license applies; packaging
must include these source files and notices.

## Source contracts

All paths below are relative to the pinned repository. Parameter bounds/defaults
and raw-to-display mappings come from DSP settings plus plugin parameter/view code,
not illustrative screenshot labels.

| Effect | Algorithm under `crates/os-engine/src/effects/` | Controls under `plugins/` |
| --- | --- | --- |
| Soft Clipper | `soft_clipper.rs` | `lemon-small-fx/src/soft_clipper.rs` |
| Reverb | `reverb.rs` | `lemon-reverb/src/lib.rs`, `params.rs`, `view.rs` |
| Parametric EQ | `eq/` | `lemon-parametric-eq-2/src/` |
| Fast Distortion | `fast_dist.rs` | `lemon-small-fx/src/fast_distortion.rs` |
| Delay | `delay.rs` | `lemon-delay/src/` |
| Chorus | `chorus.rs` | `lemon-small-fx/src/chorus.rs` |
| Limiter | `limiter.rs` | `lemon-limiter/src/` |
| Balance | `balance.rs` | `lemon-small-fx/src/balance.rs` |
| Stereo Enhancer | `stereo_enhancer.rs` | `lemon-small-fx/src/stereo_enhancer.rs` |
| Stereo Shaper | `stereo_shaper.rs` | `lemon-small-fx/src/stereo_shaper.rs` |
| Transient Processor | `transient_processor.rs` | `lemon-small-fx/src/transient_processor.rs` |

## Reference renders and provenance

Before judging each candidate, record supported controls, units/ranges/defaults,
channel policy, algorithm helpers, latency, tail reporting and deliberate differences.
Use an isolated reference harness outside production dependencies, pinned to the
commit above. Its manifest records compiler, source-file SHA-256 hashes, input
fixture hash, settings, sample rate, reset/seed/tempo, block sizes and output hashes.
Retain reference PCM or reproducible generation instructions with verification.

Cases include impulses, positive/negative steps and amplitude sweeps, silence,
independent stereo tones, multitones and parameter extremes. Test 44.1/48/96 kHz,
mono promotion, deterministic reset and multiple block sizes. State numerical
tolerance before comparison: memoryless output targets 2e-7 absolute error;
stateful effects require effect-specific peak/RMS tolerances and phase/latency
alignment. Do not loosen a tolerance simply to pass an unexplained mismatch.

Compare reset fixed-setting reference output against WaveBob offline output,
then compare the same public-command render to manually clocked playback.
Test smoothing/graph transitions separately: the reference's abrupt parameter
updates do not establish WaveBob's live safety. Verify envelopes/instance isolation,
single-step history, cancel, bypass/order, tail cap/activity, latency compensation,
callback allocation and state retention independently. Record measured versus
auditioned evidence. Take and inspect each actual native editor viewport capture;
a main-window screenshot is insufficient for a separate window.

No effect DSP is enabled by the state foundation. Passing its state tests does not
establish algorithm fidelity, real-time safety or native editor acceptance.

## Native reference arithmetic

The Delay verification adds `wavebob-effect-reference` as a dev-only local crate
under `tools/effect-reference/reference-lib`. It compiles unchanged pinned source
and a minimal test trait, without an engine/plugin SDK. Native comparison avoids
mistaking platform sinf ulps for a different modulated fractional-tap algorithm.
The source hash is asserted and rustfmt skips the pinned module. Production
builds never depend on this crate. Source-complete kits include it for test
reproduction; capture generators and per-effect manifests remain separate from
application dependencies.
