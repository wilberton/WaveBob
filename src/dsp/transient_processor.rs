// Copyright (C) 2026 ItsAlphaNeon (reference Transient Processor algorithm)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Split-band transient detection with prepared coefficients and shared live transitions.
mod laws;
use crate::{
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
use laws::*;
pub use laws::{attack, drive, gain, position, release, shape, split_hz};
pub const PARAMS: usize = 9;
pub const ATTACK: usize = 0;
pub const DRIVE: usize = 1;
pub const RELEASE: usize = 2;
pub const GAIN: usize = 3;
pub const ATTACK_SHAPE: usize = 4;
pub const RELEASE_SHAPE: usize = 5;
pub const SPLIT: usize = 6;
pub const BALANCE: usize = 7;
pub const ON: usize = 8;
pub const IDS: [&str; 9] = [
    "attack",
    "drive",
    "release",
    "gain",
    "attack_shape",
    "release_shape",
    "split",
    "balance",
    "on",
];
pub const NAMES: [&str; 9] = [
    "Attack",
    "Drive",
    "Release",
    "Gain",
    "Attack shape",
    "Release shape",
    "Split frequency",
    "Split balance",
    "Processing",
];
pub const RANGES: [(i32, i32); 9] = [
    (0, 160),
    (0, 160),
    (0, 160),
    (0, 160),
    (0, 2),
    (0, 2),
    (0, 160),
    (0, 160),
    (0, 1),
];
pub const DEFAULTS: [i32; 9] = [80, 0, 80, 80, 1, 1, 80, 80, 1];
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::TransientProcessor,
        available: true,
        parameters: (0..9)
            .map(|i| ParameterInfo {
                id: IDS[i].into(),
                name: NAMES[i].into(),
                unit: "reference raw".into(),
                value_type: "integer".into(),
                default: ParameterValue::Integer(DEFAULTS[i]),
                minimum: RANGES[i].0 as f64,
                maximum: RANGES[i].1 as f64,
            })
            .collect(),
    }
}
pub fn values(instance: &Instance) -> [i32; 9] {
    IDS.map(|id| match instance.parameters[id] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!("Validated integer"),
    })
}
pub fn value_text(i: usize, v: i32) -> String {
    let x = position(i, v);
    match i {
        ATTACK | RELEASE => {
            let w = if i == ATTACK { attack(x) } else { release(x) };
            if w == 0.0 {
                "0%".into()
            } else {
                format!("{:+.2}%", w * 100.0)
            }
        }
        DRIVE | GAIN => format!(
            "{:.2} dB",
            20.0 * if i == DRIVE { drive(x) } else { gain(x) }.log10()
        ),
        ATTACK_SHAPE | RELEASE_SHAPE => match v {
            0 => "Sharp",
            1 => "Mid",
            _ => "Slow",
        }
        .into(),
        SPLIT => format!("{:.1} Hz", split_hz(x)),
        BALANCE => {
            if v == 80 {
                "Centred".into()
            } else {
                format!(
                    "{:.1}% {}",
                    (v as f64 - 80.0).abs() / 80.0 * 100.0,
                    if v < 80 { "low" } else { "high" }
                )
            }
        }
        _ => if v == 0 { "Off" } else { "On" }.into(),
    }
}
#[derive(Clone, Copy)]
struct Work {
    attack: f32,
    release: f32,
    drive: f32,
    out_gain: f32,
    split: Split,
    lo: f32,
    hi: f32,
    consts: Consts,
}
impl Work {
    fn new(v: [i32; 9], rate: u32) -> Self {
        let p: [f32; 9] = std::array::from_fn(|i| position(i, v[i]));
        let (lo, hi) = balance(p[BALANCE]);
        Self {
            attack: attack(p[ATTACK]),
            release: release(p[RELEASE]),
            drive: drive(p[DRIVE]),
            out_gain: gain(p[GAIN]),
            split: Split::new(split_k(p[SPLIT], rate)),
            lo,
            hi,
            consts: Consts::new(rate, shape(p[ATTACK_SHAPE]), shape(p[RELEASE_SHAPE])),
        }
    }
    fn slew(&mut self, t: Self, n: u32) {
        let n = n as f32;
        macro_rules! step{($($field:ident).+)=>{self.$($field).+ += (t.$($field).+ - self.$($field).+)/n;};}
        step!(attack);
        step!(release);
        step!(drive);
        step!(out_gain);
        step!(split.c);
        step!(split.ck);
        step!(split.a);
        step!(lo);
        step!(hi);
        step!(consts.kf);
        step!(consts.ka);
        step!(consts.kr);
        step!(consts.kr2);
        step!(consts.k4);
        step!(consts.k3);
        step!(consts.kp);
    }
}
pub(crate) struct TransientProcessor {
    rate: u32,
    settings: [i32; 9],
    work: Work,
    target: Work,
    ramp: u32,
    attack_copy: f32,
    release_copy: f32,
    drive_copy: f32,
    e: [f32; 5],
    x1: [f32; 2],
    low1: [f32; 2],
    high1: [f32; 2],
    count: u64,
    quiet: i64,
}
impl TransientProcessor {
    pub fn new(instance: &Instance, rate: u32) -> Self {
        let settings = values(instance);
        let target = Work::new(settings, rate);
        Self {
            rate,
            settings,
            work: target,
            target,
            ramp: 0,
            attack_copy: 0.0,
            release_copy: 0.0,
            drive_copy: DRIVE_AT_CREATION,
            e: [0.0; 5],
            x1: [0.0; 2],
            low1: [0.0; 2],
            high1: [0.0; 2],
            count: 0,
            quiet: 0,
        }
    }
    /// One sample pair (spec "The split", "The gain, per sample", "Saturation and output").
    fn process_frame(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.count = self.count.wrapping_add(1);
        let Split { c, ck, a } = self.work.split;
        let x = [z(l), z(r)];
        let mut low = [0.0f32; 2];
        let mut high = [0.0f32; 2];
        for ch in 0..2 {
            low[ch] = add(
                add(mul(ck, x[ch]), mul(ck, self.x1[ch])),
                mul(a, self.low1[ch]),
            );
            high[ch] = add(
                add(mul(c, x[ch]), mul(-c, self.x1[ch])),
                mul(a, self.high1[ch]),
            );
        }
        self.x1 = x;
        self.low1 = low;
        self.high1 = high;

        let k = self.work.consts;
        let m = mul(add(high[1].abs(), high[0].abs()), 0.5);
        let [e1, e2, e3, e4, e5] = &mut self.e;
        *e1 = if m > *e1 { m } else { mul(*e1, k.kf) };
        *e2 = if m > *e2 {
            add(mul(sub(m, *e2), k.ka), *e2)
        } else {
            mul(*e2, k.kf)
        };
        *e3 = if m >= *e3 { m } else { mul(*e3, k.k3) };
        *e4 = if m >= *e4 { m } else { mul(*e4, k.k4) };
        let q = if self.release_copy >= 0.0 && *e4 > 0.0 {
            let d = add(*e4, C_1E4);
            div(sub(d, *e5), d)
        } else {
            1.0
        };
        let q = mul(q, 3.0).clamp(0.0, 1.0);
        *e5 = add(mul(add(mul(q, k.kr), k.kr2), sub(*e4, *e5)), *e5);
        let (e1, e2, e3, e5) = (*e1, *e2, *e3, *e5);

        let ra = if e2 > 0.0 { div(e1, e2) } else { 0.0 };
        let att = if e2 > 0.0 && ra >= 1.0 {
            if self.attack_copy < 0.0 {
                ra
            } else if ra <= 10.0 {
                mul(ra, sub(C_A1, mul(mul(ra, C_A2), ra)))
            } else {
                C_A_MAX
            }
        } else {
            1.0
        };
        let rb = if e5 > C_1E6 { div(e3, e5) } else { 0.0 };
        let rel = if e5 > C_1E6 && rb >= 1.0 {
            if self.release_copy < 0.0 {
                rb
            } else if rb <= 20.0 {
                sub(mul(rb, C_B1), mul(mul(rb, rb), mul(rb, C_B2)))
            } else {
                12.0
            }
        } else {
            1.0
        };
        let s = add(
            mul(sub(rel, 1.0), self.release_copy),
            mul(sub(att, 1.0), self.attack_copy),
        );
        let g = if s >= 0.0 {
            add(s, 1.0)
        } else {
            div(1.0, sub(1.0, s))
        };

        let d = self.drive_copy;
        let mut out = [0.0f32; 2];
        for ch in 0..2 {
            let u = mul(d, mul(high[ch], g));
            let sat = if u < -3.0 {
                -1.0
            } else if u > 3.0 {
                1.0
            } else {
                div(mul(add(mul(u, u), 27.0), u), add(mul(mul(u, 9.0), u), 27.0))
            };
            let y = div(sat, d);
            out[ch] = mul(
                add(mul(self.work.lo, low[ch]), mul(self.work.hi, y)),
                self.work.out_gain,
            );
        }

        self.attack_copy = add(sub(self.work.attack, self.attack_copy), self.attack_copy);
        self.release_copy = add(sub(self.work.release, self.release_copy), self.release_copy);
        self.drive_copy = add(sub(self.work.drive, self.drive_copy), self.drive_copy);
        (out[0], out[1])
    }
}
impl Processor for TransientProcessor {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.ramp > 0 {
            self.work.slew(self.target, self.ramp);
            self.ramp -= 1;
            if self.ramp == 0 {
                self.work = self.target;
            }
        }
        if input.iter().all(|x| x.abs() <= C_1E6) {
            self.quiet -= 1;
        } else {
            self.quiet = (self.rate as f64 * 1.5).round_ties_even() as i64 + 88;
        }
        if self.quiet <= -1 {
            self.quiet = STILL;
            return input;
        }
        if self.settings[ON] == 0 {
            return input;
        }
        let (l, r) = self.process_frame(input[0], input[1]);
        [l, r]
    }
    fn has_tail(&self) -> bool {
        self.settings[ON] != 0
    }
    fn needs_context(&self) -> bool {
        true
    }
    fn activity(&self) -> f32 {
        if self
            .e
            .iter()
            .chain(&self.x1)
            .chain(&self.low1)
            .chain(&self.high1)
            .any(|v| !v.is_finite())
        {
            return f32::NAN;
        }
        if self.settings[ON] == 0 {
            return 0.0;
        }
        self.x1
            .iter()
            .chain(&self.low1)
            .chain(&self.high1)
            .map(|v| v.abs())
            .sum::<f32>()
            * 64.0
    }
    fn reset(&mut self) {
        self.work = self.target;
        self.ramp = 0;
        self.attack_copy = 0.0;
        self.release_copy = 0.0;
        self.drive_copy = DRIVE_AT_CREATION;
        self.e = [0.0; 5];
        self.x1 = [0.0; 2];
        self.low1 = [0.0; 2];
        self.high1 = [0.0; 2];
        self.count = 0;
        self.quiet = 0;
    }
    fn can_retarget(&self, p: &dyn Processor) -> bool {
        (p as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|p| p.rate == self.rate && p.settings[ON] == self.settings[ON])
    }
    fn retarget(&mut self, p: &mut dyn Processor) {
        let p = (p as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        self.work = p.work;
        self.ramp = (self.rate / 100).max(1);
        self.attack_copy = p.attack_copy;
        self.release_copy = p.release_copy;
        self.drive_copy = p.drive_copy;
        self.e = p.e;
        self.x1 = p.x1;
        self.low1 = p.low1;
        self.high1 = p.high1;
        self.count = p.count;
        self.quiet = p.quiet;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob_effect_reference::transient_processor as reference;
    fn instance(v: [i32; 9]) -> Instance {
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::TransientProcessor,
            bypassed: false,
            parameters: IDS
                .into_iter()
                .zip(v)
                .map(|(id, v)| (id.into(), ParameterValue::Integer(v)))
                .collect(),
        }
    }
    fn compare(v: [i32; 9], rate: u32, held_out: bool) {
        let mut a = TransientProcessor::new(&instance(v), rate);
        let mut r = reference::TransientProcessor::with_settings(rate, &reference::Settings { v });
        let n = rate as usize * 2;
        let mut energy = 0f64;
        let mut error = 0f64;
        for i in 0..n {
            let input = if i == 0 {
                [2.0, -0.7]
            } else if i < 2048 {
                let env = if held_out {
                    (-(i as f32) * 0.0007).exp()
                } else {
                    1.0
                };
                [
                    (i as f32 * 0.137).sin() * env,
                    (i as f32 * 0.193).cos() * 0.3 * env,
                ]
            } else if i == n - 257 {
                [1.1, -0.3]
            } else {
                [0.0; 2]
            };
            let (mut l, mut right) = ([input[0]], [input[1]]);
            r.process(&mut l, &mut right);
            for (x, y) in a.frame(input).into_iter().zip([l[0], right[0]]) {
                assert!(x.is_finite());
                assert!((x - y).abs() <= 2e-6, "{v:?}/{rate}/{i}: {x} != {y}");
                error += f64::from(x - y).powi(2);
                energy += f64::from(y).powi(2);
            }
        }
        if energy > 1e-12 {
            assert!((error / energy).sqrt() <= 2e-5);
        }
        assert_eq!(a.latency(), 0);
        a.reset();
        assert_eq!(
            a.frame([1.0, -1.0]),
            TransientProcessor::new(&instance(v), rate).frame([1.0, -1.0])
        );
    }
    #[test]
    fn native_reference_endpoints_shape_pairs_and_rates() {
        for rate in [44100, 48000, 96000] {
            compare(DEFAULTS, rate, false);
            for i in 0..9 {
                for end in [RANGES[i].0, RANGES[i].1] {
                    let mut v = DEFAULTS;
                    v[i] = end;
                    compare(v, rate, false);
                }
            }
            for attack_shape in 0..3 {
                for release_shape in 0..3 {
                    compare(
                        [160, 160, 0, 117, attack_shape, release_shape, 160, 160, 1],
                        rate,
                        false,
                    );
                }
            }
        }
    }
    #[test]
    fn held_out_mixed_controls() {
        compare([127, 93, 41, 113, 2, 0, 51, 129, 1], 96000, true);
        assert_eq!(value_text(0, 80), "0%");
        assert_eq!(value_text(6, 160), "1000.0 Hz");
    }
    #[test]
    fn compatible_changes_keep_detector_and_crossover_and_reset_is_complete() {
        let mut old = TransientProcessor::new(&instance(DEFAULTS), 48000);
        for _ in 0..127 {
            old.frame([1.0, 0.7]);
        }
        let state = old.e;
        let mut next =
            TransientProcessor::new(&instance([160, 160, 0, 117, 2, 0, 160, 160, 1]), 48000);
        assert!(next.can_retarget(&old));
        next.retarget(&mut old);
        assert_eq!(next.e, state);
        assert_eq!(next.count, 127);
        for _ in 0..480 {
            next.frame([0.0; 2]);
        }
        assert_eq!(next.ramp, 0);
        next.reset();
        assert_eq!(next.e, [0.0; 5]);
        assert_eq!(next.count, 0);
        assert_eq!(next.low1, [0.0; 2]);
        assert_eq!(next.drive_copy, 0.01);
    }
}
