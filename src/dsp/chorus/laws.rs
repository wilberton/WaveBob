// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// WaveBob laws following Lemon-Studio 0af860dfd31c7ee9cbf8af6747617e031a1a7742 effects/chorus.rs.

//! Exact knob/table laws with fallible one-time off-callback table preparation.
//!
//! The input, at 0.7, is split by a one-pole crossover; one band goes into a short delay line per side and is read
//! back at three places that three slow LFOs move; the three reads, at 0.6, are added to the input. There is no
//! feedback. The LFOs start where a generator puts them each time the effect starts (FL seeds it from the clock;
//! Lemon Studio from a fixed seed, so its renders repeat).
//!
// spec: plugins/fruity-chorus.md, plugins/lemon-chorus.md

#![allow(clippy::approx_constant)] // Source constants retain reference arithmetic.
use std::sync::OnceLock;

use crate::{CommandError, ErrorCode};

/// The settings, in the saved state's order (after the version field).
pub const PARAMS: usize = 12;
pub const DELAY: usize = 0;
pub const DEPTH: usize = 1;
pub const STEREO: usize = 2;
/// LFO 1, 2 and 3: frequency and wave.
pub const SPEED: [usize; 3] = [3, 4, 5];
pub const WAVE: [usize; 3] = [6, 7, 8];
pub const CROSS_TYPE: usize = 9;
pub const CROSS_CUTOFF: usize = 10;
pub const WET_ONLY: usize = 11;

/// The knobs' ranges (spec "Settings"). A stored value outside its range acts as the end it passes (the knob's
/// position is kept within 0..1).
pub const RANGES: [(i32, i32); PARAMS] = [
    (0, 1024),
    (0, 5000),
    (0, 1024),
    (0, 5000),
    (0, 5000),
    (0, 5000),
    (0, 1024),
    (0, 1024),
    (0, 1024),
    (0, 100),
    (0, 5000),
    (0, 1024),
];

/// What a new Fruity Chorus holds (spec "Settings", the Default column). FL's "Default" preset differs in LFO 1's wave
/// only (0 instead of 23: the same shape).
pub const DEFAULTS: [i32; PARAMS] = [512, 2250, 341, 1500, 2500, 3500, 23, 0, 0, 0, 2500, 0];

/// The delay line per side, and the LFO tables' length.
pub const LINE: usize = 4096;

/// Last supported quantized LFO shape.
pub const SINES_CUBED: usize = 3;

// ---------------------------------------------------------------------------------------------------------------
// From a knob to its working value (spec "Settings")

/// Knob `k`'s position: `single(value / max)`, the quotient in doubles rounded once, kept within 0..1.
pub fn position(k: usize, v: i32) -> f32 {
    ((f64::from(v) / f64::from(RANGES[k].1)) as f32).clamp(0.0, 1.0)
}

/// Trunc to a 32-bit unsigned integer.
fn trunc(x: f64) -> u32 {
    x as u32
}

/// The delay in 1/4096 of a sample: `Trunc((x * 29.95 + 0.05) * rate * 4.096)` in doubles (0.05 to 30 ms).
pub fn delay(x: f32, rate: u32) -> u32 {
    trunc((f64::from(x) * 29.95 + 0.05) * f64::from(rate) * 4.096)
}

/// The LFOs' swing: `Trunc(s * 4.096 / 64)` with `s = (x * 5) * rate`, all in singles (0 to 5 ms).
pub fn depth(x: f32, rate: u32) -> u32 {
    let s = (x * 5.0) * rate as f32;
    trunc(f64::from((s * 4.096) / 64.0))
}

/// The table entries by which the right side's LFOs run ahead: `Trunc(single(x * 4096) * 0.5) - 1`, wrapping (0 gives
/// one entry behind; 2048 is half a cycle).
pub fn stereo(x: f32) -> u32 {
    trunc(f64::from((x * 4096.0) * 0.5)).wrapping_sub(1)
}

/// An LFO's step per sample, where 2^24 is one cycle: `Trunc(((x * x) * 83886080) / rate)` in singles (`5 x^2` Hz).
pub fn step(x: f32, rate: u32) -> u32 {
    trunc(f64::from(((x * x) * 83_886_080.0) / rate as f32))
}

/// An LFO's shape: `Trunc(single(x * 3))`, 0..3 (3 only at the end of the knob).
pub fn shape(x: f32) -> usize {
    (((f64::from(x) * 3.0) as f32) as usize).min(SINES_CUBED)
}

/// The cross cutoff's frequency: a MIDI note number `127 x` as a frequency, `single(exp((127 x - 69) / 12 * ln 2) *
/// 440)` (8.2 Hz to 12.5 kHz).
pub fn cutoff_hz(x: f32) -> f32 {
    ((((127.0 * f64::from(x) - 69.0) / 12.0) * std::f64::consts::LN_2).exp() * 440.0) as f32
}

/// The crossover's pole and gain for the cutoff knob at `x` (spec "Cross cutoff"; the frequency at most `rate / 4.1`).
pub fn crossover(x: f32, rate: u32) -> (f32, f32) {
    let r = f64::from(rate);
    let w = f64::from(cutoff_hz(x)).min(r / 4.1) / r * 6.283184;
    let pole = ((1.0 - w.sin()) / w.cos()) as f32;
    let gain = (f64::from((1.0 - f64::from(pole)) as f32) * 0.5) as f32;
    (pole, gain)
}

/// "Cross type" and "wet only" are switches: on above a quarter of the knob.
pub fn switch(x: f32) -> bool {
    x > 0.25
}

/// The working values of a set of settings at a sample rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Working {
    pub delay: u32,
    pub depth: u32,
    pub stereo: u32,
    pub step: [u32; 3],
    pub shape: [usize; 3],
    /// The low band is chorused instead of the high band.
    pub low_band: bool,
    pub pole: f32,
    pub gain: f32,
    pub wet_only: bool,
}

impl Working {
    pub fn new(v: &[i32; PARAMS], rate: u32) -> Working {
        let x = |k: usize| position(k, v[k]);
        let (pole, gain) = crossover(x(CROSS_CUTOFF), rate);
        Working {
            delay: delay(x(DELAY), rate),
            depth: depth(x(DEPTH), rate),
            stereo: stereo(x(STEREO)),
            step: SPEED.map(|k| step(x(k), rate)),
            shape: WAVE.map(|k| shape(x(k))),
            low_band: switch(x(CROSS_TYPE)),
            pole,
            gain,
            wet_only: switch(x(WET_ONLY)),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// LFO tables and the generator (spec "LFO tables", "State and timing")

/// The four LFO tables (arc, arc cubed, sines, sines cubed), entries 0..32766 with 16383 the middle.
pub(super) fn tables() -> Result<&'static [u32], CommandError> {
    static TABLES: OnceLock<Result<Vec<u32>, ()>> = OnceLock::new();
    let prepared = TABLES.get_or_init(|| {
        let mut t = Vec::new();
        t.try_reserve_exact(4 * LINE).map_err(|_| ())?;
        t.resize(4 * LINE, 0);
        let third = 1.0 / 3.0;
        let norm = 2.0 * 2f64.sqrt() + 1.0;
        for i in 0..LINE {
            let x = (i as f64 / 2048.0) as f32;
            let arc = if x < 1.0 {
                x * 4.0 * (1.0 - x)
            } else {
                (x - 1.0) * -4.0 * (2.0 - x)
            };
            let d = f64::from(x);
            let sines = (((3.1415 * d).sin() + ((9.4245 * d).sin() + (6.283 * d).sin()) * third)
                * 3.0
                / norm) as f32;
            for (k, v) in [arc, arc * arc * arc, sines, sines * sines * sines]
                .into_iter()
                .enumerate()
            {
                t[k * LINE + i] = trunc(f64::from((v + 1.0) * 16383.0));
            }
        }
        Ok(t)
    });
    prepared.as_deref().map_err(|_| {
        CommandError::new(ErrorCode::ResourceLimit, "Cannot prepare Chorus LFO tables")
    })
}
