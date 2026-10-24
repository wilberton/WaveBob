// Copyright (C) 2026 ItsAlphaNeon (reference filter mathematics and processing)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordinary seven-band IIR EQ. Reference: Lemon-Studio
//! 0af860dfd31c7ee9cbf8af6747617e031a1a7742, effects/eq/.
//! WaveBob adds fixed-quantum streaming, prepared smooth updates and tail accounting.
mod design;
use crate::{
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
use design::{BandCoefs, Coefs, Stored};
const BANDS: usize = 7;
const LOW_PASS: i32 = 1;
const BAND_PASS: i32 = 2;
const HIGH_PASS: i32 = 3;
const BAND_STOP: i32 = 4;
const LOW_SHELF: i32 = 5;
const PEAKING: i32 = 6;
const HIGH_SHELF: i32 = 7;
pub const QUANTUM: usize = 64;
pub const TYPE_NAMES: [&str; 8] = [
    "Disabled",
    "Low pass",
    "Band pass",
    "High pass",
    "Band stop",
    "Low shelf",
    "Peaking",
    "High shelf",
];
pub const ORDER_NAMES: [&str; 7] = [
    "Steep 8", "Steep 6", "Steep 4", "2", "Gentle 4", "Gentle 6", "Gentle 8",
];
pub const DEFAULT_FREQUENCIES: [i32; 7] = [10923, 18204, 25486, 32768, 40050, 47332, 54613];
pub fn parameter_id(group: &str, band: usize) -> String {
    format!("{group}{}", band + 1)
}
pub fn frequency_hz(raw: i32) -> f64 {
    design::freq_hz(raw) as f64
}
pub fn frequency_raw(hz: f64) -> i32 {
    ((hz.max(20.0) / 20.0).ln() / 1000.0_f64.ln() * 65536.0)
        .round_ties_even()
        .clamp(0.0, 65536.0) as i32
}
pub fn info() -> EffectInfo {
    let mut parameters = Vec::new();
    for (band, frequency) in DEFAULT_FREQUENCIES.into_iter().enumerate() {
        let shelf = band == 0 || band == 6;
        for (group, name, unit, min, max, default) in [
            ("level", "Level", "0.01 dB", -1800, 1800, 0),
            (
                "freq",
                "Frequency",
                "log raw (0=20 Hz, 65536=20 kHz)",
                0,
                65536,
                frequency,
            ),
            (
                "width",
                "Bandwidth",
                "raw / 65536",
                0,
                65536,
                if shelf { 40124 } else { 25412 },
            ),
            (
                "type",
                "Shape",
                "enum 0..7",
                0,
                7,
                if band == 0 {
                    5
                } else if band == 6 {
                    7
                } else {
                    6
                },
            ),
            ("order", "Order", "enum -3..3", -3, 3, 0),
        ] {
            parameters.push(ParameterInfo {
                id: parameter_id(group, band),
                name: format!("Band {} {name}", band + 1),
                unit: unit.into(),
                value_type: "integer".into(),
                default: ParameterValue::Integer(default),
                minimum: min as f64,
                maximum: max as f64,
            });
        }
        parameters.push(ParameterInfo {
            id: parameter_id("enabled", band),
            name: format!("Band {} enabled", band + 1),
            unit: "toggle".into(),
            value_type: "boolean".into(),
            default: ParameterValue::Boolean(true),
            minimum: 0.0,
            maximum: 1.0,
        });
    }
    parameters.push(ParameterInfo {
        id: "main".into(),
        name: "Main level".into(),
        unit: "0.01 dB".into(),
        value_type: "integer".into(),
        default: ParameterValue::Integer(0),
        minimum: -1800.0,
        maximum: 1800.0,
    });
    EffectInfo {
        kind: EffectKind::ParametricEq,
        available: true,
        parameters,
    }
}
#[derive(Clone, Copy)]
struct Settings {
    bands: [Stored; BANDS],
    enabled: [bool; BANDS],
    main: i32,
}
impl Settings {
    fn read(instance: &Instance) -> Self {
        let raw = |id: &str| match instance.parameters[id] {
            ParameterValue::Integer(v) => v,
            _ => unreachable!(),
        };
        Self {
            bands: std::array::from_fn(|band| Stored {
                level: raw(&parameter_id("level", band)),
                freq: raw(&parameter_id("freq", band)),
                width: raw(&parameter_id("width", band)),
                kind: raw(&parameter_id("type", band)),
                order: raw(&parameter_id("order", band)),
            }),
            enabled: std::array::from_fn(|band| {
                matches!(
                    instance.parameters[&parameter_id("enabled", band)],
                    ParameterValue::Boolean(true)
                )
            }),
            main: raw("main"),
        }
    }
}
/// Prepared UI response: evaluate one coefficient bank per visible editor frame.
pub struct Response {
    bands: [BandCoefs; BANDS],
    enabled: [bool; BANDS],
    main: f64,
    rate: f64,
}
impl Response {
    pub fn new(instance: &Instance, rate: u32) -> Self {
        let s = Settings::read(instance);
        Self {
            bands: s.bands.map(|b| design::band(&b, rate as f64)),
            enabled: s.enabled,
            main: s.main as f64 * 0.01,
            rate: rate as f64,
        }
    }
    pub fn db(&self, hz: f64) -> f64 {
        self.main
            + self
                .bands
                .iter()
                .zip(self.enabled)
                .filter(|(_, enabled)| *enabled)
                .flat_map(|(b, _)| b.as_slice())
                .map(|c| {
                    10.0 * c
                        .power(std::f64::consts::TAU * hz / self.rate)
                        .max(1e-24)
                        .log10()
                })
                .sum::<f64>()
    }
}
#[derive(Clone, Copy, Default)]
struct Section {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}
impl Section {
    fn frame(&mut self, c: Coefs, input: f32) -> f32 {
        let input = input as f64;
        let output =
            input * c.b0 + (((self.x1 * c.b1 - self.y2 * c.a2) + self.x2 * c.b2) - self.y1 * c.a1);
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output as f32
    }
    fn tidy(&mut self) {
        for v in [&mut self.x1, &mut self.x2, &mut self.y1, &mut self.y2] {
            if v.abs() < 5.960464477539063e-8 {
                *v = 0.0;
            }
        }
    }
    fn activity(&self) -> f64 {
        self.x1.abs() + self.x2.abs() + self.y1.abs() + self.y2.abs()
    }
}
#[derive(Clone, Copy, Default)]
struct Band {
    coefs: BandCoefs,
    target: BandCoefs,
    state: [[Section; 4]; 2],
    count: i64,
    off: bool,
}
pub(crate) struct ParametricEq {
    rate: u32,
    settings: Settings,
    bands: [Band; BANDS],
    gain: f32,
    target_gain: f32,
    remaining: u32,
    input: [[f32; 2]; QUANTUM],
    output: [[f32; 2]; QUANTUM],
    index: usize,
    gate: i64,
    closed: bool,
}
impl ParametricEq {
    pub fn new(instance: &Instance, rate: u32) -> Self {
        let settings = Settings::read(instance);
        let bands = settings.bands.map(|b| {
            let coefs = design::band(&b, rate as f64);
            Band {
                coefs,
                target: coefs,
                count: rate as i64 / 5,
                ..Default::default()
            }
        });
        let gain = ((settings.main as f32 * 0.01 * design::db_unit()) as f64).exp() as f32;
        Self {
            rate,
            settings,
            bands,
            gain,
            target_gain: gain,
            remaining: 0,
            input: [[0.0; 2]; QUANTUM],
            output: [[0.0; 2]; QUANTUM],
            index: 0,
            gate: 0,
            closed: true,
        }
    }
    fn flat(&self, band: usize) -> bool {
        self.settings.bands[band].has_gain() && self.settings.bands[band].level == 0
    }
    fn runs(&self, band: usize) -> bool {
        self.settings.enabled[band]
            && self.settings.bands[band].kind != 0
            && !(self.flat(band) && self.bands[band].off)
    }
    fn block(&mut self) {
        if self
            .input
            .iter()
            .flatten()
            .any(|v| v.abs() > f32::from_bits(0x358637BD))
        {
            self.gate = self.rate as i64 / 5;
            self.closed = false;
        } else {
            self.gate = self.gate.saturating_sub(QUANTUM as i64);
            if self.gate <= -(QUANTUM as i64) {
                if !self.closed {
                    self.closed = true;
                    for k in 0..BANDS {
                        if self.runs(k) && self.flat(k) {
                            self.bands[k].state = Default::default();
                        }
                    }
                }
                self.output = self.input;
                return;
            }
        }
        let running: [bool; BANDS] = std::array::from_fn(|k| self.runs(k));
        for frame in 0..QUANTUM {
            if self.remaining > 0 {
                let remaining = self.remaining as f64;
                self.gain += (self.target_gain - self.gain) / self.remaining as f32;
                for band in &mut self.bands {
                    for (c, t) in band.coefs.s.iter_mut().zip(band.target.s) {
                        for (v, target) in [
                            (&mut c.b0, t.b0),
                            (&mut c.b1, t.b1),
                            (&mut c.b2, t.b2),
                            (&mut c.a1, t.a1),
                            (&mut c.a2, t.a2),
                        ] {
                            *v += (target - *v) / remaining;
                        }
                    }
                }
                self.remaining -= 1;
                if self.remaining == 0 {
                    self.gain = self.target_gain;
                    for b in &mut self.bands {
                        b.coefs = b.target;
                    }
                }
            }
            let mut value = self.input[frame];
            if self.gain != 1.0 {
                value = value.map(|v| v * self.gain);
            }
            for (band, running) in self.bands.iter_mut().zip(running) {
                if running {
                    for (channel, value) in value.iter_mut().enumerate() {
                        for (coef, state) in
                            band.coefs.as_slice().iter().zip(&mut band.state[channel])
                        {
                            *value = state.frame(*coef, *value);
                        }
                    }
                }
            }
            self.output[frame] = value;
        }
        for (index, running) in running.into_iter().enumerate() {
            if running {
                let flat = self.flat(index);
                let band = &mut self.bands[index];
                for state in band.state.iter_mut().flatten() {
                    state.tidy();
                }
                if flat {
                    band.count -= QUANTUM as i64;
                    if band.count <= 0 {
                        band.off = true;
                        band.state = Default::default();
                    }
                }
            }
        }
    }
}
impl Processor for ParametricEq {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let out = self.output[self.index];
        self.input[self.index] = input;
        self.index += 1;
        if self.index == QUANTUM {
            self.block();
            self.index = 0;
        }
        out
    }
    fn latency(&self) -> usize {
        QUANTUM
    }
    fn has_tail(&self) -> bool {
        (0..BANDS).any(|k| self.runs(k) && !self.flat(k))
    }
    fn activity(&self) -> f32 {
        let queue = self
            .output
            .iter()
            .flatten()
            .map(|v| v.abs())
            .fold(0.0_f32, f32::max);
        let state = if self.closed {
            0.0
        } else {
            self.bands
                .iter()
                .enumerate()
                .filter(|(k, _)| self.runs(*k))
                .flat_map(|(_, b)| b.state.iter().flatten())
                .map(Section::activity)
                .sum::<f64>() as f32
        };
        queue.max(state * 8.0)
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
    fn reset(&mut self) {
        for band in &mut self.bands {
            band.state = Default::default();
            band.count = self.rate as i64 / 5;
            band.off = false;
            band.coefs = band.target;
        }
        self.gain = self.target_gain;
        self.remaining = 0;
        self.gate = 0;
        self.closed = true;
        self.input.fill([0.0; 2]);
        self.output.fill([0.0; 2]);
        self.index = 0;
    }
    fn can_retarget(&self, previous: &dyn Processor) -> bool {
        (previous as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|old| {
                self.rate == old.rate
                    && self.settings.enabled == old.settings.enabled
                    && self
                        .settings
                        .bands
                        .iter()
                        .zip(old.settings.bands)
                        .all(|(a, b)| a.kind == b.kind && a.order == b.order)
            })
    }
    fn retarget(&mut self, previous: &mut dyn Processor) {
        let old = (previous as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        for (band, prior) in self.bands.iter_mut().zip(&old.bands) {
            band.coefs = prior.coefs;
            band.state = prior.state;
            band.off = false;
            band.count = self.rate as i64 / 5;
        }
        self.gain = old.gain;
        self.remaining = (self.rate / 100).max(1);
        self.input = old.input;
        self.output = old.output;
        self.index = old.index;
        self.gate = old.gate;
        self.closed = old.closed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{io::Read, path::Path};
    fn instance(case: &Value) -> Instance {
        let mut instance = Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::ParametricEq,
            bypassed: false,
            parameters: info()
                .parameters
                .into_iter()
                .map(|p| (p.id, p.default))
                .collect(),
        };
        for (group, offset) in [
            ("level", 0),
            ("freq", 7),
            ("width", 14),
            ("type", 21),
            ("order", 28),
        ] {
            for band in 0..7 {
                instance.parameters.insert(
                    parameter_id(group, band),
                    ParameterValue::Integer(
                        case["parameters"][offset + band].as_i64().unwrap() as i32
                    ),
                );
            }
        }
        for band in 0..7 {
            instance.parameters.insert(
                parameter_id("enabled", band),
                ParameterValue::Boolean(case["mask"].as_u64().unwrap() & (1 << band) != 0),
            );
        }
        instance.parameters.insert(
            "main".into(),
            ParameterValue::Integer(case["parameters"][35].as_i64().unwrap() as i32),
        );
        instance
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
    #[test]
    fn deterministic_reset_preserves_mono_policy_and_continuous_state() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/processing/parametric-eq/manifest.json");
        let manifest: Value = serde_json::from_slice(&std::fs::read(root).unwrap()).unwrap();
        let case = manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "peak-48000")
            .unwrap();
        let mut instance = instance(case);
        let mut eq = ParametricEq::new(&instance, 48000);
        assert!(!eq.stereo());
        assert_eq!(eq.latency(), 64);
        assert!(eq.has_tail());
        let expected: Vec<_> = (0..4096)
            .map(|n| eq.frame(if n == 0 { [1.0, -0.5] } else { [0.0; 2] }))
            .collect();
        eq.reset();
        for (n, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                eq.frame(if n == 0 { [1.0, -0.5] } else { [0.0; 2] }),
                expected
            );
        }
        instance
            .parameters
            .insert("freq4".into(), ParameterValue::Integer(0));
        let mut next = ParametricEq::new(&instance, 48000);
        assert!(next.can_retarget(&eq));
        next.retarget(&mut eq);
        for n in 0..10000 {
            assert!(
                next.frame([(n as f32 * 0.17).sin(), (n as f32 * 0.117).cos()])
                    .into_iter()
                    .all(|v| v.is_finite())
            );
        }
        instance
            .parameters
            .insert("order4".into(), ParameterValue::Integer(-3));
        assert!(!ParametricEq::new(&instance, 48000).can_retarget(&next));
    }
    fn compare(held_out: bool) {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/processing/parametric-eq");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        for case in manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| (c["role"] == "held-out") == held_out)
        {
            let instance = instance(case);
            let rate = case["sample_rate"].as_u64().unwrap() as u32;
            let input = pcm(&root.join(case["input"].as_str().unwrap()));
            let expected = pcm(&root.join(case["output"].as_str().unwrap()));
            let mut eq = ParametricEq::new(&instance, rate);
            let frames = input.len() / 2;
            let (mut maximum, mut error_energy, mut energy) = (0.0_f64, 0.0_f64, 0.0_f64);
            for frame in 0..frames + QUANTUM {
                let actual = eq.frame(if frame < frames {
                    [input[frame * 2], input[frame * 2 + 1]]
                } else {
                    [0.0; 2]
                });
                if frame < QUANTUM {
                    assert_eq!(actual, [0.0; 2]);
                    continue;
                }
                for (channel, value) in actual.into_iter().enumerate() {
                    assert!(value.is_finite(), "{}: {frame}", case["name"]);
                    let reference = expected[(frame - QUANTUM) * 2 + channel] as f64;
                    let difference = value as f64 - reference;
                    maximum = maximum.max(difference.abs());
                    error_energy += difference * difference;
                    energy += reference * reference;
                }
            }
            let relative = (error_energy / energy.max(1e-30)).sqrt();
            eprintln!(
                "{}: max {maximum:e}, relative RMS {relative:e}",
                case["name"]
            );
            assert!(maximum <= 2e-6, "{}: maximum {maximum:e}", case["name"]);
            assert!(
                energy <= 1e-12 || relative <= 2e-5,
                "{}: relative {relative:e}",
                case["name"]
            );
            let response = Response::new(&instance, rate);
            for point in case["response"].as_array().unwrap() {
                let hz = point[0].as_f64().unwrap();
                let reference = point[1].as_f64().unwrap();
                let actual = response.db(hz);
                assert!(actual.is_finite());
                if reference > -120.0 {
                    assert!(
                        (actual - reference).abs() <= 2e-5,
                        "{}: {hz} Hz: {actual} != {reference}",
                        case["name"]
                    );
                }
            }
        }
    }
    #[test]
    fn development_reference_pcm_and_response() {
        compare(false);
    }
    #[test]
    fn held_out_reference_pcm_and_response() {
        compare(true);
    }
}
