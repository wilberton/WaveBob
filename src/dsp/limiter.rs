// Copyright (C) 2026 ItsAlphaNeon (reference Dynamics algorithm)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Dynamics with an internal detector.
//! One-frame canonical ticks, preallocated logarithmic peak hold, fixed smoother
//! arrays, complete reset and prepared live updates adapt it to WaveBob.
mod design;
use crate::{
    CommandError, ErrorCode,
    metering::Column,
    processing::{EffectInfo, EffectKind, Instance, ParameterInfo, ParameterValue},
    rendering::Processor,
};
use design::*;
use std::cell::Cell;
pub const PARAMS: usize = 18;
pub const GAIN: usize = 0;
pub const SAT: usize = 1;
pub const CEILING: usize = 2;
pub const LIM_ATT: usize = 3;
pub const LIM_ATT_CURVE: usize = 4;
pub const LIM_REL: usize = 5;
pub const LIM_REL_CURVE: usize = 6;
pub const PEAK_WINDOW: usize = 7;
pub const COMP_THRES: usize = 8;
pub const COMP_RATIO: usize = 9;
pub const COMP_KNEE: usize = 10;
pub const COMP_ATT: usize = 11;
pub const COMP_REL: usize = 12;
pub const COMP_CURVE: usize = 13;
pub const RMS_WINDOW: usize = 14;
pub const GATE_GAIN: usize = 15;
pub const GATE_THRES: usize = 16;
pub const GATE_REL: usize = 17;
pub const IDS: [&str; 18] = [
    "gain",
    "sat",
    "ceil",
    "att",
    "att_curve",
    "rel",
    "rel_curve",
    "sustain",
    "c_thres",
    "c_ratio",
    "c_knee",
    "c_att",
    "c_rel",
    "c_curve",
    "c_rms",
    "g_gain",
    "g_thres",
    "g_rel",
];
pub const NAMES: [&str; 18] = [
    "Gain",
    "Soft saturation threshold",
    "Limiter ceiling",
    "Limiter attack time",
    "Limiter attack curve",
    "Limiter release time",
    "Limiter release curve",
    "Limiter peak window",
    "Comp threshold",
    "Comp ratio",
    "Comp knee",
    "Comp attack time",
    "Comp release time",
    "Comp curve",
    "Comp RMS window",
    "Noise gain",
    "Noise threshold",
    "Noise release time",
];
pub const RANGES: [(i32, i32); 18] = [
    (0, 2000),
    (0, 1000),
    (0, 2000),
    (0, 10000),
    (1, 8),
    (0, 10000),
    (1, 8),
    (0, 10000),
    (0, 1000),
    (-1000, 1000),
    (-1000, 1000),
    (0, 10000),
    (0, 10000),
    (1, 8),
    (0, 10000),
    (0, 1000),
    (0, 1000),
    (0, 10000),
];
pub const DEFAULTS: [i32; 18] = [
    1000, 1000, 1000, 1115, 3, 5000, 3, 2882, 1000, 0, 0, 0, 6666, 1, 650, 1000, 0, 4187,
];
pub fn info() -> EffectInfo {
    let mut parameters: Vec<_> = (0..18)
        .map(|i| ParameterInfo {
            id: IDS[i].into(),
            name: NAMES[i].into(),
            unit: match i {
                0..=2 | 8 | 15 | 16 => "raw (exponential dB/amplitude)",
                3 | 7 | 11 | 14 => "raw (derived ms)",
                5 | 12 | 17 => "raw (derived doubled release ms)",
                4 | 6 | 13 => "smoother stages",
                9 => "raw compression/expansion ratio",
                _ => "raw knee %",
            }
            .into(),
            value_type: "integer".into(),
            default: ParameterValue::Integer(DEFAULTS[i]),
            minimum: RANGES[i].0 as f64,
            maximum: RANGES[i].1 as f64,
        })
        .collect();
    parameters.push(ParameterInfo {
        id: "exact_ceiling".into(),
        name: "Aim at exact ceiling (disable safety margin)".into(),
        unit: "toggle".into(),
        value_type: "boolean".into(),
        default: ParameterValue::Boolean(false),
        minimum: 0.0,
        maximum: 1.0,
    });
    EffectInfo {
        kind: EffectKind::Limiter,
        available: true,
        parameters,
    }
}
pub fn values(instance: &Instance) -> [i32; 18] {
    std::array::from_fn(|i| match instance.parameters[IDS[i]] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!(),
    })
}
pub fn amplitude(i: usize, raw: i32) -> f32 {
    level(raw, if matches!(i, 1 | 2) { 3.0 } else { 7.0 })
}
pub fn value_text(i: usize, raw: i32) -> String {
    match i {
        0..=2 | 8 | 15 | 16 => {
            let a = amplitude(i, raw);
            if a == 0.0 {
                "−∞ dB · 0.00".into()
            } else {
                format!("{:.1} dB · {a:.2}", 20.0 * (a as f64).log10())
            }
        }
        3 | 7 | 11 | 14 => format!("{:.2} ms", time_ms(raw)),
        5 | 12 | 17 => format!("{:.2} ms", 2.0 * time_ms(raw)),
        4 | 6 | 13 => raw.to_string(),
        9 => {
            let c = 51.0_f64.powf(raw.unsigned_abs() as f64 / 1000.0) - 1.0;
            if raw > 0 {
                format!("{:.1}:1", 0.38 * c + 1.0)
            } else {
                format!("1:{:.1}", 0.06 * c + 1.0)
            }
        }
        _ => format!("{}%", (raw as f64 / 10.0).round()),
    }
}
fn limit() -> CommandError {
    CommandError::new(
        ErrorCode::ResourceLimit,
        "Limiter storage exceeds the prepared chain budget",
    )
}
fn zeros<T: Clone + Default>(n: usize) -> Result<Vec<T>, CommandError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| limit())?;
    v.resize(n, T::default());
    Ok(v)
}
/// Exact held maximum in logarithmic worst-case work, without a deque pop burst.
struct Peak {
    tree: Vec<f32>,
    capacity: usize,
    len: usize,
    at: usize,
}
impl Peak {
    fn new(n: usize) -> Result<Self, CommandError> {
        let capacity = n.checked_next_power_of_two().ok_or_else(limit)?;
        Ok(Self {
            tree: zeros(capacity * 2)?,
            capacity,
            len: n,
            at: 0,
        })
    }
    fn frame(&mut self, x: f32) -> f32 {
        let mut i = self.capacity + self.at;
        self.tree[i] = x;
        while i > 1 {
            i /= 2;
            let v = self.tree[i * 2].max(self.tree[i * 2 + 1]);
            if self.tree[i] == v {
                break;
            }
            self.tree[i] = v;
        }
        self.at = (self.at + 1) % self.len;
        self.tree[1]
    }
    fn clear(&mut self) {
        self.tree.fill(0.0);
        self.at = 0;
    }
}
pub(crate) struct Limiter {
    rate: u32,
    current: Design,
    target: Design,
    remaining: u32,
    peak: Peak,
    rms: Vec<f32>,
    rms_at: usize,
    rms_sum: f64,
    delay: Vec<[f32; 2]>,
    delay_at: usize,
    gate: f32,
    counter: i64,
    asleep: bool,
    meter: Cell<Column>,
    meter_frames: Cell<u64>,
}
impl Limiter {
    pub fn new(instance: &Instance, rate: u32) -> Result<Self, CommandError> {
        let exact = matches!(
            instance.parameters["exact_ceiling"],
            ParameterValue::Boolean(true)
        );
        let target = Design::new(&values(instance), rate as f64, exact);
        let w = target.work;
        let peak = w.peak_n.checked_next_power_of_two().ok_or_else(limit)?;
        let bytes = peak
            .checked_mul(8)
            .and_then(|b| b.checked_add(w.rms_m * 4))
            .and_then(|b| b.checked_add(w.look * 8))
            .ok_or_else(limit)?;
        if bytes > crate::rendering::MAX_CHAIN_BYTES || w.look > rate as usize {
            return Err(limit());
        }
        Ok(Self {
            rate,
            current: target,
            target,
            remaining: 0,
            peak: Peak::new(w.peak_n)?,
            rms: zeros(w.rms_m)?,
            rms_at: 0,
            rms_sum: 0.0,
            delay: zeros(w.look)?,
            delay_at: 0,
            gate: 1.0,
            counter: 0,
            asleep: true,
            meter: Cell::new(Column {
                gain: f32::MAX,
                ..Default::default()
            }),
            meter_frames: Cell::new(0),
        })
    }
    fn reset_partial(&mut self) {
        self.current.attack.reset();
        self.current.release.reset();
        self.current.compressor.reset();
        self.gate = 1.0;
        self.delay.fill([0.0; 2]);
        self.delay_at = 0;
    }
    fn record(&self, input: f32, output: f32, level: f32, gain: f32) {
        let mut c = self.meter.get();
        c.input = c.input.max(input);
        c.output = c.output.max(output);
        c.level = c.level.max(level);
        c.gain = c.gain.min(gain);
        self.meter.set(c);
        self.meter_frames.set(self.meter_frames.get() + 1);
    }
    fn slew(&mut self) {
        if self.remaining == 0 {
            return;
        }
        let n = self.remaining as f64;
        self.current.slew(&self.target, n);
        self.remaining -= 1;
        if self.remaining == 0 {
            self.current.finish(&self.target);
        }
    }
}
impl Processor for Limiter {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.slew();
        let w = self.current.work;
        let sound = input.iter().any(|x| x.abs() > 1e-6 || !x.is_finite());
        if sound {
            self.counter = w.tail;
        } else {
            self.counter -= 1;
            if self.counter <= -1 {
                if !self.asleep {
                    self.asleep = true;
                    self.reset_partial();
                }
                self.record(
                    input[0].abs().max(input[1].abs()),
                    input[0].abs().max(input[1].abs()),
                    0.0,
                    1.0,
                );
                return input;
            }
        }
        self.asleep = false;
        let pk = input[0].abs().max(input[1].abs());
        let held = self.peak.frame(pk);
        let mut gain = w.gain;
        if w.comp {
            let m = w.rms_m;
            let inv = (1.0 / (2.0 * m as f64)) as f32;
            let x = input[0] * input[0] + input[1] * input[1];
            self.rms_sum = (self.rms_sum - self.rms[self.rms_at] as f64) + x as f64;
            self.rms[self.rms_at] = x;
            self.rms_at = (self.rms_at + 1) % m;
            let rms = ((inv as f64 * self.rms_sum) as f32).max(0.0).sqrt();
            let lv = self.current.compressor.run(rms);
            if lv > w.threshold {
                let over = (f64::from(lv.min(w.top)).log10() as f32) * 20.0 + (-w.thr_db);
                let db = match w.knee_kind {
                    0 => over * w.slope,
                    kind => {
                        let mut u = over + 1.0;
                        u = if kind > 0 {
                            (f64::from(u * w.ka + 1.0).ln() as f32) * w.kb
                        } else {
                            ((f64::from(u * w.kb).exp() as f32) + -1.0) * w.kc
                        };
                        (u + -1.0) * w.ratio - over
                    }
                };
                let db_unit = (10.0_f64.ln() * 0.05) as f32;
                gain = (f64::from(db * db_unit).exp() as f32) * w.gain;
            }
        }
        let mut out = input;
        if w.look > 0 {
            out = self.delay[self.delay_at];
            self.delay[self.delay_at] = input;
            self.delay_at = (self.delay_at + 1) % w.look;
        }
        let lv = self.current.release.run(held);
        if lv * gain > w.limit {
            gain = w.limit / lv;
        }
        if lv < w.gate_below {
            gain = w.gate_gain + (self.gate - w.gate_gain) * w.gate_coef;
        }
        self.gate = gain;
        gain = self.current.attack.run(gain);
        let inv_room = 1.0 / w.room;
        for s in &mut out {
            let v = *s * gain;
            let a = v.abs();
            *s = if w.saturate {
                if a > w.knee {
                    let y = ((1.0 - (f64::from(w.knee - a) * f64::from(inv_room)).exp())
                        * f64::from(w.room)
                        + f64::from(w.knee)) as f32;
                    y.copysign(v)
                } else {
                    v
                }
            } else if a > w.ceiling {
                w.ceiling.copysign(v)
            } else {
                v
            };
        }
        self.record(pk, out[0].abs().max(out[1].abs()), lv, gain);
        self.current.release.flush();
        self.current.compressor.flush();
        self.current.attack.flush();
        out
    }
    fn reset(&mut self) {
        self.current = self.target;
        self.remaining = 0;
        self.reset_partial();
        self.peak.clear();
        self.rms.fill(0.0);
        self.rms_at = 0;
        self.rms_sum = 0.0;
        self.counter = 0;
        self.asleep = true;
        self.meter.set(Column {
            gain: f32::MAX,
            ..Default::default()
        });
        self.meter_frames.set(0);
    }
    fn latency(&self) -> usize {
        self.target.work.look
    }
    fn needs_context(&self) -> bool {
        true
    }
    fn activity(&self) -> f32 {
        if !self.rms_sum.is_finite()
            || [
                &self.current.release,
                &self.current.compressor,
                &self.current.attack,
            ]
            .into_iter()
            .any(|c| c.state[..c.len].iter().any(|v| !v.is_finite()))
        {
            f32::NAN
        } else {
            0.0
        }
    }
    fn take_meter(&self) -> Option<crate::metering::Block> {
        let frames = self.meter_frames.replace(0);
        if frames == 0 {
            None
        } else {
            Some(crate::metering::Block {
                frames,
                sample_rate: self.rate,
                levels: self.meter.replace(Column {
                    gain: f32::MAX,
                    ..Default::default()
                }),
            })
        }
    }
    fn bytes(&self) -> usize {
        self.peak.tree.capacity() * 4 + self.rms.capacity() * 4 + self.delay.capacity() * 8
    }
    fn can_retarget(&self, previous: &dyn Processor) -> bool {
        (previous as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|old| self.rate == old.rate && self.target.compatible(&old.current))
    }
    fn retarget(&mut self, previous: &mut dyn Processor) {
        let old = (previous as &mut dyn std::any::Any)
            .downcast_mut::<Self>()
            .unwrap();
        std::mem::swap(&mut self.peak, &mut old.peak);
        std::mem::swap(&mut self.rms, &mut old.rms);
        std::mem::swap(&mut self.delay, &mut old.delay);
        self.current = old.current;
        self.gate = old.gate;
        self.rms_at = old.rms_at;
        self.rms_sum = old.rms_sum;
        self.delay_at = old.delay_at;
        self.counter = old.counter;
        self.asleep = old.asleep;
        self.remaining = (self.rate / 100).max(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob_effect_reference::dynamics as reference;
    fn instance(v: [i32; 18], exact: bool) -> Instance {
        let mut parameters: std::collections::BTreeMap<_, _> = IDS
            .into_iter()
            .zip(v)
            .map(|(id, v)| (id.into(), ParameterValue::Integer(v)))
            .collect();
        parameters.insert("exact_ceiling".into(), ParameterValue::Boolean(exact));
        Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: EffectKind::Limiter,
            bypassed: false,
            parameters,
        }
    }
    fn input(i: usize, rate: u32) -> [f32; 2] {
        let t = i as f64 / rate as f64;
        if i == 0 {
            [2.8, -0.1]
        } else if i < 128 {
            [1.4, -0.4]
        } else if i < rate as usize / 8 {
            [
                (t * 1381.0).sin() as f32 * 0.1,
                (t * 2341.0).cos() as f32 * 0.04,
            ]
        } else if i < rate as usize / 2 {
            [
                (t * 6263.0).sin() as f32 * 1.6 + (t * 11931.0).sin() as f32 * 0.3,
                (t * 1973.0).cos() as f32 * 0.7,
            ]
        } else {
            [0.0; 2]
        }
    }
    fn compare(v: [i32; 18], rate: u32, exact: bool, label: &str) {
        let mut a = Limiter::new(&instance(v, exact), rate).unwrap();
        let mut r = reference::Dynamics::new(rate as f64, v, exact as u8);
        assert_eq!(a.latency(), r.latency());
        let mut max = 0.0_f32;
        let mut error = 0.0;
        let mut energy = 0.0;
        for i in 0..rate as usize * 2 {
            let x = input(i, rate);
            let out = a.frame(x);
            let mut l = [x[0]];
            let mut rr = [x[1]];
            r.process(&mut l, &mut rr, None);
            for (a, e) in out.into_iter().zip([l[0], rr[0]]) {
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
        eprintln!("{label}/{rate}: maximum {max:e}, relative RMS {rms:e}");
        assert!(
            max <= 2e-6 && rms <= 2e-5,
            "{label}/{rate}: {max:e}/{rms:e}"
        );
    }
    #[test]
    fn development_native_reference_modes_curves_endpoints_and_rates() {
        for rate in [44100, 48000, 96000] {
            compare(DEFAULTS, rate, false, "default");
            for (i, (lo, hi)) in RANGES.into_iter().enumerate() {
                for value in [lo, hi] {
                    let mut v = DEFAULTS;
                    v[i] = value;
                    compare(v, rate, false, &format!("endpoint-{i}-{value}"));
                }
            }
            for changes in [
                vec![(COMP_THRES, 500), (COMP_RATIO, 1000), (COMP_KNEE, 700)],
                vec![(COMP_THRES, 700), (COMP_RATIO, -1000), (COMP_KNEE, -700)],
                vec![(GATE_THRES, 500), (GATE_GAIN, 0), (LIM_ATT, 0)],
                vec![(SAT, 400), (CEILING, 1600), (GAIN, 1700)],
                vec![
                    (COMP_THRES, 300),
                    (COMP_ATT, 5000),
                    (COMP_REL, 10000),
                    (RMS_WINDOW, 10000),
                    (COMP_CURVE, 8),
                ],
            ] {
                let mut v = DEFAULTS;
                for (i, value) in changes {
                    v[i] = value;
                }
                compare(v, rate, true, "mixed-mode");
            }
        }
    }
    #[test]
    fn held_out_native_reference() {
        compare(
            [
                1371, 724, 817, 2781, 5, 6183, 7, 3782, 613, -731, 427, 1181, 7132, 4, 2171, 293,
                137, 5271,
            ],
            96000,
            true,
            "held-out",
        );
    }
    #[test]
    fn complete_reset_and_compatible_updates_keep_detector_state() {
        let mut p = DEFAULTS;
        p[COMP_THRES] = 500;
        p[COMP_RATIO] = 1000;
        let mut a = Limiter::new(&instance(p, false), 48000).unwrap();
        for i in 0..4096 {
            a.frame(input(i, 48000));
        }
        a.reset();
        let mut fresh = Limiter::new(&instance(p, false), 48000).unwrap();
        for i in 0..4096 {
            assert_eq!(a.frame(input(i, 48000)), fresh.frame(input(i, 48000)));
        }
        let (at, rms, sum) = (a.peak.at, a.rms_at, a.rms_sum);
        p[GAIN] = 1700;
        let mut b = Limiter::new(&instance(p, false), 48000).unwrap();
        assert!(b.can_retarget(&a));
        b.retarget(&mut a);
        assert_eq!((b.peak.at, b.rms_at, b.rms_sum), (at, rms, sum));
        for i in 0..480 {
            assert!(b.frame(input(i, 48000)).iter().all(|v| v.is_finite()));
        }
        assert_eq!(b.remaining, 0);
        assert_eq!(b.current.work.gain, b.target.work.gain);
        b.reset();
        let mut fresh = Limiter::new(&instance(p, false), 48000).unwrap();
        for i in 0..4096 {
            assert_eq!(b.frame(input(i, 48000)), fresh.frame(input(i, 48000)));
        }
        p[LIM_ATT] = 10000;
        assert!(
            !Limiter::new(&instance(p, false), 48000)
                .unwrap()
                .can_retarget(&b)
        );
        assert!(!b.stereo());
        assert!(!b.has_tail());
        assert!(b.needs_context());
    }
    #[test]
    fn canonical_sleep_wake_and_deep_quiet_match_the_original() {
        let mut p = DEFAULTS;
        p[COMP_THRES] = 500;
        p[COMP_RATIO] = -900;
        p[GATE_THRES] = 600;
        p[GATE_GAIN] = 0;
        let mut a = Limiter::new(&instance(p, true), 8000).unwrap();
        let mut r = reference::Dynamics::new(8000.0, p, 1);
        for i in 0..50000 {
            let x = if i < 2000 {
                1e-7
            } else if !(4000..=40000).contains(&i) {
                0.8 * (i as f32 * 0.19).sin()
            } else {
                0.0
            };
            let out = a.frame([x, -x * 0.37]);
            let mut l = [x];
            let mut rr = [-x * 0.37];
            r.process(&mut l, &mut rr, None);
            for (a, e) in out.into_iter().zip([l[0], rr[0]]) {
                assert!((a - e).abs() <= 2e-6);
            }
            if i == 39000 {
                assert!(a.asleep && r.asleep());
            }
        }
    }
    #[test]
    fn compensated_lookahead_longer_than_a_short_body_has_no_tail_or_missing_last_sample() {
        use crate::{
            documents::FrameRange,
            processing::Processing,
            rendering::{Chain, Channels, Voice},
        };
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut samples = vec![0.125; 257];
        samples[256] = 0.5;
        let source = crate::audio::prepare(
            8000,
            1,
            "float32".into(),
            Default::default(),
            samples,
            &cancel,
            |_| {},
        )
        .unwrap();
        for attack in [0, 1115, 10000] {
            let mut p = DEFAULTS;
            p[LIM_ATT] = attack;
            let state = Processing {
                effects: vec![instance(p, false)],
                ..Default::default()
            };
            let (audio, m) = crate::rendering::offline(
                &source,
                FrameRange { start: 0, end: 257 },
                &state,
                120.0,
                &cancel,
            )
            .unwrap();
            assert_eq!(m.channels, 1);
            assert_eq!(m.tail_frames, 0);
            assert_eq!(
                m.latency_frames,
                Limiter::new(&state.effects[0], 8000).unwrap().latency() as u64
            );
            assert_eq!(audio.samples.len(), 257);
            assert!(audio.samples[256] > 0.1);
            let chain = Chain::prepare(&state, 8000, Channels::Mono, 120.0).unwrap();
            let mut voice = Voice::with_chain(chain, &source, 0, None, || false).unwrap();
            for expected in audio.samples.iter() {
                assert!((voice.values[0][0] - expected).abs() <= 2e-6);
                voice.advance(&source, 1);
            }
        }
        let mut p = DEFAULTS;
        p[LIM_ATT] = 10000;
        let state = Processing {
            effects: vec![instance(p, false), instance(p, false)],
            ..Default::default()
        };
        assert_eq!(
            Chain::prepare(&state, 8000, Channels::Mono, 120.0)
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
    }
    #[test]
    fn retained_dynamics_source_matches_its_hash() {
        use sha2::{Digest, Sha256};
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../tools/effect-reference/pinned/dynamics.rs"
                ))
            ),
            "caf483ad97a52d0663eda8b01ac138d1957d5445e3365b489993043eb0c8b828"
        );
    }
}
