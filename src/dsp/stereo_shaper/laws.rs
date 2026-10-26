// Copyright (C) 2026 ItsAlphaNeon (reference matrix/delay/all-pass laws)
// SPDX-License-Identifier: GPL-3.0-or-later
use super::{LEFT, LEFT_INTO_RIGHT, PARAMS, RIGHT, RIGHT_INTO_LEFT};
pub fn level_gain(v: i32) -> f32 {
    let x = (f64::from(v) / 12800.0) as f32;
    if x == 0.0 {
        return 0.0;
    }
    let g = (((f64::from(x.abs() * (3f64.ln() as f32))).exp() - 1.0) * 0.5) as f32;
    if x < 0.0 { -g } else { g }
}

/// The gains' targets `[L, R, RL, LR]`.
pub fn gains(v: &[i32; PARAMS]) -> [f32; 4] {
    [LEFT, RIGHT, RIGHT_INTO_LEFT, LEFT_INTO_RIGHT].map(|k| level_gain(v[k]))
}

/// The delay in samples for setting `k` and the rate factor `c = single(rate / 100000)`: `(1001^(|k| / 4096) - 1) / 2`
/// ms, rounded (spec "Delay").
pub fn delay_samples(k: i32, c: f32) -> usize {
    let x = (f64::from(k.unsigned_abs()) / 4096.0) as f32;
    let e = ((f64::from(x) * 1001f64.ln()).exp() - 1.0) as f32;
    let t = e * 50.0;
    (t * c).round_ties_even().max(0.0) as usize
}

/// The channel a delay or phase setting acts on: the right for a positive one, the left for a negative one.
pub(super) fn side(k: i32) -> usize {
    usize::from(k >= 0)
}

/// The all-pass's frequency for setting `k` (not 0) before the rate's limit: `single(single(exp(u * ln 5001) - 1) *
/// 4.398 + 10)` with `u = single(1 - |k| / 4096)`, 22 kHz just off the centre to 10 Hz at either end.
pub fn phase_hz(k: i32) -> f32 {
    let u = (1.0 - f64::from(k.unsigned_abs()) / 4096.0) as f32;
    let e = ((f64::from(u) * 5001f64.ln()).exp() - 1.0) as f32;
    (f64::from(e) * 4.398 + 10.0) as f32
}

/// The all-pass's frequency and coefficients (spec "Phase").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllPass {
    pub hz: f32,
    pub a1: f64,
    pub a2: f64,
}

impl AllPass {
    /// `None` for a setting of 0 (off).
    pub fn new(k: i32, rate: u32) -> Option<AllPass> {
        if k == 0 {
            return None;
        }
        let hz = phase_hz(k).min((f64::from(rate) * 0.4999) as f32);
        let w = f64::from(hz) * ((2.0 * std::f64::consts::PI) / f64::from(rate));
        let k = 1.0 / (w.sin() + 1.0);
        Some(AllPass {
            hz,
            a1: -2.0 * k * w.cos(),
            a2: k * (1.0 - w.sin()),
        })
    }
}
