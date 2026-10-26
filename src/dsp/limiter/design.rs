// Copyright (C) 2026 ItsAlphaNeon (reference equations)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Limiter coefficient laws.
//! Fixed smoother arrays and preparation-only coefficient design replace allocations.
use super::{
    CEILING, COMP_ATT, COMP_CURVE, COMP_KNEE, COMP_RATIO, COMP_REL, COMP_THRES, GAIN, GATE_GAIN,
    GATE_REL, GATE_THRES, LIM_ATT, LIM_ATT_CURVE, LIM_REL, LIM_REL_CURVE, PARAMS, PEAK_WINDOW,
    RMS_WINDOW, SAT,
};
const FLAG_EXACT_CEILING: u8 = 1;
/// A level knob: amplitude for stored value `v` with base `b` (spec "Knob laws").
pub fn level(v: i32, b: f64) -> f32 {
    let t = v as f32 * 0.001f32;
    if t == 1.0 {
        return 1.0;
    }
    let x = t * (b.ln() as f32);
    ((f64::from(x).exp() - 1.0) * f64::from((1.0 / (b - 1.0)) as f32)) as f32
}

/// A time knob's milliseconds (0..1000).
pub fn time_ms(v: i32) -> f32 {
    let x = (f64::from(v) * 0.0001) as f32;
    ((f64::from(x) * 501f64.ln()).exp() - 1.0) as f32 * 2.0
}

/// A time knob's length in samples.
pub fn time_samples(v: i32, rate: f64) -> f64 {
    f64::from(time_ms(v)) * 0.001 * rate
}

fn round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// The coefficient of a smoother direction taking `t` samples (0 = no smoothing).
fn coef(t: f64) -> f64 {
    if t > 0.0 {
        (0.01f64.ln() / t.max(1.0)).exp()
    } else {
        0.0
    }
}

/// The values worked out from the parameters (spec "Working values").
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Work {
    pub(super) gain: f32,
    pub(super) ceiling: f32,
    pub(super) limit: f32,
    pub(super) gate_gain: f32,
    pub(super) gate_below: f32,
    pub(super) gate_coef: f32,
    pub(super) knee: f32,
    pub(super) room: f32,
    pub(super) saturate: bool,
    pub(super) comp: bool,
    pub(super) threshold: f32,
    pub(super) thr_db: f32,
    pub(super) top: f32,
    pub(super) ratio: f32,
    pub(super) slope: f32,
    pub(super) knee_kind: i8,
    pub(super) ka: f32,
    pub(super) kb: f32,
    pub(super) kc: f32,
    pub(super) look: usize,
    pub(super) peak_n: usize,
    pub(super) rms_m: usize,
    pub(super) tail: i64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Smoother {
    pub(super) rise: f64,
    pub(super) fall: f64,
    start: f64,
    pub(super) state: [f64; 8],
    pub(super) len: usize,
}
impl Smoother {
    fn new(start: f64) -> Self {
        Self {
            rise: 0.0,
            fall: 0.0,
            start,
            state: [start; 8],
            len: 0,
        }
    }
    fn set(&mut self, n: usize, rise: f64, fall: f64) {
        if self.len != n {
            self.state.fill(self.start);
            self.len = n;
        }
        self.rise = coef(rise);
        self.fall = coef(fall);
    }
    pub(super) fn reset(&mut self) {
        self.state.fill(self.start);
    }
    pub(super) fn run(&mut self, mut x: f32) -> f32 {
        for s in &mut self.state[..self.len] {
            let v = x as f64;
            let c = if v >= *s { self.rise } else { self.fall };
            *s = v + (*s - v) * c;
            x = *s as f32;
        }
        x
    }
    pub(super) fn flush(&mut self) {
        for s in &mut self.state[..self.len] {
            if s.abs() <= 1.0 / 16_777_216.0 || s.is_nan() {
                *s = 0.0;
            }
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Design {
    pub(super) work: Work,
    pub(super) release: Smoother,
    pub(super) compressor: Smoother,
    pub(super) attack: Smoother,
}
impl Design {
    pub(super) fn new(p: &[i32; PARAMS], rate: f64, exact: bool) -> Self {
        let mut d = Self {
            work: Work::default(),
            release: Smoother::new(0.0),
            compressor: Smoother::new(0.0),
            attack: Smoother::new(1.0),
        };
        d.design(p, rate, exact as u8);
        d
    }
    fn design(&mut self, p: &[i32; PARAMS], rate: f64, flags: u8) {
        let mut w = Work {
            gain: level(p[GAIN], 7.0),
            ceiling: level(p[CEILING], 3.0),
            ..Work::default()
        };
        w.limit = if flags & FLAG_EXACT_CEILING != 0 {
            w.ceiling
        } else {
            w.ceiling * 0.975
        };
        w.gate_gain = level(p[GATE_GAIN], 7.0);
        w.gate_below = level(p[GATE_THRES], 7.0).min(w.limit);
        w.gate_coef =
            (0.01f64.ln() / (time_samples(p[GATE_REL], rate).max(1.0) * 2.0)).exp() as f32;
        w.knee = level(p[SAT], 3.0);
        w.saturate = w.knee < 1.0;
        if w.saturate {
            let room = w.ceiling.min(1.0) - w.knee;
            w.room = if room > 0.0 { room } else { 1.525_878_9e-5 };
        }

        w.comp = p[COMP_THRES] < 1000;
        let db_unit = (10f64.ln() * 0.05) as f32;
        w.threshold = level(p[COMP_THRES], 7.0).max((f64::from(db_unit) * -60.0).exp() as f32);
        w.thr_db = (f64::from(w.threshold).log10() * 20.0) as f32;
        w.top = w.threshold * 100.0;
        let c = ((f64::from((p[COMP_RATIO].unsigned_abs() as f64 * 0.001) as f32) * 51f64.ln())
            .exp()
            - 1.0) as f32;
        w.ratio = if p[COMP_RATIO] > 0 {
            (1.0 / (f64::from(c) * 0.38 + 1.0)) as f32
        } else {
            (f64::from(c) * 0.06 + 1.0) as f32
        };
        w.slope = w.ratio - 1.0;
        let kv = p[COMP_KNEE];
        let k = ((f64::from((kv as f32 * 0.001).abs()) * 201f64.ln()).exp() - 1.0) as f32;
        if kv > 0 {
            w.knee_kind = 1;
            w.ka = (f64::from(k) * 0.005) as f32;
            w.kb = (1.0 / f64::from(w.ka + 1.0).ln()) as f32;
        } else if kv < 0 {
            w.knee_kind = -1;
            w.ka = (f64::from(k) * 0.001) as f32;
            w.kb = f64::from(w.ka + 1.0).ln() as f32;
            w.kc = 1.0 / w.ka;
        }

        let look = round(time_samples(p[LIM_ATT], rate)).max(0) as usize;
        w.look = look;
        w.peak_n = (round(time_samples(p[PEAK_WINDOW], rate)).max(0) as usize)
            .max(look)
            .max(1);
        w.rms_m = (round(time_samples(p[RMS_WINDOW], rate)).max(0) as usize).max(1);
        w.tail = look as i64 + round(rate * 1.5);

        // Smoother chains.
        let rel = time_samples(p[LIM_REL], rate);
        let rel_stages = if rel == 0.0 {
            0
        } else {
            p[LIM_REL_CURVE].clamp(0, 64) as usize
        };
        self.release.set(
            rel_stages,
            0.0,
            rel * 2.0 / (rel_stages.max(1) as f64).sqrt(),
        );
        let cs = p[COMP_CURVE].clamp(0, 64) as usize;
        let sq = (cs.max(1) as f64).sqrt();
        self.compressor.set(
            cs,
            time_samples(p[COMP_ATT], rate) / sq,
            time_samples(p[COMP_REL], rate) * 2.0 / sq,
        );
        let att_stages = if look < 1 {
            0
        } else {
            p[LIM_ATT_CURVE].clamp(0, 64) as usize
        };
        let at = f64::from((look as f64 * 0.8) as f32) / (att_stages.max(1) as f64).sqrt();
        self.attack.set(att_stages, at, at);

        self.work = w;
    }
}

impl Design {
    pub(super) fn compatible(&self, old: &Self) -> bool {
        self.work.look == old.work.look
            && self.work.peak_n == old.work.peak_n
            && self.work.rms_m == old.work.rms_m
            && self.work.comp == old.work.comp
            && self.work.saturate == old.work.saturate
            && self.work.knee_kind == old.work.knee_kind
            && self.attack.len == old.attack.len
            && self.release.len == old.release.len
            && self.compressor.len == old.compressor.len
    }
    pub(super) fn slew(&mut self, target: &Self, n: f64) {
        self.work.gain =
            (self.work.gain as f64 + (target.work.gain as f64 - self.work.gain as f64) / n) as f32;
        self.work.ceiling = (self.work.ceiling as f64
            + (target.work.ceiling as f64 - self.work.ceiling as f64) / n)
            as f32;
        self.work.limit = (self.work.limit as f64
            + (target.work.limit as f64 - self.work.limit as f64) / n)
            as f32;
        self.work.gate_gain = (self.work.gate_gain as f64
            + (target.work.gate_gain as f64 - self.work.gate_gain as f64) / n)
            as f32;
        self.work.gate_below = (self.work.gate_below as f64
            + (target.work.gate_below as f64 - self.work.gate_below as f64) / n)
            as f32;
        self.work.gate_coef = (self.work.gate_coef as f64
            + (target.work.gate_coef as f64 - self.work.gate_coef as f64) / n)
            as f32;
        self.work.knee =
            (self.work.knee as f64 + (target.work.knee as f64 - self.work.knee as f64) / n) as f32;
        self.work.room =
            (self.work.room as f64 + (target.work.room as f64 - self.work.room as f64) / n) as f32;
        self.work.threshold = (self.work.threshold as f64
            + (target.work.threshold as f64 - self.work.threshold as f64) / n)
            as f32;
        self.work.thr_db = (self.work.thr_db as f64
            + (target.work.thr_db as f64 - self.work.thr_db as f64) / n)
            as f32;
        self.work.top =
            (self.work.top as f64 + (target.work.top as f64 - self.work.top as f64) / n) as f32;
        self.work.ratio = (self.work.ratio as f64
            + (target.work.ratio as f64 - self.work.ratio as f64) / n)
            as f32;
        self.work.slope = (self.work.slope as f64
            + (target.work.slope as f64 - self.work.slope as f64) / n)
            as f32;
        self.work.ka =
            (self.work.ka as f64 + (target.work.ka as f64 - self.work.ka as f64) / n) as f32;
        self.work.kb =
            (self.work.kb as f64 + (target.work.kb as f64 - self.work.kb as f64) / n) as f32;
        self.work.kc =
            (self.work.kc as f64 + (target.work.kc as f64 - self.work.kc as f64) / n) as f32;
        for (now, next) in [
            (&mut self.release, &target.release),
            (&mut self.compressor, &target.compressor),
            (&mut self.attack, &target.attack),
        ] {
            now.rise += (next.rise - now.rise) / n;
            now.fall += (next.fall - now.fall) / n;
        }
    }
    pub(super) fn finish(&mut self, target: &Self) {
        self.work = target.work;
        for (now, next) in [
            (&mut self.release, &target.release),
            (&mut self.compressor, &target.compressor),
            (&mut self.attack, &target.attack),
        ] {
            now.rise = next.rise;
            now.fall = next.fall;
        }
    }
}
