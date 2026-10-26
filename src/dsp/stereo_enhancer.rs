// Copyright (C) 2026 ItsAlphaNeon (reference stereo matrix and phase-offset algorithm)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Intentional channel delay is an audible effect, not computational latency.
mod laws;
use crate::{
    CommandError, ErrorCode,
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
pub use laws::{delay_samples, gains, rate_factor, volume_gain};
pub const PARAMS: usize = 6;
pub const PAN: usize = 0;
pub const VOLUME: usize = 1;
pub const SEPARATION: usize = 2;
pub const PHASE: usize = 3;
pub const PLACEMENT: usize = 4;
pub const INVERT: usize = 5;
pub const INVERT_LEFT: i32 = 1;
pub const INVERT_RIGHT: i32 = 2;
pub const IDS: [&str; 6] = [
    "pan",
    "volume",
    "separation",
    "phase",
    "placement",
    "invert",
];
pub const NAMES: [&str; 6] = [
    "Pan",
    "Volume",
    "Separation",
    "Offset",
    "Placement",
    "Inversion",
];
pub const RANGES: [(i32, i32); 6] = [
    (-128, 128),
    (0, 320),
    (-96, 96),
    (-512, 512),
    (0, 1),
    (0, 2),
];
pub const DEFAULTS: [i32; 6] = [0, 256, 0, 0, 1, 0];
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::StereoEnhancer,
        available: true,
        parameters: (0..6)
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
pub fn values(instance: &Instance) -> [i32; 6] {
    IDS.map(|id| match instance.parameters[id] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!("Validated integer"),
    })
}
pub fn value_text(i: usize, v: i32) -> String {
    match i {
        PAN => super::balance::value_text(0, v),
        VOLUME => {
            let g = volume_gain(v);
            if g == 0.0 {
                "−∞ dB · mute".into()
            } else {
                format!("{:.2} dB", 20.0 * g.log10())
            }
        }
        SEPARATION => {
            if v == 0 {
                "None".into()
            } else {
                format!(
                    "{:.1}% {}",
                    v.unsigned_abs() as f64 / 128.0 * 100.0,
                    if v < 0 { "separated" } else { "merged" }
                )
            }
        }
        PHASE => {
            if v == 0 {
                "Off".into()
            } else {
                format!(
                    "{:.2} ms {}",
                    (1001f64.powf(v.unsigned_abs() as f64 / 512.0) - 1.0) * 0.5,
                    if v < 0 { "left" } else { "right" }
                )
            }
        }
        PLACEMENT => if v == 0 { "Pre" } else { "Post" }.into(),
        _ => match v {
            1 => "Left",
            2 => "Right",
            _ => "None",
        }
        .into(),
    }
}
/// An exact-length line with a conservative constant-time activity bound.
pub(super) struct Line {
    data: Vec<f32>,
    at: usize,
    remaining: usize,
    peak: f32,
}
impl Line {
    pub(super) fn new(n: usize) -> Result<Self, CommandError> {
        let mut data = Vec::new();
        data.try_reserve_exact(n).map_err(|_| {
            CommandError::new(
                ErrorCode::ResourceLimit,
                "Cannot allocate stereo offset line",
            )
        })?;
        data.resize(n, 0.0);
        Ok(Self {
            data,
            at: 0,
            remaining: 0,
            peak: 0.0,
        })
    }
    pub(super) fn frame(&mut self, x: f32) -> f32 {
        if self.data.is_empty() {
            return x;
        }
        let y = std::mem::replace(&mut self.data[self.at], x);
        self.at = (self.at + 1) % self.data.len();
        if !x.is_finite() {
            self.peak = f32::NAN;
        } else if x != 0.0 {
            self.remaining = self.data.len();
            self.peak = self.peak.max(x.abs());
        } else {
            self.remaining = self.remaining.saturating_sub(1);
            if self.remaining == 0 {
                self.peak = 0.0;
            }
        }
        y
    }
    pub(super) fn len(&self) -> usize {
        self.data.len()
    }
    pub(super) fn activity(&self) -> f32 {
        self.peak
    }
    pub(super) fn bytes(&self) -> usize {
        self.data.capacity() * 4
    }
    pub(super) fn clear(&mut self) {
        self.data.fill(0.0);
        self.at = 0;
        self.remaining = 0;
        self.peak = 0.0;
    }
}
pub(crate) struct StereoEnhancer {
    rate: u32,
    settings: [i32; 6],
    current: [f32; 4],
    target: [f32; 4],
    ramp: u32,
    line: Line,
    side: usize,
}
impl StereoEnhancer {
    pub fn new(instance: &Instance, rate: u32) -> Result<Self, CommandError> {
        let settings = values(instance);
        let target = gains(&settings);
        let n = delay_samples(settings[PHASE], rate_factor(rate));
        Ok(Self {
            rate,
            settings,
            current: target,
            target,
            ramp: 0,
            line: Line::new(n)?,
            side: usize::from(settings[PHASE] >= 0),
        })
    }
}
impl Processor for StereoEnhancer {
    fn frame(&mut self, mut x: [f32; 2]) -> [f32; 2] {
        if self.ramp > 0 {
            for (c, t) in self.current.iter_mut().zip(self.target) {
                *c += (t - *c) / self.ramp as f32;
            }
            self.ramp -= 1;
            if self.ramp == 0 {
                self.current = self.target;
            }
        }
        if self.settings[PLACEMENT] == 0 {
            x[self.side] = self.line.frame(x[self.side]);
        }
        let [ll, rr, rl, lr] = self.current;
        let mut y = [x[0] * ll + x[1] * rl, x[1] * rr + x[0] * lr];
        if self.settings[PLACEMENT] != 0 {
            y[self.side] = self.line.frame(y[self.side]);
        }
        y
    }
    fn stereo(&self) -> bool {
        true
    }
    fn has_tail(&self) -> bool {
        !self.line.data.is_empty()
    }
    fn needs_context(&self) -> bool {
        !self.line.data.is_empty()
    }
    fn activity(&self) -> f32 {
        if !self.line.peak.is_finite() {
            return f32::NAN;
        }
        let gain = self
            .current
            .into_iter()
            .chain(self.target)
            .map(f32::abs)
            .sum::<f32>()
            .max(1.0);
        self.line.peak * gain
    }
    fn bytes(&self) -> usize {
        self.line.data.capacity() * 4
    }
    fn reset(&mut self) {
        self.current = self.target;
        self.ramp = 0;
        self.line.clear();
    }
    fn can_retarget(&self, p: &dyn Processor) -> bool {
        (p as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|p| {
                p.rate == self.rate
                    && p.settings[PHASE] == self.settings[PHASE]
                    && p.settings[PLACEMENT] == self.settings[PLACEMENT]
            })
    }
    fn retarget(&mut self, p: &mut dyn Processor) {
        let p = (p as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        std::mem::swap(&mut self.line, &mut p.line);
        self.current = p.current;
        self.ramp = (self.rate / 100).max(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob_effect_reference::stereo_enhancer as reference;
    fn instance(v: [i32; 6]) -> Instance {
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::StereoEnhancer,
            bypassed: false,
            parameters: IDS
                .into_iter()
                .zip(v)
                .map(|(id, v)| (id.into(), ParameterValue::Integer(v)))
                .collect(),
        }
    }
    fn compare(v: [i32; 6], rate: u32) {
        let mut a = StereoEnhancer::new(&instance(v), rate).unwrap();
        let mut r = reference::StereoEnhancer::with_settings(rate, &reference::Settings { v });
        r.flush();
        let frames = rate as usize / 2 + 2048;
        let mut l: Vec<_> = (0..frames)
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
        let mut right: Vec<_> = (0..frames)
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
        r.process(&mut l, &mut right);
        let mut error = 0f64;
        let mut energy = 0f64;
        for i in 0..frames {
            for (x, y) in a.frame(input[i]).into_iter().zip([l[i], right[i]]) {
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
            StereoEnhancer::new(&instance(v), rate)
                .unwrap()
                .frame([1.0, -1.0])
        );
    }
    #[test]
    fn native_reference_endpoints_placements_inversion_and_rates() {
        for rate in [44100, 48000, 96000] {
            compare(DEFAULTS, rate);
            for i in 0..6 {
                for end in [RANGES[i].0, RANGES[i].1] {
                    let mut v = DEFAULTS;
                    v[i] = end;
                    compare(v, rate);
                }
            }
            for phase in [-512, -248, 0, 248, 512] {
                for placement in 0..2 {
                    for invert in 0..3 {
                        compare([37, 320, -96, phase, placement, invert], rate);
                    }
                }
            }
        }
    }
    #[test]
    fn held_out_mixed_controls() {
        compare([-73, 283, 57, -371, 0, 2], 96000);
        assert_eq!(value_text(3, 512), "500.00 ms right");
        assert_eq!(delay_samples(512, rate_factor(48000)), 24000);
    }
    #[test]
    fn compatible_matrix_change_keeps_offset_history_and_phase_change_uses_prepared_path() {
        let mut old = StereoEnhancer::new(&instance([0, 256, 0, 248, 1, 0]), 48000).unwrap();
        old.frame([1.0, 2.0]);
        let mut next = StereoEnhancer::new(&instance([64, 320, -96, 248, 1, 2]), 48000).unwrap();
        assert!(next.can_retarget(&old));
        next.retarget(&mut old);
        assert_eq!(next.line.data[0], 2.0);
        for _ in 0..480 {
            next.frame([0.0; 2]);
        }
        assert_eq!(next.current, next.target);
        let changed = StereoEnhancer::new(&instance([64, 320, -96, -248, 1, 2]), 48000).unwrap();
        assert!(!changed.can_retarget(&next));
    }
    #[test]
    fn explicit_mono_conversion_cancels_inverted_dual_mono_without_mutating_the_matrix() {
        let mut p = StereoEnhancer::new(&instance([0, 256, 0, 0, 1, 2]), 48000).unwrap();
        let samples: Vec<_> = (0..257)
            .flat_map(|i| {
                let x = (i as f32 * 0.17).sin();
                p.frame([x, x])
            })
            .collect();
        assert!(samples.as_chunks::<2>().0.iter().all(|f| f[0] == -f[1]));
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let audio = crate::audio::prepare(
            48000,
            2,
            "float32".into(),
            Default::default(),
            samples,
            &cancel,
            |_| {},
        )
        .unwrap();
        let mono = crate::audio::resample::convert(
            &audio,
            &crate::documents::FrameRange { start: 0, end: 257 },
            48000,
            1,
            &cancel,
            |_| {},
        )
        .unwrap();
        assert!(mono.iter().all(|&x| x == 0.0));
        assert_eq!(p.target, [1.0, -1.0, 0.0, -0.0]);
    }
}
