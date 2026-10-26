// Copyright (C) 2026 ItsAlphaNeon (reference matrix equations)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact static balance matrix laws.
pub fn gains(balance: i32, volume: i32) -> [f32; 4] {
    use std::f64::consts::PI;
    let x = (f64::from(volume) / 320.0 * 1.25) as f32;
    let level = ((f64::from(x) * 11f64.ln()).exp() - 1.0) as f32;
    let b = f64::from(balance);
    let side = b.abs() / 256.0;
    let unit = 1.0 / (10.0 * cos(PI / 4.0));
    let gain = f64::from(level) * (1.0 - 0.5 * sin(side * PI)) * unit;
    let angle = (b / 128.0 + 1.0) * PI / 4.0;
    let ll = gain * cos(angle);
    let rr = gain * sin(angle);
    let (rl, lr) = if balance < 0 {
        (ll - rr, 0.0)
    } else {
        (0.0, rr - ll)
    };
    [ll as f32, rr as f32, rl as f32, lr as f32]
}

/// The double nearest to `sin(x)`, for `|x|` up to a few units (the Taylor series in double-double arithmetic).
pub fn sin(x: f64) -> f64 {
    series(x, Dd(x, 0.0), 1)
}

/// The double nearest to `cos(x)`, for `|x|` up to a few units.
pub fn cos(x: f64) -> f64 {
    series(x, Dd(1.0, 0.0), 0)
}

/// `first - first * x^2 / ((n + 1) (n + 2)) + ...`, summed in double-double until the terms no longer count.
fn series(x: f64, first: Dd, n: u32) -> f64 {
    let x2 = Dd::prod(x, x);
    let (mut term, mut sum, mut n) = (first, first, n);
    while term.0.abs() > 1e-40 * sum.0.abs().max(1e-300) {
        term = term.mul(x2).div(f64::from((n + 1) * (n + 2))).neg();
        sum = sum.add(term);
        n += 2;
    }
    sum.0 + sum.1
}

/// A double-double: the unevaluated sum `hi + lo`, `|lo|` at most half a unit in the last place of `hi`.
#[derive(Clone, Copy)]
struct Dd(f64, f64);

impl Dd {
    /// `a + b` exactly, `|a| >= |b|`.
    fn quick(a: f64, b: f64) -> Dd {
        let s = a + b;
        Dd(s, b - (s - a))
    }

    /// `a + b` exactly.
    fn sum(a: f64, b: f64) -> Dd {
        let s = a + b;
        let bb = s - a;
        Dd(s, (a - (s - bb)) + (b - bb))
    }

    /// `a * b` exactly.
    fn prod(a: f64, b: f64) -> Dd {
        let p = a * b;
        Dd(p, a.mul_add(b, -p))
    }

    fn neg(self) -> Dd {
        Dd(-self.0, -self.1)
    }

    fn add(self, o: Dd) -> Dd {
        let Dd(s, e) = Dd::sum(self.0, o.0);
        Dd::quick(s, e + self.1 + o.1)
    }

    fn mul(self, o: Dd) -> Dd {
        let Dd(p, e) = Dd::prod(self.0, o.0);
        Dd::quick(p, e + (self.0 * o.1 + self.1 * o.0))
    }

    fn div(self, d: f64) -> Dd {
        let q = self.0 / d;
        let r = self.add(Dd::prod(q, d).neg());
        Dd::quick(q, r.0 / d)
    }
}
