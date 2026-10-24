// Copyright (C) 2026 ItsAlphaNeon (reference algorithm)
// SPDX-License-Identifier: GPL-3.0-or-later
//! WaveBob's Fast Distortion follows Lemon-Studio
//! 0af860dfd31c7ee9cbf8af6747617e031a1a7742, effects/fast_dist.rs.
//! Tables prepare off callback; continuous gain/mix edits slew for 10 ms.
use crate::{
    CommandError, ErrorCode,
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
pub const IDS: [&str; 5] = ["pre", "threshold", "type", "mix", "post"];
pub const DEFAULTS: [i32; 5] = [128, 10, 0, 128, 128];
pub const RANGES: [(i32, i32); 5] = [(64, 192), (1, 10), (0, 1), (0, 128), (0, 128)];
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::FastDistortion,
        available: true,
        parameters: (0..5)
            .map(|i| ParameterInfo {
                id: IDS[i].into(),
                name: ["Pre amp", "Threshold", "Curve type", "Mix", "Post gain"][i].into(),
                unit: [
                    "raw (128 = unity)",
                    "1..10",
                    "0 = A, 1 = B",
                    "raw exponential share (128 = 100%)",
                    "raw (128 = unity)",
                ][i]
                    .into(),
                value_type: "integer".into(),
                default: ParameterValue::Integer(DEFAULTS[i]),
                minimum: RANGES[i].0 as f64,
                maximum: RANGES[i].1 as f64,
            })
            .collect(),
    }
}
pub fn values(instance: &Instance) -> [i32; 5] {
    std::array::from_fn(|i| match instance.parameters[IDS[i]] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!(),
    })
}
pub fn level(raw: i32) -> f32 {
    super::soft_clipper::post_gain(raw)
}
pub fn value_text(index: usize, raw: i32) -> String {
    match index {
        1 => raw.to_string(),
        2 => if raw == 0 { "A" } else { "B" }.into(),
        3 => format!("{:.1}%", level(raw) as f64 * 100.0),
        _ => {
            if raw == 0 {
                "−∞ dB".into()
            } else {
                format!("{:.1} dB", 20.0 * (level(raw) as f64).log10())
            }
        }
    }
}
fn gains(v: [i32; 5]) -> [f64; 3] {
    let pre = level(v[0]);
    let post = level(v[4]);
    let wet = level(v[3]) * post;
    let dry = (post - wet) * pre * post;
    [pre as f64 * 8191.0, wet as f64, dry as f64]
}
fn table_level(index: usize, threshold: i32, kind: i32) -> f64 {
    if index == 0 {
        return 0.0;
    }
    let db = 20.0 * (index as f64 / 8191.0).log10();
    let step = (10 - threshold) as f64;
    let shaped = if kind == 0 {
        let compressed = if db >= -25.0 {
            (db + 25.0) * (step + 1.0) * 0.1 - 25.0
        } else {
            db
        };
        compressed + (9.0 - step) * 1.6
    } else {
        -(-db).powf(step * 0.02 + 0.8)
    };
    (10.0_f64.powf(shaped / 20.0) * 32767.0).round_ties_even() / 32768.0
}
fn toward_zero(value: f64) -> f32 {
    let rounded = value as f32;
    if (rounded as f64).abs() > value.abs() {
        f32::from_bits(rounded.to_bits() - 1)
    } else {
        rounded
    }
}
/// Prepared exact transfer for both the audio stage and visible UI graph.
pub struct Curve {
    table: Vec<f64>,
    key: [i32; 2],
    gains: [f64; 3],
}
impl Curve {
    pub fn new(values: [i32; 5]) -> Result<Self, CommandError> {
        if values
            .iter()
            .zip(RANGES)
            .any(|(value, (min, max))| !(min..=max).contains(value))
        {
            return Err(crate::processing::invalid(
                "Fast Distortion controls exceed their supported ranges",
            ));
        }
        let mut table = Vec::new();
        table.try_reserve_exact(8193).map_err(|_| {
            CommandError::new(
                ErrorCode::ResourceLimit,
                "Cannot allocate Fast Distortion curve table",
            )
        })?;
        for index in 0..8192 {
            table.push(table_level(index, values[1], values[2]));
        }
        table.push(table[8191]);
        Ok(Self {
            table,
            key: [values[1], values[2]],
            gains: gains(values),
        })
    }
    fn apply(&self, input: f32, gains: [f64; 3]) -> f32 {
        let input = input as f64;
        let position = (input * gains[0]).abs();
        let shaped = if position < 2147483648.0 {
            let index = position.min(8191.0) as usize;
            let fraction = position - position.trunc();
            let a = self.table[index] * gains[1];
            let b = self.table[index + 1] * gains[1];
            (a + (b - a) * fraction).copysign(input)
        } else {
            0.0
        };
        toward_zero(input * gains[2] + shaped)
    }
    pub fn sample(&self, input: f32) -> f32 {
        self.apply(input, self.gains)
    }
}
pub(crate) struct FastDistortion {
    curve: Curve,
    current: [f64; 3],
    target: [f64; 3],
    remaining: u32,
    rate: u32,
}
impl FastDistortion {
    pub fn new(instance: &Instance, rate: u32) -> Result<Self, CommandError> {
        let curve = Curve::new(values(instance))?;
        let target = curve.gains;
        Ok(Self {
            curve,
            current: target,
            target,
            remaining: 0,
            rate,
        })
    }
}
impl Processor for FastDistortion {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.remaining > 0 {
            for (v, t) in self.current.iter_mut().zip(self.target) {
                *v += (t - *v) / self.remaining as f64;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            }
        }
        input.map(|v| self.curve.apply(v, self.current))
    }
    fn reset(&mut self) {
        self.current = self.target;
        self.remaining = 0;
    }
    fn bytes(&self) -> usize {
        self.curve.table.capacity() * 8
    }
    fn can_retarget(&self, previous: &dyn Processor) -> bool {
        (previous as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|old| self.rate == old.rate && self.curve.key == old.curve.key)
    }
    fn retarget(&mut self, previous: &mut dyn Processor) {
        let old = (previous as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        self.current = old.current;
        self.remaining = (self.rate / 100).max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{io::Read, path::Path};
    fn instance(values: [i32; 5]) -> Instance {
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::FastDistortion,
            bypassed: false,
            parameters: IDS
                .into_iter()
                .zip(values)
                .map(|(id, value)| (id.into(), ParameterValue::Integer(value)))
                .collect(),
        }
    }
    #[test]
    fn gain_updates_slew_for_ten_milliseconds_reset_is_exact_and_curves_crossfade() {
        let mut old = FastDistortion::new(&instance(DEFAULTS), 48000).unwrap();
        let initial = old.frame([0.75, -0.75])[0];
        let mut muted = FastDistortion::new(&instance([128, 10, 0, 128, 0]), 48000).unwrap();
        assert!(muted.can_retarget(&old));
        muted.retarget(&mut old);
        let mut prior = initial;
        for _ in 0..480 {
            let out = muted.frame([0.75, -0.75]);
            assert_eq!(out[0], -out[1]);
            assert!(out[0] <= prior && prior - out[0] <= initial / 480.0 + 1e-7);
            prior = out[0];
        }
        assert_eq!(prior, 0.0);
        assert_eq!(muted.frame([1.0, -1.0]), [0.0; 2]);
        let values = [192, 10, 0, 64, 64];
        let mut next = FastDistortion::new(&instance(values), 48000).unwrap();
        assert!(next.can_retarget(&muted));
        next.retarget(&mut muted);
        next.frame([0.75; 2]);
        next.reset();
        let expected = Curve::new(values).unwrap().sample(0.75);
        for _ in 0..20 {
            assert_eq!(next.frame([0.75; 2]), [expected; 2]);
        }
        assert!(!next.stereo());
        assert!(!next.has_tail());
        assert_eq!(next.latency(), 0);
        assert!(next.bytes() >= 8193 * 8);
        assert!(
            !FastDistortion::new(&instance([192, 10, 1, 64, 64]), 48000)
                .unwrap()
                .can_retarget(&next)
        );
        assert!(
            !FastDistortion::new(&instance([192, 1, 0, 64, 64]), 48000)
                .unwrap()
                .can_retarget(&next)
        );
    }
    fn pcm(path: &Path) -> Vec<f32> {
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect()
    }
    fn compare(held_out: bool) {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/processing/fast-distortion");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        for case in manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| (c["role"] == "held-out") == held_out)
        {
            let values = std::array::from_fn(|i| case["values"][i].as_i64().unwrap() as i32);
            let curve = Curve::new(values).unwrap();
            let input = pcm(&root.join(case["input"].as_str().unwrap()));
            let expected = pcm(&root.join(case["output"].as_str().unwrap()));
            assert_eq!(input.len(), expected.len());
            let mut maximum = 0.0_f32;
            for (input, expected) in input.into_iter().zip(expected) {
                let actual = curve.sample(input);
                assert!(actual.is_finite());
                maximum = maximum.max((actual - expected).abs());
                assert!(
                    (actual - expected).abs() <= 2e-7,
                    "{}: input {input}: {actual} != {expected}",
                    case["name"]
                );
            }
            eprintln!("{}: maximum error {maximum:e}", case["name"]);
        }
    }
    #[test]
    fn development_reference_captures() {
        compare(false);
    }
    #[test]
    fn held_out_reference_capture() {
        compare(true);
    }
}
