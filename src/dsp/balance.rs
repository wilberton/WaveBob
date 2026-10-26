// Copyright (C) 2026 ItsAlphaNeon (reference folding balance matrix)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Folding stereo matrix, distinct from WaveBob track balance/pan.
mod matrix;
use crate::{
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
pub use matrix::gains;
pub const IDS: [&str; 2] = ["balance", "volume"];
pub const NAMES: [&str; 2] = ["Left / right", "Volume"];
pub const RANGES: [(i32, i32); 2] = [(-128, 128), (0, 320)];
pub const DEFAULTS: [i32; 2] = [0, 256];
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::Balance,
        available: true,
        parameters: (0..2)
            .map(|i| ParameterInfo {
                id: IDS[i].into(),
                name: NAMES[i].into(),
                unit: if i == 0 {
                    "raw (-128 left, 0 centre, 128 right)"
                } else {
                    "raw (256 = unity)"
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
pub fn values(instance: &Instance) -> [i32; 2] {
    IDS.map(|id| match instance.parameters[id] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!("Validated integer"),
    })
}
pub fn value_text(index: usize, raw: i32) -> String {
    if index == 0 {
        if raw == 0 {
            "Centred".into()
        } else {
            format!(
                "{:.1}% {}",
                raw.unsigned_abs() as f64 / 128.0 * 100.0,
                if raw < 0 { "left" } else { "right" }
            )
        }
    } else {
        let g = gains(0, raw)[0];
        if g == 0.0 {
            "−∞ dB · mute".into()
        } else {
            format!("{:.2} dB", 20.0 * g.log10())
        }
    }
}
pub(crate) struct Balance {
    current: [f32; 4],
    target: [f32; 4],
    remaining: u32,
    rate: u32,
}
impl Balance {
    pub fn new(instance: &Instance, rate: u32) -> Self {
        let [b, v] = values(instance);
        let target = gains(b, v);
        Self {
            current: target,
            target,
            remaining: 0,
            rate,
        }
    }
}
impl Processor for Balance {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.remaining > 0 {
            for (current, target) in self.current.iter_mut().zip(self.target) {
                *current += (target - *current) / self.remaining as f32;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            }
        }
        let [ll, rr, rl, lr] = self.current;
        [ll * input[0] + rl * input[1], rr * input[1] + lr * input[0]]
    }
    fn stereo(&self) -> bool {
        true
    }
    fn reset(&mut self) {
        self.current = self.target;
        self.remaining = 0;
    }
    fn can_retarget(&self, p: &dyn Processor) -> bool {
        (p as &dyn std::any::Any).is::<Self>()
    }
    fn retarget(&mut self, p: &mut dyn Processor) {
        self.current = (p as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap()
            .current;
        self.remaining = (self.rate / 100).max(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob_effect_reference::{Effect, balance as reference};
    fn candidate(b: i32, v: i32) -> Balance {
        let mut i = Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::Balance,
            bypassed: false,
            parameters: info()
                .parameters
                .into_iter()
                .map(|p| (p.id, p.default))
                .collect(),
        };
        i.parameters
            .insert("balance".into(), ParameterValue::Integer(b));
        i.parameters
            .insert("volume".into(), ParameterValue::Integer(v));
        Balance::new(&i, 48000)
    }
    fn compare(b: i32, v: i32, held_out: bool) {
        let mut a = candidate(b, v);
        let mut r = reference::Balance::with_settings(48000, &reference::Settings { v: [b, v] });
        let mut expected: Vec<f32> = (0..257)
            .flat_map(|i| match i {
                0 => [1.0, 0.0],
                1 => [0.0, 1.0],
                2 => [1.0, 1.0],
                3 => [1.0, -1.0],
                _ => [
                    (i as f32 * if held_out { 0.271 } else { 0.117 }).sin(),
                    (i as f32 * 0.193).cos() * 0.7,
                ],
            })
            .collect();
        let source = expected.clone();
        Effect::process(&mut r, &mut expected);
        let actual: Vec<_> = source
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|f| a.frame([f[0], f[1]]))
            .collect();
        let mut energy = 0f64;
        let mut error = 0f64;
        for (&x, &y) in actual.iter().zip(&expected) {
            assert!((x - y).abs() <= 2e-6, "{b}/{v}: {x} != {y}");
            error += f64::from(x - y).powi(2);
            energy += f64::from(y).powi(2);
        }
        if energy > 1e-12 {
            assert!((error / energy).sqrt() <= 2e-5);
        }
        a.reset();
        assert_eq!(a.frame([1.0, -0.5]), candidate(b, v).frame([1.0, -0.5]));
    }
    #[test]
    fn native_reference_all_balance_values_and_volume_extremes() {
        for b in -128..=128 {
            for v in [0, 128, 256, 320] {
                compare(b, v, false);
            }
        }
    }
    #[test]
    fn held_out_stereo_and_mapping() {
        compare(-43, 283, true);
        assert_eq!(gains(0, 256), [1.0, 1.0, 0.0, -2f32.powi(-52)]);
        assert_eq!(value_text(0, 0), "Centred");
        assert!(value_text(1, 0).contains("mute"));
    }
    #[test]
    fn smooth_matrix_edit_and_reset_are_bounded() {
        let mut old = candidate(-128, 320);
        let mut new = candidate(128, 0);
        new.retarget(&mut old);
        let mut prev = new.current;
        for _ in 0..480 {
            new.frame([1.0, -1.0]);
            for (a, b) in new.current.iter().zip(prev) {
                assert!((a - b).abs() < 0.01);
            }
            prev = new.current;
        }
        assert_eq!(new.current, new.target);
        assert_eq!(new.frame([2.0, 3.0]), [0.0; 2]);
        assert!(new.stereo());
        assert_eq!(new.latency(), 0);
        assert!(!new.has_tail());
    }
}
