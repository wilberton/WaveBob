// Copyright (C) 2026 ItsAlphaNeon (reference algorithm)
// SPDX-License-Identifier: GPL-3.0-or-later
//! WaveBob's plate network, following Lemon-Studio
//! 0af860dfd31c7ee9cbf8af6747617e031a1a7742, effects/reverb.rs.
//! Host adaptations: fallible preparation, bounded state accounting, deterministic
//! resets, state-preserving 10 ms coefficient updates and prepared room changes.
use crate::{
    CommandError, ErrorCode,
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
use std::f64::consts::PI;

pub const IDS: [&str; 15] = [
    "low_cut",
    "high_cut",
    "predelay",
    "size",
    "diffusion",
    "decay",
    "damping",
    "bass",
    "crossover",
    "separation",
    "dry",
    "early",
    "wet",
    "mod_speed",
    "mod_depth",
];
pub const DEFAULTS: [i32; 15] = [
    75, 40, 0, 50, 100, 15, 40, 100, 500, 0, 128, 128, 128, 684, 0,
];
pub const RANGES: [(i32, i32); 15] = [
    (19, 3000),
    (5, 221),
    (0, 384),
    (1, 100),
    (0, 100),
    (1, 200),
    (5, 221),
    (20, 300),
    (25, 2000),
    (-64, 64),
    (0, 160),
    (0, 160),
    (0, 160),
    (0, 2048),
    (0, 2048),
];
const NAMES: [&str; 15] = [
    "Low cut",
    "High cut",
    "Predelay",
    "Room size",
    "Diffusion",
    "Decay",
    "High damping",
    "Bass multiplier",
    "Crossover",
    "Stereo separation",
    "Dry level",
    "Early reflections",
    "Wet level",
    "Modulation speed",
    "Modulation depth",
];
const UNITS: [&str; 15] = [
    "Hz (19 = Off)",
    "100 Hz steps (221 = Off)",
    "1/384 s or 1/192 beat",
    "raw",
    "%",
    "0.1 s",
    "100 Hz steps (221 = Off)",
    "%",
    "Hz",
    "raw (±64 = ±100%)",
    "raw (128 = 100%)",
    "raw (128 = 100%)",
    "raw (128 = 100%)",
    "raw (2048 = 100%)",
    "raw (2048 = 100%)",
];
pub fn info() -> EffectInfo {
    let mut parameters: Vec<_> = (0..15)
        .map(|i| ParameterInfo {
            id: IDS[i].into(),
            name: NAMES[i].into(),
            unit: UNITS[i].into(),
            value_type: "integer".into(),
            default: ParameterValue::Integer(DEFAULTS[i]),
            minimum: RANGES[i].0 as f64,
            maximum: RANGES[i].1 as f64,
        })
        .collect();
    for (id, name) in [
        ("tempo_sync", "Tempo-based predelay"),
        ("mid_side", "Mid / side input"),
    ] {
        parameters.push(ParameterInfo {
            id: id.into(),
            name: name.into(),
            unit: "toggle".into(),
            value_type: "boolean".into(),
            default: ParameterValue::Boolean(false),
            minimum: 0.0,
            maximum: 1.0,
        });
    }
    EffectInfo {
        kind: EffectKind::Reverb,
        available: true,
        parameters,
    }
}
pub fn value_text(index: usize, value: i32, sync: bool) -> String {
    match index {
        0 if value == 19 => "Off".into(),
        0 | 8 => format!("{value} Hz"),
        1 | 6 if value == 221 => "Off".into(),
        1 | 6 => format!("{:.1} kHz", value as f64 / 10.0),
        2 if sync => format!("{}:{:02}", value / 48, value % 48),
        2 => format!("{} ms", value as i64 * 1000 / 384),
        5 => format!("{:.1} s", value as f64 / 10.0),
        7 => format!("{value}%"),
        9 if value == 0 => "Original".into(),
        9 => format!(
            "{:.0}% {}",
            (value.abs() as f64 * 100.0 / 64.0).round_ties_even(),
            if value < 0 { "separated" } else { "merged" }
        ),
        10..=12 => format!("{:.0}%", (value as f64 * 100.0 / 128.0).round()),
        13..=14 => format!("{:.0}%", (value as f64 * 100.0 / 2048.0).round()),
        _ => value.to_string(),
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Settings {
    knobs: [i32; 15],
    sync: bool,
    side: bool,
}
impl Settings {
    fn read(instance: &Instance) -> Self {
        Self {
            knobs: std::array::from_fn(|i| match instance.parameters[IDS[i]] {
                ParameterValue::Integer(v) => v,
                _ => unreachable!("Validated Reverb parameter"),
            }),
            sync: matches!(
                instance.parameters["tempo_sync"],
                ParameterValue::Boolean(true)
            ),
            side: matches!(
                instance.parameters["mid_side"],
                ParameterValue::Boolean(true)
            ),
        }
    }
}
#[derive(Clone)]
struct Geometry {
    lengths: [usize; 12],
    taps: [[(usize, usize); 7]; 2],
    early: (usize, usize, f32, f32),
}
impl Geometry {
    fn new(room: i32, rate: u32) -> Self {
        const LENGTHS: [i32; 12] = [
            142, 107, 379, 277, 672, 4453, 1800, 3720, 908, 4217, 2656, 3163,
        ];
        const TAPS: [[(usize, i32); 7]; 2] = [
            [
                (9, 266),
                (9, 2974),
                (10, 1913),
                (11, 2496),
                (5, 1690),
                (6, 187),
                (7, 1566),
            ],
            [
                (5, 3627),
                (5, 353),
                (6, 1728),
                (7, 3173),
                (9, 1811),
                (10, 335),
                (11, 621),
            ],
        ];
        let scale = (room + 15) as i64 * rate as i64 / 115;
        let offset = |n: i32| (scale * n as i64 / 20000).max(0) as usize;
        let lengths = LENGTHS.map(|n| offset(n).max(1));
        let taps = TAPS.map(|side| side.map(|(line, n)| (line, offset(n))));
        let e = (rate as f64 * (room + 3) as f64 * 3.3022917905026087e-7) as f32;
        let left = (1102.0 * e).round_ties_even().max(1.0) as usize;
        let right = (837.0 * e).round_ties_even().max(1.0) as usize;
        let shorter = left.min(right) as f64;
        Self {
            lengths,
            taps,
            early: (
                left,
                right,
                (shorter / left as f64 * 0.5) as f32,
                (shorter / right as f64 * 0.36) as f32,
            ),
        }
    }
}
fn lowpass(raw: i32, rate: u32) -> f32 {
    if raw == 221 {
        return 1.0;
    }
    let q = (1.0 / rate as f64) as f32;
    let x = (2.0 - (raw as f64 * 628.3185307179587 * q as f64).cos()) as f32;
    let square = (x as f64 * x as f64) as f32;
    (1.0 - (x as f64 - (square as f64 - 1.0).sqrt())) as f32
}
fn output_matrix(value: f32, separation: i32, side: bool) -> [f32; 4] {
    let sep = separation as f32 / 64.0;
    let mut matrix = if sep == 0.0 {
        [value, value, 0.0, 0.0]
    } else if value == 0.0 {
        [0.0; 4]
    } else {
        let scale = value * (1.0 / (PI / 4.0).cos()) as f32;
        let l = (scale as f64 * (PI / 4.0).cos()) as f32;
        let r = (scale as f64 * (PI / 4.0).sin()) as f32;
        let merge = if sep < 0.0 {
            sep
        } else {
            ((sep as f64 * 5.0_f64.ln()).exp() - 1.0) as f32 * 0.25
        }
        .min(1.0);
        [
            l * (1.0 - merge * 0.5),
            r * (1.0 - merge * 0.5),
            l * merge * 0.5,
            r * merge * 0.5,
        ]
    };
    if side {
        matrix[1] = -matrix[1];
        matrix[3] = -matrix[3];
    }
    matrix
}
#[derive(Clone, Copy)]
struct Coefficients {
    values: [f32; 21],
    predelay: usize,
    side: bool,
}
impl Coefficients {
    fn new(settings: Settings, g: &Geometry, rate: u32, bpm: f64) -> Self {
        let k = settings.knobs;
        let q = (1.0 / rate as f64) as f32;
        let diffusion = (k[4] as f64 * 0.01) as f32;
        let diffuser = |n: usize, spread: f64| {
            if diffusion == 0.0 {
                0.0
            } else {
                let length = (n as f64 * (q * 0.5) as f64) as f32;
                (-6.907755278982137 * length as f64 / (diffusion as f64 * spread)).exp() as f32
            }
        };
        let circuit = (g.lengths[5] + g.lengths[7] + g.lengths[9] + g.lengths[11]) as f32;
        let time = (k[5] as i64 * rate as i64) as f32 * 0.1;
        let quarter = |time: f32| {
            (0.25
                * (-6.907755278982137 * circuit as f64 / time as f64)
                    .exp()
                    .ln())
            .exp() as f32
        };
        let decay = quarter(time);
        let bass = quarter(time * (k[7] as f32 * 0.01)) / decay;
        let shelf_gain = ((bass as f64 - 1.0) * 0.5) as f32;
        let crossover = (k[8] as f64 * PI * q as f64).tan() as f32;
        let shelf = (crossover - bass.min(1.0)) / (crossover + bass.min(1.0));
        let low = if k[0] == 19 {
            0.0
        } else {
            let t = (k[0] as f64 * PI * q as f64).tan() as f32;
            ((t as f64 - 1.0) / (t as f64 + 1.0)) as f32
        };
        let speed = k[13] as f32 / 2048.0;
        let delay = if settings.sync {
            (k[2] as f64 * rate as f64 * 0.3125 / (bpm as f32) as f64) as f32
        } else {
            (k[2] as i64 * rate as i64) as f32 / 384.0
        };
        let slider = |raw: i32, denominator: f32| {
            let x = raw as f32 / denominator;
            if x == 1.0 {
                1.0
            } else {
                ((((x * 11.0_f64.ln() as f32) as f64).exp() - 1.0) * 0.1_f32 as f64) as f32
            }
        };
        let early = output_matrix(slider(k[11], 256.0), k[9], settings.side);
        let wet = output_matrix(slider(k[12], 256.0), k[9], settings.side);
        let mut values = [0.0; 21];
        values[..13].copy_from_slice(&[
            lowpass(k[1], rate),
            low,
            lowpass(k[6], rate),
            diffuser(g.lengths[0] + g.lengths[1], 0.07),
            diffuser(g.lengths[4] + g.lengths[8], 0.35),
            diffuser(g.lengths[6] + g.lengths[10], 0.51),
            decay,
            shelf,
            shelf_gain,
            (speed * speed * speed * 9.9 + 0.1) / rate as f32,
            (k[14] as f32 / 2048.0) * 0.004_535_147_5 * rate as f32,
            1.0 - 0.01_f64.powf((1.0 / (rate as f32 * 0.1)) as f64) as f32,
            slider(k[10], 128.0),
        ]);
        values[13..17].copy_from_slice(&early);
        values[17..21].copy_from_slice(&wet);
        Self {
            values,
            predelay: delay.round_ties_even().max(1.0) as usize,
            side: settings.side,
        }
    }
}
struct Buffer {
    samples: Vec<f32>,
    head: usize,
    energy: f64,
}
impl Buffer {
    fn new(length: usize) -> Result<Self, CommandError> {
        let mut samples = Vec::new();
        samples.try_reserve_exact(length.max(1)).map_err(|_| {
            CommandError::new(
                ErrorCode::ResourceLimit,
                "Cannot allocate Reverb delay storage",
            )
        })?;
        samples.resize(length.max(1), 0.0);
        Ok(Self {
            samples,
            head: 0,
            energy: 0.0,
        })
    }
    fn out(&self) -> f32 {
        self.samples[self.head]
    }
    fn past(&self, n: usize) -> f32 {
        self.samples[(self.head + self.samples.len() - n % self.samples.len()) % self.samples.len()]
    }
    fn push(&mut self, value: f32) {
        let old = self.samples[self.head] as f64;
        self.energy = (self.energy + value as f64 * value as f64 - old * old).max(0.0);
        self.samples[self.head] = value;
        self.head = (self.head + 1) % self.samples.len();
    }
    fn delay(&mut self, input: f32) -> f32 {
        let out = self.out();
        self.push(input);
        out
    }
    fn allpass(&mut self, input: f32, coefficient: f32) -> f32 {
        let out = self.out() - coefficient * input;
        self.push(coefficient * out + input);
        out
    }
    fn reset(&mut self) {
        self.samples.fill(0.0);
        self.head = 0;
        self.energy = 0.0;
    }
    fn bytes(&self) -> usize {
        self.samples.capacity() * 4
    }
}
#[derive(Clone, Copy, Default)]
struct Half {
    previous: f32,
    shelf: f32,
    damping: f32,
}
fn tank_input(
    state: &mut Half,
    input: f32,
    pre: f32,
    c: &[f32; 21],
    history: &mut Buffer,
    offset: usize,
    fraction: f32,
) -> f32 {
    let shelf = (input - state.shelf) * c[7] + state.previous;
    state.previous = input;
    state.shelf = shelf;
    let filtered = (((shelf + input) * c[8] + input + pre) - state.damping) * c[2] + state.damping;
    state.damping = filtered;
    history.push(filtered);
    fraction * history.past(offset + 2) + (1.0 - fraction) * history.past(offset + 1)
}
pub(crate) struct Reverb {
    rate: u32,
    settings: Settings,
    geometry: Geometry,
    current: Coefficients,
    target: Coefficients,
    remaining: u32,
    lines: Vec<Buffer>,
    early: Buffer,
    pre: Buffer,
    modulation: [Buffer; 2],
    halves: [Half; 2],
    high_state: f32,
    low_input: f32,
    low_state: f32,
    phase: f32,
    depth: f32,
    excited: bool,
}
impl Reverb {
    pub fn new(instance: &Instance, rate: u32, tempo: f64) -> Result<Self, CommandError> {
        let settings = Settings::read(instance);
        let geometry = Geometry::new(settings.knobs[3], rate);
        let coefficients = Coefficients::new(settings, &geometry, rate, tempo);
        let lines = geometry
            .lengths
            .iter()
            .map(|n| Buffer::new(*n))
            .collect::<Result<Vec<_>, _>>()?;
        let early = Buffer::new(geometry.early.0.max(geometry.early.1) + 2)?;
        let pre = Buffer::new((rate as usize * 2 + 2).max(coefficients.predelay + 1))?;
        let modulation_length = (0.004_535_147_5 * rate as f64).ceil() as usize + 4;
        Ok(Self {
            rate,
            settings,
            geometry,
            current: coefficients,
            target: coefficients,
            remaining: 0,
            lines,
            early,
            pre,
            modulation: [
                Buffer::new(modulation_length)?,
                Buffer::new(modulation_length)?,
            ],
            halves: [Half::default(); 2],
            high_state: 0.0,
            low_input: 0.0,
            low_state: 0.0,
            phase: 0.0,
            depth: 0.0,
            excited: false,
        })
    }
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.remaining > 0 {
            for (current, target) in self.current.values.iter_mut().zip(self.target.values) {
                *current += (target - *current) / self.remaining as f32;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            }
        }
        self.excited |= input[0] != 0.0 || input[1] != 0.0;
        let c = self.current.values;
        let sine = (self.phase as f64 * 2.0 * PI).sin() as f32;
        self.phase += c[9];
        self.phase -= self.phase.trunc();
        self.depth = if (c[10] - self.depth).abs() < 0.01 {
            c[10]
        } else {
            (c[10] - self.depth) * c[11] + self.depth + 1e-30
        };
        let sweep = self.depth * ((sine + 1.0) * 0.5);
        let whole = sweep.trunc() as usize;
        let fraction = sweep - sweep.trunc();
        let (left, right, lg, rg) = self.geometry.early;
        let reflections = [self.early.past(left) * lg, self.early.past(right) * rg];
        let pre = self.pre.past(self.current.predelay) + 1e-20;
        let feedback = self.lines[11].out() * c[6];
        let value = tank_input(
            &mut self.halves[0],
            feedback,
            pre,
            &c,
            &mut self.modulation[0],
            whole,
            fraction,
        );
        let value = self.lines[4].allpass(value, c[4]);
        let value = self.lines[5].delay(value) * c[6];
        let value = self.lines[6].allpass(value, c[5]);
        self.lines[7].push(value);
        let feedback = self.lines[7].past(0) * c[6];
        let value = tank_input(
            &mut self.halves[1],
            feedback,
            pre,
            &c,
            &mut self.modulation[1],
            whole,
            fraction,
        );
        let value = self.lines[8].allpass(value, c[4]);
        let value = self.lines[9].delay(value) * c[6];
        let value = self.lines[10].allpass(value, c[5]);
        self.lines[11].push(value);
        let tap = |side: usize, index: usize| {
            let (line, offset) = self.geometry.taps[side][index];
            self.lines[line].past(offset)
        };
        let inverse = 1.0 - c[6];
        let a = inverse * 0.6 + c[6];
        let b = inverse * 0.25 + c[6];
        let d = inverse * 0.7 + c[6];
        let e = inverse * 0.2 + c[6];
        let f = inverse * 0.5 + c[6];
        let wet = [
            (((tap(0, 0) - b * tap(0, 1)) - d * tap(0, 2)) - e * tap(0, 3))
                + a * tap(0, 4)
                + tap(0, 5)
                + f * tap(0, 6),
            ((((b * tap(1, 0) + tap(1, 1)) - d * tap(1, 2)) - e * tap(1, 3)) - a * tap(1, 4))
                + tap(1, 5)
                + f * tap(1, 6),
        ];
        let mono = if self.current.side {
            input[0] - input[1]
        } else {
            input[0] + input[1]
        };
        let high = ((mono + 1e-30) - self.high_state) * c[0] + self.high_state;
        self.high_state = high;
        let low = (high - self.low_state) * c[1] + self.low_input;
        self.low_input = high;
        self.low_state = low;
        let value = if c[1] != 0.0 {
            (high - low) * 0.5 + 1e-30
        } else {
            high
        };
        let mut value = self.lines[0].allpass(value, c[3]);
        self.early.push(value);
        for line in &mut self.lines[1..4] {
            value = line.allpass(value, c[3]);
        }
        self.pre.push(value);
        let mut out = if c[12] == 0.0 {
            [0.0; 2]
        } else if c[12] == 1.0 {
            input
        } else {
            [input[0] * c[12], input[1] * c[12]]
        };
        for (signal, matrix) in [(reflections, &c[13..17]), (wet, &c[17..21])] {
            if matrix[2] == 0.0 && matrix[3] == 0.0 {
                if matrix[0] != 0.0 || matrix[1] != 0.0 {
                    out[0] += signal[0] * matrix[0];
                    out[1] += signal[1] * matrix[1];
                }
            } else {
                out[0] += signal[0] * matrix[0] + signal[1] * matrix[2];
                out[1] += signal[1] * matrix[1] + signal[0] * matrix[3];
            }
        }
        if self.excited { out } else { [0.0; 2] }
    }
}
impl Processor for Reverb {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
    fn stereo(&self) -> bool {
        true
    }
    fn has_tail(&self) -> bool {
        self.current.values[13..]
            .iter()
            .chain(&self.target.values[13..])
            .any(|v| *v != 0.0)
    }
    fn activity(&self) -> f32 {
        if !self.excited || !self.has_tail() {
            return 0.0;
        }
        let audible = |range: std::ops::Range<usize>| {
            self.current.values[range.clone()]
                .iter()
                .chain(&self.target.values[range])
                .any(|v| *v != 0.0)
        };
        let wet_energy = if audible(17..21) {
            self.lines.iter().map(|line| line.energy).sum::<f64>()
                + self.pre.energy
                + self.modulation.iter().map(|line| line.energy).sum::<f64>()
                + self
                    .halves
                    .iter()
                    .map(|half| {
                        [half.previous, half.shelf, half.damping]
                            .into_iter()
                            .map(|v| (v as f64).powi(2))
                            .sum::<f64>()
                    })
                    .sum::<f64>()
        } else {
            0.0
        };
        let early_energy = if audible(13..17) {
            self.lines[0].energy + self.early.energy
        } else {
            0.0
        };
        let energy = wet_energy
            + early_energy
            + [self.high_state, self.low_input, self.low_state]
                .into_iter()
                .map(|v| (v as f64).powi(2))
                .sum::<f64>();
        (energy.sqrt() * 8.0) as f32
    }
    fn bytes(&self) -> usize {
        self.lines.iter().map(Buffer::bytes).sum::<usize>()
            + self.early.bytes()
            + self.pre.bytes()
            + self.modulation.iter().map(Buffer::bytes).sum::<usize>()
    }
    fn reset(&mut self) {
        for line in &mut self.lines {
            line.reset();
        }
        self.early.reset();
        self.pre.reset();
        for line in &mut self.modulation {
            line.reset();
        }
        self.halves = [Half::default(); 2];
        self.high_state = 0.0;
        self.low_input = 0.0;
        self.low_state = 0.0;
        self.phase = 0.0;
        self.depth = 0.0;
        self.excited = false;
        self.current = self.target;
        self.remaining = 0;
    }
    fn can_retarget(&self, previous: &dyn Processor) -> bool {
        (previous as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|old| {
                self.rate == old.rate
                    && self.settings.knobs[3] == old.settings.knobs[3]
                    && self.current.predelay == old.current.predelay
                    && self.current.side == old.current.side
            })
    }
    fn retarget(&mut self, previous: &mut dyn Processor) {
        let old = (previous as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        std::mem::swap(&mut self.lines, &mut old.lines);
        std::mem::swap(&mut self.early, &mut old.early);
        std::mem::swap(&mut self.pre, &mut old.pre);
        std::mem::swap(&mut self.modulation, &mut old.modulation);
        self.halves = old.halves;
        self.high_state = old.high_state;
        self.low_input = old.low_input;
        self.low_state = old.low_state;
        self.phase = old.phase;
        self.depth = old.depth;
        self.excited = old.excited;
        self.current = old.current;
        self.remaining = (self.rate / 100).max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{io::Read, path::Path};

    fn instance() -> Instance {
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::Reverb,
            bypassed: false,
            parameters: info()
                .parameters
                .into_iter()
                .map(|p| (p.id, p.default))
                .collect(),
        }
    }
    #[test]
    fn reset_repeats_modulation_and_continuous_updates_retain_the_tail() {
        let mut settings = instance();
        settings
            .parameters
            .insert("mod_depth".into(), ParameterValue::Integer(2048));
        let mut first = Reverb::new(&settings, 8000, 120.0).unwrap();
        let expected: Vec<_> = (0..24000)
            .map(|n| first.frame(if n == 0 { [1.0, -0.2] } else { [0.0; 2] }))
            .collect();
        first.reset();
        for (n, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                first.frame(if n == 0 { [1.0, -0.2] } else { [0.0; 2] }),
                expected
            );
        }
        first.reset();
        for n in 0..4000 {
            first.frame(if n == 0 { [1.0, 0.0] } else { [0.0; 2] });
        }
        assert!(first.activity() > 0.0);
        settings
            .parameters
            .insert("wet".into(), ParameterValue::Integer(160));
        let mut next = Reverb::new(&settings, 8000, 120.0).unwrap();
        assert!(next.can_retarget(&first));
        next.retarget(&mut first);
        assert!(next.activity() > 0.0);
        assert!(
            (0..1000)
                .flat_map(|_| next.frame([0.0; 2]))
                .any(|v| v.abs() > 1e-5)
        );
        settings
            .parameters
            .insert("size".into(), ParameterValue::Integer(100));
        assert!(
            !Reverb::new(&settings, 8000, 120.0)
                .unwrap()
                .can_retarget(&next)
        );
    }
    #[test]
    fn mono_promotion_natural_tail_predelay_and_cap_follow_the_shared_policy() {
        use crate::{
            documents::FrameRange,
            processing::{Action, Processing},
        };
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let source = crate::audio::prepare(
            8000,
            1,
            "float32".into(),
            Default::default(),
            vec![1.0],
            &cancel,
            |_| {},
        )
        .unwrap();
        let mut state = Processing::default();
        state
            .apply(
                Action::Add {
                    kind: EffectKind::Reverb,
                },
                1,
            )
            .unwrap();
        let render = |state: &Processing, tempo| {
            crate::rendering::offline(
                &source,
                FrameRange { start: 0, end: 1 },
                state,
                tempo,
                &cancel,
            )
            .unwrap()
        };
        let (audio, metadata) = render(&state, 120.0);
        assert_eq!(metadata.channels, 2);
        assert_eq!(metadata.latency_frames, 0);
        assert!(metadata.tail_frames > 800 && metadata.tail_frames < 8000 * 60);
        assert!(!metadata.tail_capped);
        assert!(audio.samples[2..].iter().any(|v| v.abs() > 1e-4));
        state.effects[0]
            .parameters
            .insert("wet".into(), ParameterValue::Integer(0));
        state.effects[0]
            .parameters
            .insert("decay".into(), ParameterValue::Integer(200));
        let (_, early_only) = render(&state, 120.0);
        assert!(
            early_only.tail_frames < 8000,
            "An inaudible tank must not extend early-only output"
        );
        state.effects[0]
            .parameters
            .insert("wet".into(), ParameterValue::Integer(128));
        state.effects[0]
            .parameters
            .insert("decay".into(), ParameterValue::Integer(15));
        for (id, value) in [
            ("dry", ParameterValue::Integer(0)),
            ("early", ParameterValue::Integer(0)),
            ("predelay", ParameterValue::Integer(384)),
            ("tempo_sync", ParameterValue::Boolean(true)),
        ] {
            state.effects[0].parameters.insert(id.into(), value);
        }
        let (audio, metadata) = render(&state, 20.0);
        assert!(metadata.tail_frames > 48000);
        assert!(audio.samples[..48000 * 2].iter().all(|v| v.abs() < 1e-15));
        assert!(audio.samples[48000 * 2..].iter().any(|v| v.abs() > 1e-5));
        for (id, value) in [
            ("decay", 200),
            ("bass", 300),
            ("size", 100),
            ("predelay", 0),
            ("low_cut", 19),
            ("high_cut", 221),
            ("damping", 221),
        ] {
            state.effects[0]
                .parameters
                .insert(id.into(), ParameterValue::Integer(value));
        }
        let (audio, metadata) = render(&state, 120.0);
        assert!(metadata.tail_capped);
        assert_eq!(metadata.tail_frames, 8000 * 60);
        assert!(audio.samples.iter().all(|v| v.is_finite()));
        assert_eq!(&audio.samples[audio.samples.len() - 2..], &[0.0; 2]);
        state.effects[0]
            .parameters
            .insert("wet".into(), ParameterValue::Integer(0));
        let (_, metadata) = render(&state, 120.0);
        assert_eq!(metadata.tail_frames, 0);
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/processing/reverb");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        for case in manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| (c["role"] == "held-out") == held_out)
        {
            let mut instance = Instance {
                id: uuid::Uuid::new_v4().to_string(),
                kind: EffectKind::Reverb,
                bypassed: false,
                parameters: Default::default(),
            };
            for (i, id) in IDS.iter().enumerate() {
                instance.parameters.insert(
                    (*id).into(),
                    ParameterValue::Integer(case["knobs"][i].as_i64().unwrap() as i32),
                );
            }
            for id in ["tempo_sync", "mid_side"] {
                instance.parameters.insert(
                    id.into(),
                    ParameterValue::Boolean(case[id].as_bool().unwrap()),
                );
            }
            let mut processor = Reverb::new(
                &instance,
                case["sample_rate"].as_u64().unwrap() as u32,
                case["tempo"].as_f64().unwrap(),
            )
            .unwrap();
            let input = pcm(&root.join(case["input"].as_str().unwrap()));
            let expected = pcm(&root.join(case["output"].as_str().unwrap()));
            assert_eq!(input.len(), expected.len());
            let mut maximum = 0.0f64;
            let mut error_energy = 0.0;
            let mut energy = 0.0;
            for (input, expected) in input
                .as_chunks::<2>()
                .0
                .iter()
                .zip(expected.as_chunks::<2>().0)
            {
                let actual = processor.frame(*input);
                for (actual, expected) in actual.into_iter().zip(expected) {
                    assert!(actual.is_finite());
                    let difference = actual as f64 - *expected as f64;
                    maximum = maximum.max(difference.abs());
                    error_energy += difference * difference;
                    energy += (*expected as f64).powi(2);
                }
            }
            let relative = (error_energy / energy.max(1e-30)).sqrt();
            eprintln!(
                "{}: max={maximum:e}, relative RMS={relative:e}",
                case["name"]
            );
            assert!(maximum <= 2e-6, "{}: max {maximum:e}", case["name"]);
            assert!(
                energy <= 1e-12 || relative <= 2e-5,
                "{}: relative {relative:e}",
                case["name"]
            );
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
