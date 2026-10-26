// Copyright (C) 2026 ItsAlphaNeon (reference main-output Stereo Shaper)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Four matrix coefficients and intentional channel delay/all-pass; no extra bus.
mod laws;
use super::stereo_enhancer::{Line, rate_factor};
use crate::{
    CommandError,
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
use laws::side;
pub use laws::{AllPass, delay_samples, gains, level_gain, phase_hz};
pub const PARAMS: usize = 7;
pub const RIGHT_INTO_LEFT: usize = 0;
pub const LEFT: usize = 1;
pub const RIGHT: usize = 2;
pub const LEFT_INTO_RIGHT: usize = 3;
pub const DELAY: usize = 4;
pub const PHASE: usize = 5;
pub const POSITION: usize = 6;
pub const IDS: [&str; 7] = ["rl", "left", "right", "lr", "delay", "phase", "position"];
pub const NAMES: [&str; 7] = [
    "From right → left",
    "From left → left",
    "From right → right",
    "From left → right",
    "Delay",
    "Phase",
    "Effects position",
];
pub const RANGES: [(i32, i32); 7] = [
    (-25600, 25600),
    (-25600, 25600),
    (-25600, 25600),
    (-25600, 25600),
    (-4096, 4096),
    (-4096, 4096),
    (-1, 1),
];
pub const DEFAULTS: [i32; 7] = [0, 12800, 12800, 0, 0, 0, 1];
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::StereoShaper,
        available: true,
        parameters: (0..7)
            .map(|i| ParameterInfo {
                id: IDS[i].into(),
                name: NAMES[i].into(),
                unit: if i < 4 {
                    "raw (12800 = 100%)"
                } else {
                    "reference raw"
                }
                .into(),
                value_type: "integer".into(),
                default: ParameterValue::Integer(DEFAULTS[i]),
                minimum: RANGES[i].0 as f64,
                maximum: RANGES[i].1 as f64,
            })
            .collect(),
    }
}
pub fn values(instance: &Instance) -> [i32; 7] {
    IDS.map(|id| match instance.parameters[id] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!("Validated integer"),
    })
}
pub fn value_text(i: usize, v: i32) -> String {
    let side = if v < 0 { "left" } else { "right" };
    match i {
        DELAY => {
            if v == 0 {
                "Off".into()
            } else {
                let ms = (1001f64.powf(v.unsigned_abs() as f64 / 4096.0) - 1.0) * 0.5;
                if ms < 1.0 {
                    format!("{ms:.4} ms {side}")
                } else if ms < 10.0 {
                    format!("{ms:.3} ms {side}")
                } else {
                    format!("{ms:.2} ms {side}")
                }
            }
        }
        PHASE => {
            if v == 0 {
                "Off".into()
            } else {
                let hz = phase_hz(v);
                if hz >= 1000.0 {
                    format!("{:.2} kHz {side}", hz / 1000.0)
                } else {
                    format!("{hz:.1} Hz {side}")
                }
            }
        }
        POSITION => match v {
            -1 => "Off",
            0 => "Pre",
            _ => "Post",
        }
        .into(),
        _ => {
            let p = (v as f64 / 128.0 * 10.0).round() / 10.0;
            format!("{:.1}%", if p == 0.0 { 0.0 } else { p })
        }
    }
}
pub(crate) struct StereoShaper {
    rate: u32,
    settings: [i32; 7],
    current: [f32; 4],
    target: [f32; 4],
    ramp: u32,
    line: Line,
    allpass: Option<AllPass>,
    mem: [f64; 4],
    quantum: u8,
}
impl StereoShaper {
    pub fn new(instance: &Instance, rate: u32) -> Result<Self, CommandError> {
        let settings = values(instance);
        let target = gains(&settings);
        Ok(Self {
            rate,
            settings,
            current: target,
            target,
            ramp: 0,
            line: Line::new(delay_samples(settings[DELAY], rate_factor(rate)))?,
            allpass: AllPass::new(settings[PHASE], rate),
            mem: [0.0; 4],
            quantum: 0,
        })
    }
    fn effects(&mut self, x: &mut [f32; 2]) {
        let d = side(self.settings[DELAY]);
        x[d] = self.line.frame(x[d]);
        if let Some(ap) = self.allpass {
            let [x1, x2, y1, y2] = self.mem;
            let s = f64::from(x[side(self.settings[PHASE])]);
            let y = s * ap.a2 + (((x1 * ap.a1 - y2 * ap.a2) + x2) - y1 * ap.a1);
            self.mem = [s, x1, y, y1];
            x[side(self.settings[PHASE])] = y as f32;
        }
    }
}
impl Processor for StereoShaper {
    fn frame(&mut self, mut x: [f32; 2]) -> [f32; 2] {
        let straight = self.ramp == 0 && self.current[2] == 0.0 && self.current[3] == 0.0;
        if self.ramp > 0 {
            for (c, t) in self.current.iter_mut().zip(self.target) {
                *c += (t - *c) / self.ramp as f32;
            }
            self.ramp -= 1;
            if self.ramp == 0 {
                self.current = self.target;
            }
        }
        if self.settings[POSITION] == 0 {
            self.effects(&mut x);
        }
        let [l, r, rl, lr] = self.current;
        let mut y = if straight {
            [l * x[0], r * x[1]]
        } else {
            [l * x[0] + rl * x[1], r * x[1] + lr * x[0]]
        };
        if self.settings[POSITION] > 0 {
            self.effects(&mut y);
        }
        self.quantum += 1;
        if self.quantum == 64 {
            self.quantum = 0;
            for m in &mut self.mem {
                if m.abs() < 1.0 / 16777216.0 {
                    *m = 0.0;
                }
            }
        }
        y
    }
    fn stereo(&self) -> bool {
        true
    }
    fn has_tail(&self) -> bool {
        self.settings[POSITION] >= 0 && (self.line.len() > 0 || self.allpass.is_some())
    }
    fn needs_context(&self) -> bool {
        self.has_tail()
    }
    fn activity(&self) -> f32 {
        if !self.has_tail() {
            return 0.0;
        }
        if self.mem.iter().any(|v| !v.is_finite()) || !self.line.activity().is_finite() {
            return f32::NAN;
        }
        let matrix = self
            .current
            .into_iter()
            .chain(self.target)
            .map(f32::abs)
            .sum::<f32>()
            .max(1.0);
        let filter = self
            .allpass
            .map_or(0.0, |a| 8.0 / (1.0 - a.a2.abs().sqrt()).max(1e-9));
        (self.line.activity() as f64
            + self.mem.into_iter().map(f64::abs).fold(0.0, f64::max) * filter) as f32
            * matrix
    }
    fn bytes(&self) -> usize {
        self.line.bytes()
    }
    fn reset(&mut self) {
        self.current = self.target;
        self.ramp = 0;
        self.line.clear();
        self.mem = [0.0; 4];
        self.quantum = 0;
    }
    fn can_retarget(&self, p: &dyn Processor) -> bool {
        (p as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|p| p.rate == self.rate && p.settings[DELAY..] == self.settings[DELAY..])
    }
    fn retarget(&mut self, p: &mut dyn Processor) {
        let p = (p as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        std::mem::swap(&mut self.line, &mut p.line);
        self.current = p.current;
        self.mem = p.mem;
        self.quantum = p.quantum;
        self.ramp = (self.rate / 100).max(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob_effect_reference::stereo_shaper as reference;
    fn instance(v: [i32; 7]) -> Instance {
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::StereoShaper,
            bypassed: false,
            parameters: IDS
                .into_iter()
                .zip(v)
                .map(|(id, v)| (id.into(), ParameterValue::Integer(v)))
                .collect(),
        }
    }
    fn compare(v: [i32; 7], rate: u32) {
        let mut a = StereoShaper::new(&instance(v), rate).unwrap();
        let original = [v[0], v[1], v[2], v[3], v[4], v[5], 0, v[6]];
        let mut r =
            reference::StereoShaper::with_settings(rate, &reference::Settings { v: original });
        r.flush();
        let n = rate as usize * 2;
        let mut l: Vec<_> = (0..n)
            .map(|i| {
                if i == 0 {
                    1.0
                } else if i < 1024 {
                    (i as f32 * 0.137).sin()
                } else {
                    0.0
                }
            })
            .collect();
        let mut right: Vec<_> = (0..n)
            .map(|i| {
                if i == 1 {
                    -0.7
                } else if i < 1024 {
                    (i as f32 * 0.193).cos() * 0.3
                } else {
                    0.0
                }
            })
            .collect();
        let input: Vec<_> = l.iter().zip(&right).map(|(&l, &r)| [l, r]).collect();
        for (l, rpart) in l.chunks_mut(64).zip(right.chunks_mut(64)) {
            r.process(l, rpart);
        }
        let mut energy = 0f64;
        let mut error = 0f64;
        for i in 0..n {
            for (x, y) in a.frame(input[i]).into_iter().zip([l[i], right[i]]) {
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
        assert!(a.stereo());
        assert_eq!(a.activity(), 0.0);
        a.reset();
        assert_eq!(
            a.frame([1.0, -1.0]),
            StereoShaper::new(&instance(v), rate)
                .unwrap()
                .frame([1.0, -1.0])
        );
    }
    #[test]
    fn native_reference_matrices_endpoints_phase_delay_positions_and_rates() {
        for rate in [44100, 48000, 96000] {
            compare(DEFAULTS, rate);
            for i in 0..7 {
                for end in [RANGES[i].0, RANGES[i].1] {
                    let mut v = DEFAULTS;
                    v[i] = end;
                    compare(v, rate);
                }
            }
            for matrix in [
                [12800, 0, 0, 12800],
                [-12800, 12800, 12800, -12800],
                [8076, 8076, 8076, -8076],
                [25600, -25600, 25600, -25600],
            ] {
                for position in -1..=1 {
                    compare(
                        [
                            matrix[0], matrix[1], matrix[2], matrix[3], -4096, 4096, position,
                        ],
                        rate,
                    );
                }
            }
        }
    }
    #[test]
    fn held_out_mixed_controls() {
        compare([-7193, 15283, -12731, 3197, -1731, 2713, 0], 96000);
        assert_eq!(value_text(4, -4096), "500.00 ms left");
        assert_eq!(level_gain(25600), 4.0);
        assert_eq!(level_gain(-25600), -4.0);
        assert!(!info().parameters.iter().any(|p| p.id == "difference"));
    }
    #[test]
    fn compatible_matrix_change_keeps_line_filter_and_full_reset() {
        let mut old =
            StereoShaper::new(&instance([0, 12800, 12800, 0, 248, -2496, 1]), 48000).unwrap();
        for _ in 0..128 {
            old.frame([1.0, 2.0]);
        }
        let memory = old.mem;
        let mut next =
            StereoShaper::new(&instance([12800, 0, 0, -12800, 248, -2496, 1]), 48000).unwrap();
        assert!(next.can_retarget(&old));
        next.retarget(&mut old);
        assert_eq!(next.mem, memory);
        for _ in 0..480 {
            next.frame([0.0; 2]);
        }
        assert_eq!(next.current, next.target);
        next.reset();
        assert_eq!(next.mem, [0.0; 4]);
        assert_eq!(next.line.activity(), 0.0);
        assert_eq!(next.quantum, 0);
    }
}
