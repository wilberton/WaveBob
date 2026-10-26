// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference-derived Soft Clipper for WaveBob.
//! WaveBob adaptations: typed controls, stereo frame interface and 10 ms live
//! coefficient ramps. Reset/fixed-setting arithmetic retains reference rounding.
use crate::{
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
pub fn post_gain(raw: i32) -> f32 {
    let position = raw as f32 * 0.0078125;
    if position == 1.0 {
        return 1.0;
    }
    let exponent = position * (11.0_f64.ln() as f32);
    ((f64::from(exponent).exp() - 1.0) * f64::from(0.1_f32)) as f32
}
fn transfer(input: f32, threshold: f32) -> f32 {
    let knee = (1.0 - threshold).max(1.0 / 65536.0);
    let reciprocal = 1.0 / knee;
    if input.abs() <= threshold {
        return input;
    }
    let difference =
        -((f64::from(threshold) - f64::from(input.abs())) * f64::from(reciprocal)).exp_m1();
    let difference = if difference <= 0.5 {
        (difference * 18_446_744_073_709_551_616.0).round_ties_even() / 18_446_744_073_709_551_616.0
    } else {
        difference
    };
    ((difference * f64::from(knee) + f64::from(threshold)) as f32).copysign(input)
}
/// Transfer curve for the editor and deterministic inspection, including post gain.
pub fn curve(input: f32, threshold: i32, post: i32) -> f32 {
    let output = transfer(input, threshold as f32 * 0.0078125);
    let gain = post_gain(post);
    if gain == 1.0 { output } else { output * gain }
}
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::SoftClipper,
        available: true,
        parameters: vec![
            ParameterInfo {
                id: "threshold".into(),
                name: "Threshold".into(),
                unit: "raw (level = raw / 128)".into(),
                value_type: "integer".into(),
                default: ParameterValue::Integer(100),
                minimum: 1.0,
                maximum: 127.0,
            },
            ParameterInfo {
                id: "post".into(),
                name: "Post gain".into(),
                unit: "raw (128 = unity)".into(),
                value_type: "integer".into(),
                default: ParameterValue::Integer(128),
                minimum: 0.0,
                maximum: 160.0,
            },
        ],
    }
}
pub(crate) struct SoftClipper {
    threshold: f64,
    gain: f64,
    target_threshold: f64,
    target_gain: f64,
    remaining: u32,
    rate: u32,
}
impl SoftClipper {
    pub fn new(instance: &Instance, rate: u32) -> Self {
        let integer = |id| match instance.parameters[id] {
            ParameterValue::Integer(value) => value,
            _ => unreachable!("Validated integer parameter"),
        };
        let threshold = (integer("threshold") as f32 * 0.0078125) as f64;
        let gain = post_gain(integer("post")) as f64;
        Self {
            threshold,
            gain,
            target_threshold: threshold,
            target_gain: gain,
            remaining: 0,
            rate,
        }
    }
}
impl Processor for SoftClipper {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.remaining > 0 {
            self.threshold += (self.target_threshold - self.threshold) / self.remaining as f64;
            self.gain += (self.target_gain - self.gain) / self.remaining as f64;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.threshold = self.target_threshold;
                self.gain = self.target_gain;
            }
        }
        input.map(|sample| {
            let output = transfer(sample, self.threshold as f32);
            let gain = self.gain as f32;
            if gain == 1.0 { output } else { output * gain }
        })
    }
    fn reset(&mut self) {
        self.threshold = self.target_threshold;
        self.gain = self.target_gain;
        self.remaining = 0;
    }
    fn can_retarget(&self, previous: &dyn Processor) -> bool {
        (previous as &dyn std::any::Any).is::<Self>()
    }
    fn retarget(&mut self, previous: &mut dyn Processor) {
        let previous = (previous as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        self.threshold = previous.threshold;
        self.gain = previous.gain;
        self.remaining = (self.rate / 100).max(1);
    }
}
