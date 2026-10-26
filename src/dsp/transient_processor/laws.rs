// Copyright (C) 2026 ItsAlphaNeon (reference numeric/filter/detector laws)
// SPDX-License-Identifier: GPL-3.0-or-later
use super::{ATTACK_SHAPE, RELEASE_SHAPE};
/// A denormal counts as zero (keeping its sign).
#[inline]
pub(super) fn z(x: f32) -> f32 {
    if x != 0.0 && x.abs() < f32::MIN_POSITIVE {
        0.0f32.copysign(x)
    } else {
        x
    }
}

#[inline]
pub(super) fn add(a: f32, b: f32) -> f32 {
    z(a + b)
}

#[inline]
pub(super) fn sub(a: f32, b: f32) -> f32 {
    z(a - b)
}

#[inline]
pub(super) fn mul(a: f32, b: f32) -> f32 {
    z(a * b)
}

#[inline]
pub(super) fn div(a: f32, b: f32) -> f32 {
    z(a / b)
}

/// A single power, correctly rounded; measured reference values differ at one near-tie.
pub(super) fn powf(b: f32, e: f32) -> f32 {
    f64::from(b).powf(f64::from(e)) as f32
}

/// A single tangent, correctly rounded.
pub(super) fn tanf(x: f32) -> f32 {
    f64::from(x).tan() as f32
}

pub(super) const C_04: f32 = f32::from_bits(0x3ECC_CCCD);
pub(super) const C_06: f32 = f32::from_bits(0x3F19_999A);
pub(super) const C_01: f32 = f32::from_bits(0x3DCC_CCCD);
pub(super) const C_DRIVE_EXP: f32 = f32::from_bits(0x3F8E_38E3);
pub(super) const C_GAIN_EXP: f32 = f32::from_bits(0x3F66_6667);
pub(super) const C_SPLIT_SQ: f32 = f32::from_bits(0x4543_5E28);
pub(super) const C_SPLIT_LIN: f32 = f32::from_bits(0x417B_53D2);
pub(super) const C_1E4: f32 = f32::from_bits(0x38D1_B717);
pub(super) const C_1E6: f32 = f32::from_bits(0x3586_37BD);
pub(super) const C_A1: f32 = f32::from_bits(0x3F75_C290);
pub(super) const C_A2: f32 = f32::from_bits(0x3B44_9BA6);
pub(super) const C_A_MAX: f32 = f32::from_bits(0x40D3_3333);
pub(super) const C_B1: f32 = f32::from_bits(0x3F57_0A3D);
pub(super) const C_B2: f32 = f32::from_bits(0x3A1D_4952);
/// The drive's copy when the plugin is created.
pub(super) const DRIVE_AT_CREATION: f32 = 0.01;
/// The silence count after a block with sound: `Round(rate * 1.5) + 88`; and while standing still.
pub(super) const STILL: i64 = -1_073_741_823;

// ---------------------------------------------------------------------------------------------------------------
// From a setting to its value (spec "From a setting to its value")

/// Setting `k`'s position in 0..1: `single(clamp(value * (1 / range), 0, 1))` in doubles (range 160, or 2 for the
/// shapes).
pub fn position(k: usize, value: i32) -> f32 {
    let range = if k == ATTACK_SHAPE || k == RELEASE_SHAPE {
        2.0
    } else {
        160.0
    };
    (f64::from(value) * (1.0 / range)).clamp(0.0, 1.0) as f32
}

/// `s = t * |t|` with `t = ((x - 0.5) + x) - 0.5`, -1..1.
pub(super) fn signed_square(x: f32) -> f32 {
    let t = sub(add(sub(x, 0.5), x), 0.5);
    let s = mul(t, t);
    if t < 0.0 { -s } else { s }
}

/// The attack's weight, -0.6..0.4.
pub fn attack(x: f32) -> f32 {
    let s = signed_square(x);
    if s >= 0.0 { mul(C_04, s) } else { mul(C_06, s) }
}

/// The release's weight, -1..0.6.
pub fn release(x: f32) -> f32 {
    let s = signed_square(x);
    if s >= 0.0 { mul(s, C_06) } else { s }
}

/// The drive `D`, 0.1..15 (1 at position 0.1).
pub fn drive(x: f32) -> f32 {
    if x < C_01 {
        add(mul(x, 9.0), C_01)
    } else {
        powf(15.0, mul(sub(x, C_01), C_DRIVE_EXP))
    }
}

/// The output gain `G`, -9..+9 dB.
pub fn gain(x: f32) -> f32 {
    powf(10.0, mul(sub(x, 0.5), C_GAIN_EXP))
}

/// A shape's factor: Sharp 0.5, Mid 1, Slow 2 (the position compared as a double).
pub fn shape(x: f32) -> f64 {
    let x = f64::from(x);
    if x < 0.333 {
        0.5
    } else if x < 0.666 {
        1.0
    } else {
        2.0
    }
}

/// The split's filter coefficient `k = tan(pi * (995 x^2 + 5) / rate)` as the spec rounds it.
pub fn split_k(x: f32, sample_rate: u32) -> f32 {
    let inv = (1.0 / f64::from(sample_rate)) as f32;
    let arg = add(mul(mul(mul(x, x), inv), C_SPLIT_SQ), mul(inv, C_SPLIT_LIN));
    tanf(arg)
}

/// The split frequency in Hz, about `995 x^2 + 5` (the editor's text).
pub fn split_hz(x: f32) -> f64 {
    let x = f64::from(x);
    995.0 * x * x + 5.0
}

/// The split balance: the low band's and the processed high band's levels.
pub fn balance(x: f32) -> (f32, f32) {
    let r = sub(1.0, x);
    (add(r, r).min(1.0), add(x, x).min(1.0))
}

/// `0.1^y` in doubles, rounded to a single.
pub(super) fn p(y: f64) -> f32 {
    0.1f64.powf(y) as f32
}

/// The followers' and the glide's constants for a rate and the two shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Consts {
    pub(super) kf: f32,
    pub(super) ka: f32,
    pub(super) kr: f32,
    pub(super) kr2: f32,
    pub(super) k4: f32,
    pub(super) k3: f32,
    pub(super) kp: f32,
}

impl Consts {
    pub(super) fn new(sample_rate: u32, ash: f64, rsh: f64) -> Consts {
        let rate = f64::from(sample_rate);
        let kr = p(1.0 / (rsh * 0.02 * rate));
        Consts {
            kf: p(5.0 / rate),
            ka: 1.0 - p(1.0 / (ash * 0.01 * rate)),
            kr,
            kr2: 1.0 - kr,
            k4: p(1.0 / (rsh * 0.1 * rate)),
            k3: p(1.0 / f64::from(sample_rate as f32 * 4.0)),
            kp: 1.0 - p(200.0 / rate),
        }
    }
}

/// The split's coefficients from `k`: `c = 1 / (k + 1)`, `c * k`, `a = -((k - 1) * c)`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Split {
    pub(super) c: f32,
    pub(super) ck: f32,
    pub(super) a: f32,
}

impl Split {
    pub(super) fn new(k: f32) -> Split {
        let c = div(1.0, add(k, 1.0));
        Split {
            c,
            ck: mul(c, k),
            a: -mul(sub(k, 1.0), c),
        }
    }
}
