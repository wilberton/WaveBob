// Copyright (C) 2026 ItsAlphaNeon (reference algorithm)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Prepared multi-model delay, following Lemon-Studio 0af860d effects/delay.rs.
//! Intentional echoes have zero computational latency. Storage is fallible and
//! bounded; live coefficient/time changes preserve the original stepped laws.
mod network;
use crate::{
    CommandError,
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
use network::*;
pub use network::{Distortion, RANGES};
pub const IDS: [&str; 26] = [
    "input",
    "type",
    "tempo_sync",
    "keep_pitch",
    "time",
    "offset",
    "smooth",
    "spread",
    "mod_rate",
    "mod_time",
    "mod_cutoff",
    "diffusion_level",
    "diffusion_spread",
    "distortion_type",
    "feedback",
    "filter",
    "cutoff",
    "resonance",
    "drive",
    "hardness",
    "symmetry",
    "rate_reduction",
    "bits",
    "wet",
    "tone",
    "dry",
];
pub const DEFAULTS: [i32; 26] = [
    6000, 1, 1, 0, 192, 3000, 500, 6000, 3000, 0, 0, 0, 0, 1, 3000, 0, 6000, 0, 0, 0, 0, 60000,
    6000, 6000, 0, 6000,
];
pub const NAMES: [&str; 26] = [
    "Input",
    "Delay model",
    "Tempo sync",
    "Keep pitch",
    "Time",
    "R/L offset",
    "Smoothing",
    "Stereo spread",
    "Modulation rate",
    "Time modulation",
    "Cutoff modulation",
    "Diffusion level",
    "Diffusion spread",
    "Distortion type",
    "Feedback",
    "Filter mode",
    "Cutoff",
    "Resonance",
    "Drive",
    "Knee",
    "Symmetry",
    "Rate reduction",
    "Bit reduction",
    "Wet",
    "Output tone",
    "Dry",
];
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::Delay,
        available: true,
        parameters: (0..26)
            .map(|i| ParameterInfo {
                id: IDS[i].into(),
                name: NAMES[i].into(),
                unit: match i {
                    2 | 3 => "toggle",
                    1 => "0 Mono / 1 True stereo / 2 Ping pong / 3 Off",
                    13 => "0 Limit / 1 Saturate",
                    15 => "0 LP / 1 HP / 2 BP / 3 Off",
                    _ => "raw (see derived value in editor)",
                }
                .into(),
                value_type: if matches!(i, 2 | 3) {
                    "boolean"
                } else {
                    "integer"
                }
                .into(),
                default: if matches!(i, 2 | 3) {
                    ParameterValue::Boolean(DEFAULTS[i] != 0)
                } else {
                    ParameterValue::Integer(DEFAULTS[i])
                },
                minimum: RANGES[i].0 as f64,
                maximum: RANGES[i].1 as f64,
            })
            .collect(),
    }
}
pub fn values(instance: &Instance) -> [i32; 26] {
    std::array::from_fn(|i| match instance.parameters[IDS[i]] {
        ParameterValue::Integer(v) => v,
        ParameterValue::Boolean(v) => v as i32,
        _ => unreachable!(),
    })
}
pub fn value_text(i: usize, v: &[i32; 26], bpm: f64) -> String {
    let x = norm(i, v[i]);
    match i {
        TYPE => ["Mono", "True stereo", "Ping pong", "Off"][v[i] as usize].into(),
        SYNC | KEEP_PITCH => if v[i] != 0 { "On" } else { "Off" }.into(),
        TIME if v[SYNC] != 0 => format!(
            "{}:{:02} · {:.3} ms",
            v[i] / 48,
            v[i] % 48,
            delay_seconds(x, true, bpm) * 1000.0
        ),
        TIME => format!("{:.3} ms", delay_seconds(x, false, bpm) * 1000.0),
        OFFSET | TONE => format!("{:+.1}%", (x - 0.5) * 200.0),
        FEEDBACK => format!("{:.1}%", x * 125.0),
        MOD_RATE => format!("{:.3} Hz", 1.0 / lfo_period(x)),
        RATE_RED => format!("{:.1} Hz", 44100.0 / hold_period(1.0 - x)),
        BITS if v[i] == 6000 => "Max bits".into(),
        BITS => format!("{:.2} bits", bits(x)),
        DIST_TYPE => if v[i] == 0 { "Limit" } else { "Saturate" }.into(),
        FILTER => ["Low pass", "High pass", "Band pass", "Off"][v[i] as usize].into(),
        CUTOFF => format!(
            "{:.1} Hz",
            if v[FILTER] == HIGH_PASS {
                lookup(&HP, x * 3.999)
            } else {
                lookup(&LP, x * if v[FILTER] == BAND_PASS { 3.6 } else { 3.999 })
            }
        ),
        SMOOTH => format!("{:.1} ms", x * 1000.0),
        _ => format!("{:.1}%", x * 100.0),
    }
}
pub(crate) struct Delay {
    network: Network,
    initial: State,
    positions: [usize; 8],
    settings: Settings,
    excited: bool,
}
impl Delay {
    pub fn new(instance: &Instance, rate: u32, bpm: f64) -> Result<Self, CommandError> {
        let settings = Settings {
            v: values(instance),
        };
        let mut network = Network::new(rate, settings.v[TYPE])?;
        network.set(&settings);
        network.set_tempo(bpm);
        network.start();
        let initial = network.state;
        let positions = std::array::from_fn(|i| match i {
            0..2 => network.lines[i].pos,
            2..4 => network.inputs[i - 2].pos,
            _ => network.allpass[(i - 4) / 2][(i - 4) % 2].pos,
        });
        Ok(Self {
            network,
            initial,
            positions,
            settings,
            excited: false,
        })
    }
    fn wet_active(&self) -> bool {
        let s = &self.network.state;
        s.wet != 0.0 || s.target[WET] != 0.0
    }
}
impl Processor for Delay {
    #[allow(clippy::excessive_precision)] // Same rounded f32 input gate as the original.
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.excited |= input.iter().any(|x| x.abs() > 9.999_999_8e-14);
        let (l, r) = self.network.frame(input[0], input[1]);
        [l, r]
    }
    fn reset(&mut self) {
        self.network.state = self.initial;
        self.excited = false;
        for (i, r) in self
            .network
            .lines
            .iter_mut()
            .chain(self.network.inputs.iter_mut())
            .chain(self.network.allpass.iter_mut().flatten())
            .enumerate()
        {
            r.clear();
            r.pos = self.positions[i];
        }
    }
    fn stereo(&self) -> bool {
        true
    }
    fn has_tail(&self) -> bool {
        let s = &self.network.state;
        self.wet_active()
            && (s.kind != OFF
                || s.filter != FILTER_OFF
                || s.colour_hz.is_some()
                || s.rate_amount > 0.0)
    }
    fn activity(&self) -> f32 {
        if !self.excited || !self.wet_active() {
            return 0.0;
        }
        let s = &self.network.state;
        let age = |index: usize| {
            s.sets
                .iter()
                .map(|set| set.live[index].max(set.target[index] as f64))
                .fold(0.0_f64, f64::max)
        };
        let mut out = 0.0_f32;
        if s.kind != OFF {
            let sway = 1.5
                * s.depth
                    .max(s.depth_setting * (0.5 * s.rate as f32) * s.lfo_scale)
                    as f64;
            let echo = age(1).max(if s.kind == MONO { age(2) } else { 0.0 }) + sway;
            for r in &self.network.lines {
                out = out.max(r.activity(echo));
            }
            if s.kind == TRUE_STEREO && (s.input != 0.0 || s.target[INPUT] != 0.0) {
                for (i, r) in self.network.inputs.iter().enumerate() {
                    out = out.max(r.activity(age(i + 2)));
                }
            }
            for r in self.network.allpass.iter().flatten() {
                out = out.max(r.activity(age(0)));
            }
            for p in &s.hp {
                out = out.max(p.x1.abs()).max(p.y1.abs());
            }
        }
        if !s.svf_k.off {
            for v in &s.svf {
                out = out.max(v.c1.abs()).max(v.c2.abs());
            }
        }
        if s.colour_hz.is_some() {
            for p in &s.tone {
                out = out.max(p.x1.abs()).max(p.y1.abs());
            }
        }
        if s.rate_amount > 0.0 {
            for v in &s.shaper {
                out = out.max(v.held.abs());
            }
        }
        if s.kind != OFF && s.feedback.max(s.target[FEEDBACK] * 1.25) >= 1.0 && out > 0.0 {
            out = out.max(0.0001);
        }
        out * 32.0
    }
    fn bytes(&self) -> usize {
        self.network
            .lines
            .iter()
            .chain(self.network.inputs.iter())
            .chain(self.network.allpass.iter().flatten())
            .map(Ring::bytes)
            .sum()
    }
    fn can_retarget(&self, previous: &dyn Processor) -> bool {
        (previous as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|old| {
                self.initial.rate == old.initial.rate
                    && [TYPE, FILTER, DIST_TYPE]
                        .iter()
                        .all(|&i| self.settings.v[i] == old.settings.v[i])
            })
    }
    fn retarget(&mut self, previous: &mut dyn Processor) {
        let old = (previous as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        std::mem::swap(&mut self.network, &mut old.network);
        self.excited = old.excited;
        self.network.set(&self.settings);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob_effect_reference::delay as reference;
    fn instance(v: [i32; 26]) -> Instance {
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::Delay,
            bypassed: false,
            parameters: IDS
                .into_iter()
                .enumerate()
                .map(|(i, id)| {
                    (
                        id.into(),
                        if matches!(i, 2 | 3) {
                            ParameterValue::Boolean(v[i] != 0)
                        } else {
                            ParameterValue::Integer(v[i])
                        },
                    )
                })
                .collect(),
        }
    }
    fn compare(v: [i32; 26], rate: u32, bpm: f64, frames: usize, label: &str) {
        let mut actual = Delay::new(&instance(v), rate, bpm).unwrap();
        let mut expected = reference::Delay::with_settings(rate, &reference::Settings { v }, bpm);
        expected.start();
        let mut max = 0.0_f32;
        let mut error = 0.0;
        let mut energy = 0.0;
        for i in 0..frames {
            let t = i as f64 / rate as f64;
            let input = if i == 0 {
                [0.8, -0.3]
            } else if i < rate as usize / 8 {
                [
                    (t * 731.0).sin() as f32 * 0.4 + (t * 1937.0).sin() as f32 * 0.15,
                    (t * 1231.0).cos() as f32 * 0.32,
                ]
            } else {
                [0.0; 2]
            };
            let a = actual.frame(input);
            let (l, r) = expected.frame(input[0], input[1]);
            for (a, e) in a.into_iter().zip([l, r]) {
                assert!(a.is_finite());
                let d = (a - e).abs();
                max = max.max(d);
                error += (d as f64).powi(2);
                energy += (e as f64).powi(2);
            }
        }
        let rms = if energy > 1e-12 {
            (error / energy).sqrt()
        } else {
            0.0
        };
        eprintln!("{label} {rate} Hz: max {max:e}; relative RMS {rms:e}");
        assert!(max <= 2e-6 && rms <= 2e-5, "{label}: {max:e}/{rms:e}");
    }
    #[test]
    fn development_native_reference_models_controls_and_rates() {
        for rate in [44100, 48000, 96000] {
            for model in 0..4 {
                let mut v = DEFAULTS;
                v[TYPE] = model;
                v[SYNC] = 0;
                v[TIME] = 160;
                v[DRY] = 0;
                v[FEEDBACK] = 3900;
                compare(v, rate, 120.0, rate as usize, "model");
                for changes in [
                    vec![(OFFSET, 0), (SPREAD, 1800)],
                    vec![(OFFSET, 6000), (KEEP_PITCH, 1)],
                    vec![(FILTER, HIGH_PASS), (CUTOFF, 2200), (RESONANCE, 4500)],
                    vec![(FILTER, BAND_PASS), (CUTOFF, 3800), (RESONANCE, 6000)],
                    vec![(FILTER, FILTER_OFF), (DIST_TYPE, 0), (DRIVE, 5000)],
                    vec![(DRIVE, 5200), (HARDNESS, 4000), (SYMMETRY, 2000)],
                    vec![(RATE_RED, 15000), (BITS, 1800)],
                    vec![(MOD_RATE, 5500), (MOD_TIME, 5600), (MOD_CUTOFF, 4200)],
                    vec![(DIFF_LEVEL, 5500), (DIFF_SPREAD, 6000), (TONE, -2700)],
                    vec![(TONE, 2400), (INPUT, 4200), (WET, 3300), (DRY, 6000)],
                ] {
                    let mut p = v;
                    for (i, x) in changes {
                        p[i] = x;
                    }
                    compare(p, rate, 120.0, rate as usize, "controls");
                }
            }
        }
    }
    #[test]
    fn native_reference_control_minima_and_maxima() {
        for (index, (lo, hi)) in RANGES.into_iter().enumerate() {
            for value in [lo, hi] {
                let mut v = DEFAULTS;
                v[SYNC] = 0;
                v[TIME] = 29;
                v[DRY] = 0;
                v[index] = value;
                compare(v, 48000, 120.0, 24000, &format!("extreme-{index}-{value}"));
            }
        }
    }
    #[test]
    fn held_out_native_reference_mixed_controls() {
        let v = [
            4200, 2, 1, 1, 17, 900, 730, 4800, 1571, 4600, 1700, 3710, 2910, 1, 3300, 2, 3940,
            2130, 4200, 2300, 800, 42000, 4920, 5000, -1777, 1900,
        ];
        compare(v, 96000, 137.5, 288000, "held-out");
    }
    #[test]
    fn native_reference_twelve_second_echo_and_deterministic_reset() {
        let mut v = DEFAULTS;
        v[TIME] = 768;
        v[DRY] = 0;
        v[FEEDBACK] = 0;
        v[SMOOTH] = 0;
        compare(v, 48000, 20.0, 600000, "12 second intentional echo");
        let mut a = Delay::new(&instance(v), 48000, 20.0).unwrap();
        let initial = a.initial;
        for _ in 0..913 {
            a.frame([0.5, -0.2]);
        }
        a.reset();
        assert_eq!(a.network.state.frame, initial.frame);
        assert_eq!(a.network.state.phase, initial.phase);
        assert_eq!(a.activity(), 0.0);
        assert!(a.stereo());
        assert_eq!(a.latency(), 0);
        assert!(a.bytes() < 64 * 1024 * 1024);
        let mut fresh = Delay::new(&instance(v), 48000, 20.0).unwrap();
        for i in 0..600000 {
            let input = if i == 0 { [0.8, -0.3] } else { [0.0; 2] };
            assert_eq!(a.frame(input), fresh.frame(input));
        }
    }
    #[test]
    fn storage_budget_and_compatible_changes_preserve_echoes() {
        let mut old = Delay::new(&instance(DEFAULTS), 192000, 20.0).unwrap();
        assert!(old.bytes() < 64 * 1024 * 1024);
        assert!(Delay::new(&instance(DEFAULTS), 1000000, 20.0).is_err());
        old.frame([1.0, -0.5]);
        let position = old.network.lines[0].pos;
        let mut v = DEFAULTS;
        v[WET] = 3000;
        v[TIME] = 768;
        v[KEEP_PITCH] = 1;
        let mut new = Delay::new(&instance(v), 192000, 20.0).unwrap();
        assert!(new.can_retarget(&old));
        new.retarget(&mut old);
        assert_eq!(new.network.lines[0].pos, position);
        assert!(new.excited);
        assert!(new.activity() > 0.0);
        v[TYPE] = 0;
        assert!(
            !Delay::new(&instance(v), 192000, 20.0)
                .unwrap()
                .can_retarget(&new)
        );
    }
    #[test]
    fn source_stepped_smoothing_glide_and_keep_pitch_survive_compatible_updates() {
        let mut v = DEFAULTS;
        v[SYNC] = 0;
        v[TIME] = 37;
        v[MOD_TIME] = 4000;
        let mut a = Delay::new(&instance(v), 48000, 120.0).unwrap();
        let mut r = reference::Delay::with_settings(48000, &reference::Settings { v }, 120.0);
        r.start();
        for (index, value) in [
            (WET, 1300),
            (FEEDBACK, 5300),
            (TIME, 700),
            (KEEP_PITCH, 1),
            (TIME, 19),
            (OFFSET, 6000),
            (DIFF_SPREAD, 6000),
            (SYNC, 1),
            (TIME, 31),
            (KEEP_PITCH, 0),
        ] {
            v[index] = value;
            let mut next = Delay::new(&instance(v), 48000, 120.0).unwrap();
            assert!(next.can_retarget(&a));
            next.retarget(&mut a);
            a = next;
            r.set(&reference::Settings { v });
            for i in 0..2048 {
                let input = [(i as f32 * 0.07).sin() * 0.8, (i as f32 * 0.13).cos() * 0.3];
                let actual = a.frame(input);
                let (l, rr) = r.frame(input[0], input[1]);
                for (x, y) in actual.into_iter().zip([l, rr]) {
                    assert!((x - y).abs() <= 2e-6, "update {index}/{value}: {x} != {y}");
                }
            }
        }
    }
    #[test]
    fn out_of_band_colour_corners_are_finite_at_low_sample_rates() {
        for rate in [4000, 8000, 16000] {
            let mut v = DEFAULTS;
            v[TYPE] = OFF;
            v[TONE] = -1;
            v[RESONANCE] = 6000;
            let mut a = Delay::new(&instance(v), rate, 120.0).unwrap();
            for i in 0..32000 {
                assert!(
                    a.frame(if i < 100 { [1.0, -1.0] } else { [0.0; 2] })
                        .into_iter()
                        .all(f32::is_finite)
                );
                assert!(a.activity().is_finite());
            }
        }
    }
}
#[cfg(test)]
mod retained_captures {
    use super::*;
    use sha2::{Digest, Sha256};
    fn pcm(path: &std::path::Path) -> Vec<f32> {
        use std::io::Read;
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    }
    #[test]
    fn original_source_hash_and_retained_captures_agree_natively() {
        let source = include_bytes!("../../tools/effect-reference/pinned/delay.rs");
        assert_eq!(
            format!("{:x}", Sha256::digest(source)),
            "6abba71c6613e039408d5464629d4b44b6f1834f6b10369785785cc5b916ecfa"
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/processing/delay");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        for c in manifest["cases"].as_array().unwrap() {
            let rate = c["sample_rate"].as_u64().unwrap() as u32;
            let bpm = c["bpm"].as_f64().unwrap();
            let v = std::array::from_fn(|i| c["values"][i].as_i64().unwrap() as i32);
            let instance = Instance {
                id: uuid::Uuid::new_v4().to_string(),
                kind: EffectKind::Delay,
                bypassed: false,
                parameters: IDS
                    .into_iter()
                    .enumerate()
                    .map(|(i, id)| {
                        (
                            id.into(),
                            if matches!(i, 2 | 3) {
                                ParameterValue::Boolean(v[i] != 0)
                            } else {
                                ParameterValue::Integer(v[i])
                            },
                        )
                    })
                    .collect(),
            };
            let input = pcm(&root.join(c["input"].as_str().unwrap()));
            let capture = pcm(&root.join(c["output"].as_str().unwrap()));
            assert_eq!(input.len(), capture.len());
            let mut actual = Delay::new(&instance, rate, bpm).unwrap();
            let mut reference = wavebob_effect_reference::delay::Delay::with_settings(
                rate,
                &wavebob_effect_reference::delay::Settings { v },
                bpm,
            );
            reference.start();
            for (i, frame) in input.as_chunks::<2>().0.iter().enumerate() {
                let a = actual.frame([frame[0], frame[1]]);
                let (l, r) = reference.frame(frame[0], frame[1]);
                for (channel, e) in [l, r].into_iter().enumerate() {
                    assert!((a[channel] - e).abs() <= 2e-6, "{} frame{i}", c["name"]);
                    if cfg!(windows) {
                        assert!(
                            (a[channel] - capture[i * 2 + channel]).abs() <= 2e-6,
                            "{} retained frame{i}",
                            c["name"]
                        );
                    }
                }
            }
        }
    }
}
