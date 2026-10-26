// Copyright (C) 2026 ItsAlphaNeon (reference parameter laws)
// SPDX-License-Identifier: GPL-3.0-or-later
use super::{INVERT, INVERT_LEFT, INVERT_RIGHT, PAN, PARAMS, SEPARATION, VOLUME};
pub fn volume_gain(volume: i32) -> f32 {
    level_curve(volume as f32 * 0.00390625)
}

/// The gains' targets `[LL, RR, RL, LR]` (spec "The gains"; singles, in the order written).
pub fn gains(v: &[i32; PARAMS]) -> [f32; 4] {
    let p = v[PAN] as f32 * 0.0078125;
    let s = v[SEPARATION] as f32 * 0.0078125;
    let g = volume_gain(v[VOLUME]) * (1.0 - s * 0.5);
    let (mut ll, mut rr) = (g, g);
    let (mut rl, mut lr) = (g * s, g * s);
    if p < 0.0 {
        rr *= p + 1.0;
        lr *= p + 1.0;
    } else {
        ll *= 1.0 - p;
        rl *= 1.0 - p;
    }
    // An inversion other than 1 or 2 counts as none.
    match v[INVERT] {
        INVERT_LEFT => (ll, rl) = (-ll, -rl),
        INVERT_RIGHT => (rr, lr) = (-rr, -lr),
        _ => {}
    }
    [ll, rr, rl, lr]
}

/// The phase offset's delay in samples for the knob `k` and the rate factor `c = single(rate / 100000)`:
/// `(1001^(|k| / 512) - 1) / 2` ms, rounded (spec "From a setting to its value").
pub fn delay_samples(k: i32, c: f32) -> usize {
    let x = (f64::from(k.unsigned_abs()) / 512.0) as f32;
    let e = ((f64::from(x) * 1001f64.ln()).exp() - 1.0) as f32;
    let t = e * 50.0;
    (t * c).round_ties_even().max(0.0) as usize
}

/// `single(rate / 100000)`.
pub fn rate_factor(sample_rate: u32) -> f32 {
    (f64::from(sample_rate) / 100000.0) as f32
}

fn level_curve(x: f32) -> f32 {
    if x == 1.0 {
        return 1.0;
    }
    let p = x * (11f64.ln() as f32);
    ((f64::from(p).exp() - 1.0) * f64::from(0.1f32)) as f32
}
