// Copyright (C) 2026 ItsAlphaNeon (reference signal laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Three-tap Chorus.
//! Host changes: fallible preparation, fixed reset phases, bounded tail queries,
//! host-independent cleanup quantum and state-preserving live coefficient slews.
mod laws;
use crate::{
    CommandError, ErrorCode,
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
use laws::*;
pub use laws::{DEFAULTS, RANGES};
pub const IDS: [&str; 12] = [
    "delay", "depth", "stereo", "speed1", "speed2", "speed3", "wave1", "wave2", "wave3", "cross",
    "cutoff", "wet",
];
pub const NAMES: [&str; 12] = [
    "Delay",
    "Depth",
    "Stereo",
    "LFO 1 speed",
    "LFO 2 speed",
    "LFO 3 speed",
    "LFO 1 wave",
    "LFO 2 wave",
    "LFO 3 wave",
    "Cross type",
    "Cross cutoff",
    "Wet only",
];
pub const WAVES: [&str; 4] = ["Arc", "Sharp arc", "Sines", "Sharp sines"];
pub fn info() -> EffectInfo {
    EffectInfo {
        kind: EffectKind::Chorus,
        available: true,
        parameters: (0..12)
            .map(|i| ParameterInfo {
                id: IDS[i].into(),
                name: NAMES[i].into(),
                unit: match i {
                    0 | 1 => "raw (derived ms)",
                    2 => "raw (derived degrees)",
                    3..=5 => "raw (5 x² Hz)",
                    6..=8 => "raw (four waveform regions)",
                    9 => "raw (>25 = low band)",
                    10 => "raw (MIDI-mapped Hz)",
                    _ => "raw (>256 = wet only)",
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
pub fn values(instance: &Instance) -> [i32; 12] {
    std::array::from_fn(|i| match instance.parameters[IDS[i]] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!(),
    })
}
pub fn value_text(i: usize, value: i32) -> String {
    let x = position(i, value);
    match i {
        DELAY => format!("{:.2} ms", x as f64 * 29.95 + 0.05),
        DEPTH => format!("{:.2} ms", x as f64 * 5.0),
        STEREO => format!("{:.1} deg", stereo(x) as i32 as f64 * 360.0 / 4096.0),
        3..=5 => format!("{:.2} Hz", 5.0 * (x as f64).powi(2)),
        6..=8 => WAVES[shape(x)].into(),
        CROSS_TYPE => if switch(x) { "Lows" } else { "Highs" }.into(),
        CROSS_CUTOFF => {
            let hz = cutoff_hz(x);
            if hz < 1000.0 {
                format!("{hz:.1} Hz")
            } else {
                format!("{:.3} kHz", hz / 1000.0)
            }
        }
        _ => if switch(x) { "Wet only" } else { "Dry + wet" }.into(),
    }
}
fn zeros(n: usize) -> Result<Vec<f32>, CommandError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| {
        CommandError::new(
            ErrorCode::ResourceLimit,
            "Cannot allocate Chorus delay storage",
        )
    })?;
    v.resize(n, 0.0);
    Ok(v)
}
/// Point/range maxima bound every future read even when the original 4096-cell
/// address arithmetic wraps at high rates. No scans or allocation during audio.
struct Line {
    data: Vec<f32>,
    max: Vec<f32>,
    position: usize,
}
impl Line {
    fn new() -> Result<Self, CommandError> {
        Ok(Self {
            data: zeros(LINE)?,
            max: zeros(LINE * 2)?,
            position: 0,
        })
    }
    fn write(&mut self, value: f32) {
        self.data[self.position] = value;
        let mut i = LINE + self.position;
        self.max[i] = value.abs();
        while i > 1 {
            i /= 2;
            let v = self.max[i * 2].max(self.max[i * 2 + 1]);
            if self.max[i] == v {
                break;
            }
            self.max[i] = v;
        }
        self.position = (self.position + 1) & (LINE - 1);
    }
    fn read(&self, p: u32) -> f32 {
        let j = ((p >> 12) & 4095) as usize;
        let f = p & 4095;
        let a = self.data[j];
        let d = self.data[(j + 1) & 4095] - a;
        (a as f64 + d as f64 * f as f64 * (1.0 / 4096.0)) as f32
    }
    fn range(&self, mut a: usize, mut b: usize) -> f32 {
        a += LINE;
        b += LINE;
        let mut out = 0.0_f32;
        while a < b {
            if a % 2 == 1 {
                out = out.max(self.max[a]);
                a += 1;
            }
            if b % 2 == 1 {
                b -= 1;
                out = out.max(self.max[b]);
            }
            a /= 2;
            b /= 2;
        }
        out
    }
    fn activity(&self, age: f64) -> f32 {
        let n = (age.ceil() as usize + 2).min(LINE);
        if n == LINE {
            return self.max[1];
        }
        let start = self.position.wrapping_sub(n) & 4095;
        if start < self.position {
            self.range(start, self.position)
        } else {
            self.range(start, LINE).max(self.range(0, self.position))
        }
    }
    fn clear(&mut self) {
        self.data.fill(0.0);
        self.max.fill(0.0);
        self.position = 0;
    }
    fn bytes(&self) -> usize {
        (self.data.capacity() + self.max.capacity()) * 4
    }
}
fn coefficients(w: Working) -> [f64; 9] {
    [
        w.delay as f64,
        w.depth as f64,
        w.stereo as i32 as f64,
        w.step[0] as f64,
        w.step[1] as f64,
        w.step[2] as f64,
        w.pole as f64,
        w.gain as f64,
        if w.wet_only { 0.0 } else { 1.0 },
    ]
}
pub(crate) struct Chorus {
    rate: u32,
    target: Working,
    work: Working,
    now: [f64; 9],
    remaining: u32,
    tables: &'static [u32],
    lines: [Line; 2],
    phase: [u32; 3],
    low: [f32; 2],
    before: [f32; 2],
    quantum: u8,
}
impl Chorus {
    pub fn new(instance: &Instance, rate: u32) -> Result<Self, CommandError> {
        let target = Working::new(&values(instance), rate.max(1));
        Ok(Self {
            rate,
            target,
            work: target,
            now: coefficients(target),
            remaining: 0,
            tables: tables()?,
            lines: [Line::new()?, Line::new()?],
            phase: [0, 1028, 28213],
            low: [0.0; 2],
            before: [0.0; 2],
            quantum: 0,
        })
    }
}
impl Processor for Chorus {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.remaining > 0 {
            for (n, t) in self.now.iter_mut().zip(coefficients(self.target)) {
                *n += (t - *n) / self.remaining as f64;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.now = coefficients(self.target);
            }
            self.work.delay = self.now[0] as u32;
            self.work.depth = self.now[1] as u32;
            self.work.stereo = self.now[2] as i32 as u32;
            self.work.step = std::array::from_fn(|i| self.now[3 + i] as u32);
            self.work.pole = self.now[6] as f32;
            self.work.gain = self.now[7] as f32;
        }
        let w = &self.work;
        let x = input.map(|v| (v as f64 * 0.7) as f32);
        let mut offsets = [[0u32; 3]; 2];
        for (k, phase) in self.phase.iter_mut().enumerate() {
            let table = &self.tables[w.shape[k] * LINE..(w.shape[k] + 1) * LINE];
            let i = *phase >> 12;
            offsets[0][k] = table[(i & 4095) as usize].wrapping_mul(w.depth) >> 9;
            offsets[1][k] =
                table[(i.wrapping_add(w.stereo) & 4095) as usize].wrapping_mul(w.depth) >> 9;
            *phase = phase.wrapping_add(w.step[k]);
        }
        let base = ((self.lines[0].position as u32) << 12).wrapping_sub(w.delay);
        let mut output = [0.0; 2];
        for s in 0..2 {
            let low = (x[s] + self.before[s]) * w.gain + w.pole * self.low[s];
            self.low[s] = low;
            self.before[s] = x[s];
            self.lines[s].write(if w.low_band { low } else { x[s] - low });
            let [a, b, c] = offsets[s].map(|o| self.lines[s].read(base.wrapping_sub(o)));
            let wet = ((a + b + c) as f64 * 0.6) as f32;
            output[s] = if self.now[8] == 0.0 {
                wet
            } else {
                wet + x[s] * self.now[8] as f32
            };
        }
        self.quantum += 1;
        if self.quantum == 64 {
            self.quantum = 0;
            for low in &mut self.low {
                if (low.abs() as f64) < 1e-11 {
                    *low = 0.0;
                }
            }
        }
        output
    }
    fn reset(&mut self) {
        self.work = self.target;
        self.now = coefficients(self.target);
        self.remaining = 0;
        self.phase = [0, 1028, 28213];
        self.low = [0.0; 2];
        self.before = [0.0; 2];
        self.quantum = 0;
        for line in &mut self.lines {
            line.clear();
        }
    }
    fn stereo(&self) -> bool {
        true
    }
    fn has_tail(&self) -> bool {
        true
    }
    fn activity(&self) -> f32 {
        let delay = self.work.delay.max(self.target.delay) as f64 / 4096.0;
        let depth = self.work.depth.max(self.target.depth) as f64 * 32766.0 / 512.0 / 4096.0;
        let peak = self
            .lines
            .iter()
            .map(|l| l.activity(delay + depth))
            .chain(self.low.into_iter().map(f32::abs))
            .chain(self.before.into_iter().map(f32::abs))
            .fold(0.0_f32, f32::max);
        peak * 8.0
    }
    fn bytes(&self) -> usize {
        self.lines.iter().map(Line::bytes).sum::<usize>() + self.tables.len() * 4
    }
    fn can_retarget(&self, previous: &dyn Processor) -> bool {
        (previous as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|old| {
                old.rate == self.rate
                    && old.work.shape == self.target.shape
                    && old.work.low_band == self.target.low_band
            })
    }
    fn retarget(&mut self, previous: &mut dyn Processor) {
        let old = (previous as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        std::mem::swap(&mut self.lines, &mut old.lines);
        self.work = old.work;
        self.now = old.now;
        self.phase = old.phase;
        self.low = old.low;
        self.before = old.before;
        self.quantum = old.quantum;
        self.remaining = (self.rate / 100).max(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob_effect_reference::chorus as reference;
    fn instance(v: [i32; 12]) -> Instance {
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::Chorus,
            bypassed: false,
            parameters: IDS
                .into_iter()
                .zip(v)
                .map(|(k, v)| (k.into(), ParameterValue::Integer(v)))
                .collect(),
        }
    }
    fn input(i: usize, rate: u32) -> [f32; 2] {
        let t = i as f64 / rate as f64;
        if i == 0 {
            [0.8, -0.2]
        } else if i < 128 {
            [0.2, -0.3]
        } else if i < rate as usize / 3 {
            [
                (t * 3137.0).sin() as f32 * 0.5 + (t * 17131.0).sin() as f32 * 0.1,
                (t * 7531.0).cos() as f32 * 0.4,
            ]
        } else {
            [0.0; 2]
        }
    }
    fn compare(v: [i32; 12], rate: u32, name: &str) {
        let frames = rate as usize;
        let mut a = Chorus::new(&instance(v), rate).unwrap();
        let mut r = reference::Chorus::with_settings(rate, &reference::Settings { v });
        let mut left: Vec<_> = (0..frames).map(|i| input(i, rate)[0]).collect();
        let mut right: Vec<_> = (0..frames).map(|i| input(i, rate)[1]).collect();
        for (l, rpart) in left.chunks_mut(64).zip(right.chunks_mut(64)) {
            r.process(l, rpart);
        }
        let mut max = 0.0_f32;
        let mut error = 0.0;
        let mut energy = 0.0;
        for i in 0..frames {
            let actual = a.frame(input(i, rate));
            for (x, y) in actual.into_iter().zip([left[i], right[i]]) {
                assert!(x.is_finite());
                let d = (x - y).abs();
                max = max.max(d);
                error += (d as f64).powi(2);
                energy += (y as f64).powi(2);
            }
        }
        let rms = if energy > 1e-12 {
            (error / energy).sqrt()
        } else {
            0.0
        };
        eprintln!("{name} {rate}: max {max:e}; relative RMS {rms:e}");
        assert!(max <= 2e-6 && rms <= 2e-5, "{name}: {max:e}/{rms:e}");
    }
    #[test]
    fn development_native_reference_all_shapes_extremes_and_rates() {
        for rate in [44100, 48000, 96000] {
            compare(DEFAULTS, rate, "default");
            for (i, (lo, hi)) in RANGES.into_iter().enumerate() {
                for value in [lo, hi] {
                    let mut v = DEFAULTS;
                    v[i] = value;
                    compare(v, rate, &format!("endpoint-{i}-{value}"));
                }
            }
            for wave in [0, 342, 683, 1024] {
                for lows in [0, 100] {
                    let mut v = DEFAULTS;
                    v[6..9].fill(wave);
                    v[CROSS_TYPE] = lows;
                    v[WET_ONLY] = 1024;
                    v[DELAY] = 1024;
                    v[DEPTH] = 5000;
                    compare(v, rate, "shape-band-wet");
                }
            }
        }
    }
    #[test]
    fn held_out_native_reference() {
        compare(
            [
                183, 3791, 719, 2173, 4931, 971, 502, 1007, 321, 79, 3981, 893,
            ],
            96000,
            "held-out",
        );
    }
    #[test]
    fn reset_repeats_seeded_phases_and_compatible_slew_keeps_state() {
        let mut a = Chorus::new(&instance(DEFAULTS), 48000).unwrap();
        let expected: Vec<_> = (0..4096).map(|i| a.frame(input(i, 48000))).collect();
        a.reset();
        assert_eq!(a.phase, [0, 1028, 28213]);
        let again: Vec<_> = (0..4096).map(|i| a.frame(input(i, 48000))).collect();
        assert_eq!(again, expected);
        let phase = a.phase;
        let position = a.lines[0].position;
        let mut v = DEFAULTS;
        v[DELAY] = 1024;
        v[DEPTH] = 5000;
        v[WET_ONLY] = 1024;
        let mut b = Chorus::new(&instance(v), 48000).unwrap();
        assert!(b.can_retarget(&a));
        b.retarget(&mut a);
        assert_eq!(b.phase, phase);
        assert_eq!(b.lines[0].position, position);
        assert_eq!(b.remaining, 480);
        for i in 0..480 {
            assert!(b.frame(input(i, 48000)).iter().all(|v| v.is_finite()));
        }
        assert_eq!(b.remaining, 0);
        assert_eq!(b.now, coefficients(b.target));
        b.reset();
        let mut fresh = Chorus::new(&instance(v), 48000).unwrap();
        for i in 0..4096 {
            assert_eq!(b.frame(input(i, 48000)), fresh.frame(input(i, 48000)));
        }
        v[WAVE[0]] = 1024;
        assert!(!Chorus::new(&instance(v), 48000).unwrap().can_retarget(&b));
        v[WAVE[0]] = 23;
        v[CROSS_TYPE] = 100;
        assert!(!Chorus::new(&instance(v), 48000).unwrap().can_retarget(&b));
        assert!(b.stereo());
        assert_eq!(b.latency(), 0);
        assert!(b.bytes() < 256 * 1024);
    }
    #[test]
    fn silence_and_exact_range_maxima_do_not_hide_future_reads() {
        let mut c = Chorus::new(&instance(DEFAULTS), 48000).unwrap();
        for _ in 0..20000 {
            assert_eq!(c.frame([0.0; 2]), [0.0; 2]);
            assert_eq!(c.activity(), 0.0);
        }
        let mut l = Line::new().unwrap();
        for i in 0..10000 {
            l.write(if i % 127 == 0 { 1.0 } else { 0.0 });
            for age in [1.0, 37.0, 4000.0, 10000.0] {
                let n = (age as usize + 2).min(LINE);
                let expected = (1..=n)
                    .map(|k| l.data[l.position.wrapping_sub(k) & 4095].abs())
                    .fold(0.0_f32, f32::max);
                assert_eq!(l.activity(age), expected);
            }
        }
    }
    #[test]
    fn mono_promotion_natural_tail_and_silence_follow_shared_policy() {
        use crate::{
            documents::FrameRange,
            processing::{Action, Processing},
        };
        let cancel = std::sync::atomic::AtomicBool::new(false);
        for level in [0.0_f32, 1.0] {
            let source = crate::audio::prepare(
                8000,
                1,
                "float32".into(),
                Default::default(),
                vec![level],
                &cancel,
                |_| {},
            )
            .unwrap();
            let mut state = Processing::default();
            state
                .apply(
                    Action::Add {
                        kind: EffectKind::Chorus,
                    },
                    1,
                )
                .unwrap();
            let (audio, m) = crate::rendering::offline(
                &source,
                FrameRange { start: 0, end: 1 },
                &state,
                120.0,
                &cancel,
            )
            .unwrap();
            assert_eq!(m.channels, 2);
            assert_eq!(m.latency_frames, 0);
            assert!(!m.tail_capped);
            if level == 0.0 {
                assert_eq!(m.tail_frames, 0);
            } else {
                assert!(m.tail_frames > 800 && m.tail_frames < 8000);
                assert!(audio.samples[2..].iter().any(|v| v.abs() > 1e-4));
            }
            state.effects[0].bypassed = true;
            assert_eq!(
                crate::rendering::offline(
                    &source,
                    FrameRange { start: 0, end: 1 },
                    &state,
                    120.0,
                    &cancel
                )
                .unwrap()
                .1
                .tail_frames,
                0
            );
        }
        let mut v = DEFAULTS;
        v[DELAY] = 1024;
        v[DEPTH] = 5000;
        compare(v, 192000, "original 4096-cell wrap");
    }
    #[test]
    fn raw_switch_regions_displays_and_pinned_source_are_exact() {
        use sha2::{Digest, Sha256};
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../tools/effect-reference/pinned/chorus.rs"
                ))
            ),
            "49049c017d9463dce7983a13e9e95a550f686ebb717de47f9439e7f8b02d89fe"
        );
        for (raw, expected) in [
            (23, "Arc"),
            (341, "Arc"),
            (342, "Sharp arc"),
            (682, "Sharp arc"),
            (683, "Sines"),
            (1023, "Sines"),
            (1024, "Sharp sines"),
        ] {
            assert_eq!(value_text(6, raw), expected);
        }
        assert_eq!(value_text(9, 25), "Highs");
        assert_eq!(value_text(9, 26), "Lows");
        assert_eq!(value_text(11, 256), "Dry + wet");
        assert_eq!(value_text(11, 257), "Wet only");
        assert_eq!(value_text(0, 512), "15.03 ms");
        assert_eq!(value_text(2, 0), "-0.1 deg");
        assert_eq!(value_text(2, 1024), "179.9 deg");
    }
}
