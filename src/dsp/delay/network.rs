// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// Source provenance and license details: assets/licenses/.

//! Multi-model stereo network. WaveBob adds fallible model-sized storage, bounded
//! block range maxima, split scalar state and stable below-Nyquist colour corners.
//!
//! A delay line with feedback in four types (mono, true stereo, ping pong, off). The feedback path runs through a 10 Hz
//! high pass, a state-variable filter, a shaper (sample-rate and bit reduction, then a saturator or a limiter) and a
//! diffusion all-pass. Delay times follow the tempo when synced and glide or crossfade when they change; an LFO sways
//! the delay time and the filter's cutoff; a one-pole colour filter tints the echoes.
//!

#![allow(clippy::approx_constant, clippy::excessive_precision)]
// Constants and evaluation order retain the pinned reference's f32 arithmetic.
use crate::{CommandError, ErrorCode};
fn limit() -> CommandError {
    CommandError::new(
        ErrorCode::ResourceLimit,
        "Delay storage exceeds its prepared memory budget",
    )
}

/// The settings, in the saved state's order.
pub const PARAMS: usize = 26;
pub const INPUT: usize = 0;
pub const TYPE: usize = 1;
pub const SYNC: usize = 2;
pub const KEEP_PITCH: usize = 3;
pub const TIME: usize = 4;
pub const OFFSET: usize = 5;
pub const SMOOTH: usize = 6;
pub const SPREAD: usize = 7;
pub const MOD_RATE: usize = 8;
pub const MOD_TIME: usize = 9;
pub const MOD_CUTOFF: usize = 10;
pub const DIFF_LEVEL: usize = 11;
pub const DIFF_SPREAD: usize = 12;
pub const DIST_TYPE: usize = 13;
pub const FEEDBACK: usize = 14;
pub const FILTER: usize = 15;
pub const CUTOFF: usize = 16;
pub const RESONANCE: usize = 17;
pub const DRIVE: usize = 18;
pub const HARDNESS: usize = 19;
pub const SYMMETRY: usize = 20;
pub const RATE_RED: usize = 21;
pub const BITS: usize = 22;
pub const WET: usize = 23;
pub const TONE: usize = 24;
pub const DRY: usize = 25;

/// The settings' ranges (stored values are kept as they are; their normalised values are clamped).
pub const RANGES: [(i32, i32); PARAMS] = [
    (0, 6000),
    (0, 3),
    (0, 1),
    (0, 1),
    (1, 768),
    (0, 6000),
    (0, 1000),
    (0, 6000),
    (0, 6000),
    (0, 6000),
    (0, 6000),
    (0, 6000),
    (0, 6000),
    (0, 1),
    (0, 6000),
    (0, 3),
    (0, 6000),
    (0, 6000),
    (0, 6000),
    (0, 6000),
    (0, 6000),
    (0, 60000),
    (0, 6000),
    (0, 6000),
    (-3000, 3000),
    (0, 6000),
];

/// A new instance without a state.
pub const DEFAULTS: [i32; PARAMS] = [
    6000, 0, 0, 0, 80, 3000, 500, 6000, 3000, 0, 0, 0, 3000, 0, 3000, 0, 3000, 3000, 0, 0, 0,
    60000, 6000, 6000, 0, 6000,
];

/// Delay types.
pub const MONO: i32 = 0;
pub const TRUE_STEREO: i32 = 1;
pub const PING_PONG: i32 = 2;
pub const OFF: i32 = 3;

/// Filter modes.
pub const LOW_PASS: i32 = 0;
pub const HIGH_PASS: i32 = 1;
pub const BAND_PASS: i32 = 2;
pub const FILTER_OFF: i32 = 3;

/// The settings that move to a new value in steps (the others take effect at once).
const SMOOTHED: [bool; PARAMS] = {
    let mut s = [false; PARAMS];
    let list = [
        INPUT, SPREAD, MOD_CUTOFF, DIFF_LEVEL, FEEDBACK, CUTOFF, RESONANCE, DRIVE, HARDNESS,
        SYMMETRY, RATE_RED, BITS, WET, TONE, DRY,
    ];
    let mut i = 0;
    while i < list.len() {
        s[list[i]] = true;
        i += 1;
    }
    s
};

/// The filter's and the colour's corner tables (Hz).
pub const LP: [f32; 5] = [270.0, 480.0, 1300.0, 7000.0, 22000.0];
pub const HP: [f32; 5] = [20.0, 90.0, 400.0, 1300.0, 4200.0];
/// The LFO's periods (s) over its speed.
pub const LFO_PERIODS: [f32; 8] = [8.872, 3.808, 1.833, 0.878, 0.398, 0.15, 0.05, 0.0];

/// Reference pre-roll silence: four device buffers of 2048 frames.
pub const PREROLL: usize = 4 * 2048;

/// Everything the saved state says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub v: [i32; PARAMS],
}

impl Default for Settings {
    fn default() -> Self {
        Settings { v: DEFAULTS }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Laws (also used by the editor's value texts)

/// Setting `k`'s normalised value for stored value `v`.
pub fn norm(k: usize, v: i32) -> f32 {
    if matches!(k, SYNC | KEEP_PITCH | DIST_TYPE) {
        return v as f32;
    }
    let (min, max) = RANGES[k];
    let r = 1.0 / f64::from(max - min);
    ((f64::from(v) - f64::from(min)) * r).clamp(0.0, 1.0) as f32
}

/// A four-position switch (type, filter mode) from its normalised value.
pub fn four_way(x: f32) -> i32 {
    (3.0 * f64::from(x) + 0.5).trunc() as i32
}

/// `(1 - u) T[i] + u T[i + 1]`, `i = Trunc(a)`.
pub fn lookup(t: &[f32], a: f32) -> f32 {
    let i = (a.trunc().max(0.0) as usize).min(t.len() - 2);
    let u = a - i as f32;
    (1.0 - u) * t[i] + u * t[i + 1]
}

/// The delay time in seconds for the Time setting's normalised value.
pub fn delay_seconds(x: f32, sync: bool, bpm: f64) -> f64 {
    if sync {
        let beats = sync_steps(x) as f32 / 192.0;
        f64::from(beats) * (1.0 / (bpm / 60.0))
    } else {
        let u = f64::from(x) * 0.94 + 0.06;
        ((u * u * u - 0.000216) * 1.0002160466660799) * 0.999 + 0.001
    }
}

/// The synced time in 1/192 beat steps (1..768).
pub fn sync_steps(x: f32) -> i32 {
    (767.0 * f64::from(x) + 0.5).trunc() as i32 + 1
}

/// The LFO's period (s) for a speed `s` (the setting's x times 0.99 in the sound).
pub fn lfo_period(s: f32) -> f32 {
    let a = s * 5.999_400_1;
    let i = (a.trunc().max(0.0) as usize).min(LFO_PERIODS.len() - 2);
    let u = a - i as f32;
    (1.0 - u) * LFO_PERIODS[i] + u * LFO_PERIODS[i + 1]
}

/// The rate reduction's hold period in frames at 44.1 kHz for an amount `t` (0..1).
pub fn hold_period(t: f32) -> f32 {
    let t = t * 127.0;
    ((((t * 0.0002) * t) * t - (t * 0.0171) * t) + t * 0.7435) + 1.0
}

/// The bit reduction's `1 - x`.
fn one_minus(x: f32) -> f32 {
    (1.0 - f64::from(x)) as f32
}

/// The bit reduction's resolution in bits (`steps = 2^bits`); 0 = off.
pub fn bits(x: f32) -> f32 {
    if x >= 1.0 {
        return 0.0;
    }
    let t = one_minus(x);
    let b = t * t;
    let q = b.sqrt().sqrt();
    (1.0 - q) * 14.0 + 2.0
}

/// `10^(db / 20)`, the division in single.
fn pow10_20(db: f32) -> f32 {
    10f64.powf(f64::from(db / 20.0)) as f32
}

// ---------------------------------------------------------------------------------------------------------------
// Building blocks

/// Flush to zero: the original runs with denormals off.
#[inline]
fn ftz(x: f32) -> f32 {
    if x.abs() < f32::MIN_POSITIVE { 0.0 } else { x }
}

/// Fractional delay with a bounded range maximum over 64-frame blocks.
/// The 128-frame capacity margin keeps partially overwritten old blocks outside
/// every possible read window. Queries may overestimate by at most 63 frames.
pub(super) struct Ring {
    buf: Vec<f32>,
    pub(super) pos: usize,
    tree: Vec<f32>,
}
impl Ring {
    fn new(len: usize) -> Result<Self, CommandError> {
        fn zeros(n: usize) -> Result<Vec<f32>, CommandError> {
            let mut v = Vec::new();
            v.try_reserve_exact(n).map_err(|_| limit())?;
            v.resize(n, 0.0);
            Ok(v)
        }
        Ok(Self {
            buf: zeros(len)?,
            pos: 0,
            tree: zeros(len / 32)?,
        })
    }
    #[inline]
    fn read(&self, d: f32) -> f32 {
        let m = self.buf.len() - 1;
        let j = d.trunc() as i64 as u32;
        let f = d - j as f32;
        let i = self.pos.wrapping_sub(j as usize);
        (1.0 - f) * self.buf[i & m] + f * self.buf[i.wrapping_sub(1) & m]
    }
    #[inline]
    fn write(&mut self, v: f32) {
        let v = ftz(v);
        self.buf[self.pos] = v;
        let mut i = self.tree.len() / 2 + self.pos / 64;
        let next = if self.pos.is_multiple_of(64) {
            v.abs()
        } else {
            self.tree[i].max(v.abs())
        };
        if self.tree[i] != next {
            self.tree[i] = next;
            while i > 1 {
                i /= 2;
                let next = self.tree[i * 2].max(self.tree[i * 2 + 1]);
                if next == self.tree[i] {
                    break;
                }
                self.tree[i] = next;
            }
        }
        self.pos = (self.pos + 1) & (self.buf.len() - 1);
    }
    pub(super) fn clear(&mut self) {
        self.buf.fill(0.0);
        self.tree.fill(0.0);
    }
    pub(super) fn bytes(&self) -> usize {
        (self.buf.capacity() + self.tree.capacity()) * 4
    }
    fn blocks(&self, mut a: usize, mut b: usize) -> f32 {
        let n = self.tree.len() / 2;
        a += n;
        b += n;
        let mut out = 0.0_f32;
        while a < b {
            if a % 2 == 1 {
                out = out.max(self.tree[a]);
                a += 1;
            }
            if b % 2 == 1 {
                b -= 1;
                out = out.max(self.tree[b]);
            }
            a /= 2;
            b /= 2;
        }
        out
    }
    pub(super) fn activity(&self, age: f64) -> f32 {
        let n = self.buf.len();
        let age = (age.ceil() as usize + 2).min(n - 128);
        if age == 0 {
            return 0.0;
        }
        let a = self.pos.wrapping_sub(age) & (n - 1);
        if a < self.pos {
            self.blocks(a / 64, self.pos.div_ceil(64))
        } else {
            self.blocks(a / 64, n / 64)
                .max(self.blocks(0, self.pos.div_ceil(64)))
        }
    }
}

/// `b0 x + b1 x1 + a1 y1`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OnePole {
    pub(super) b0: f32,
    pub(super) b1: f32,
    pub(super) a1: f32,
    pub(super) x1: f32,
    pub(super) y1: f32,
}

impl OnePole {
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = ftz((self.b0 * x + self.b1 * self.x1) + self.a1 * self.y1);
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn clear(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// The feedback high pass: 10 Hz, always designed for 44100 Hz.
fn high_pass_10hz() -> OnePole {
    let w = (1.0f64 / 44100.0) as f32 * 3.141_592_7;
    let t = (f64::from(w) * 10.0).tan() as f32;
    let b0 = 1.0 / (t + 1.0);
    OnePole {
        b0,
        b1: -b0,
        a1: -(t - 1.0) / (t + 1.0),
        ..OnePole::default()
    }
}

/// The state-variable filter's coefficients.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SvfCoefs {
    pub(super) a1: f32,
    pub(super) a2: f32,
    pub(super) a3: f32,
    pub(super) m: [f32; 3],
    pub(super) off: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Svf {
    pub(super) c1: f32,
    pub(super) c2: f32,
}

impl Svf {
    #[inline]
    fn run(&mut self, x: f32, k: &SvfCoefs) -> f32 {
        if k.off {
            return x;
        }
        let v3 = x - self.c2;
        let v1 = self.c1 * k.a1 + k.a2 * v3;
        let v2 = (self.c1 * k.a2 + self.c2) + v3 * k.a3;
        self.c1 = ftz((v1 + v1) - self.c1);
        self.c2 = ftz((v2 + v2) - self.c2);
        (v1 * k.m[1] + x * k.m[0]) + v2 * k.m[2]
    }
}

/// The saturator's numbers for one side of zero.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Side {
    pub(super) knee: f32,
    pub(super) span: f32,
    pub(super) curve: f32,
}

impl Side {
    fn new(level: f32, hardness: f32) -> Side {
        let knee = level * hardness;
        let r = (level - knee).max(1e-11);
        Side {
            knee,
            span: r + r,
            curve: 0.25 / r,
        }
    }
    #[inline]
    fn run(&self, x: f32) -> f32 {
        if x < self.knee {
            return x;
        }
        let d = (x - self.knee).min(self.span);
        (1.0 - d * self.curve) * d + self.knee
    }
}

/// The saturator and the limiter's ceiling, from the drive, hardness and symmetry.
#[derive(Clone, Copy, Debug)]
pub struct Distortion {
    pub(super) thr: f32,
    pub(super) hardness: f32,
    pub(super) symmetry: f32,
    pub(super) gain: f32,
    pub(super) ceiling: f32,
    pub(super) pos: Side,
    pub(super) neg: Side,
}

impl Default for Distortion {
    fn default() -> Self {
        let mut d = Distortion {
            thr: 0.0,
            hardness: 0.0,
            symmetry: 0.0,
            gain: 1.0,
            ceiling: 1.0,
            pos: Side::default(),
            neg: Side::default(),
        };
        d.design();
        d
    }
}

impl Distortion {
    /// The drive setting's normalised value: the threshold, the make-up gain and the limiter's ceiling.
    pub fn set_drive(&mut self, x: f32) {
        let x = x * 0.95;
        self.thr = x * -30.0;
        self.design();
        let poly = (((((x * 2.9843) * x) * x) * x - ((x * 7.5177) * x) * x) + (x * 6.8819) * x
            - (x * 3.1787) * x)
            + 1.0;
        self.gain = (1.0 / f64::from(poly)).powf(f64::from(0.707f32)) as f32;
        self.ceiling = pow10_20(self.thr) * self.gain;
    }
    pub fn set_hardness(&mut self, x: f32) {
        self.hardness = x;
        self.design();
    }
    pub fn set_symmetry(&mut self, x: f32) {
        self.symmetry = x;
        self.design();
    }
    fn design(&mut self) {
        let top = pow10_20(self.thr);
        self.pos = Side::new((self.symmetry + 1.0) * top, self.hardness);
        self.neg = Side::new((1.0 - self.symmetry) * top, self.hardness);
    }
    /// The saturator: linear up to the knee, a parabola up to the side's level, then flat; times the gain.
    #[inline]
    pub fn saturate(&self, y: f32) -> f32 {
        let v = if y >= 0.0 {
            self.pos.run(y)
        } else {
            -self.neg.run(-y)
        };
        v * self.gain
    }
    pub fn ceiling(&self) -> f32 {
        self.ceiling
    }
    /// The settings' distortion.
    pub fn of(v: &[i32; PARAMS]) -> Distortion {
        let mut d = Distortion::default();
        d.set_drive(norm(DRIVE, v[DRIVE]));
        d.set_hardness(norm(HARDNESS, v[HARDNESS]));
        d.set_symmetry(norm(SYMMETRY, v[SYMMETRY]));
        d
    }
    /// The static curve the editor draws: the saturator, or the limiter as a clip at its ceiling.
    pub fn curve(&self, saturate: bool, x: f32) -> f32 {
        if saturate {
            self.saturate(x)
        } else {
            x.clamp(-self.ceiling, self.ceiling)
        }
    }
}

/// Per-side shaper state.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Shaper {
    pub(super) count: f32,
    pub(super) held: f32,
    pub(super) env: f32,
}

/// One set of the four delays: A (diffusion), then the reads (Mono: left and right; True stereo and Ping pong: the
/// echo delay C and the left and right input offsets).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Set {
    pub(super) target: [i32; 4],
    pub(super) live: [f64; 4],
}

// ---------------------------------------------------------------------------------------------------------------
// The delay

#[derive(Clone, Copy)]
pub(super) struct State {
    pub(super) rate: f64,
    pub(super) bpm: f64,
    pub(super) applied: [i32; PARAMS],
    pub(super) now: [f32; PARAMS],
    pub(super) target: [f32; PARAMS],
    pub(super) moving: [bool; PARAMS],
    pub(super) k: f32,
    pub(super) frame: u64,
    pub(super) input: f32,
    pub(super) kind: i32,
    pub(super) sync: bool,
    pub(super) keep_pitch: bool,
    pub(super) time: f32,
    pub(super) pan: f32,
    pub(super) w_ll: f32,
    pub(super) w_lr: f32,
    pub(super) w_rr: f32,
    pub(super) w_rl: f32,
    pub(super) glide: f64,
    pub(super) straight: f32,
    pub(super) crossed: f32,
    pub(super) lfo_speed: f32,
    pub(super) depth_setting: f32,
    pub(super) filter_sway: f32,
    pub(super) diff_gain: f32,
    pub(super) diff_size: f32,
    pub(super) saturate: bool,
    pub(super) feedback: f32,
    pub(super) filter: i32,
    pub(super) cutoff: f32,
    pub(super) resonance: f32,
    pub(super) dist: Distortion,
    pub(super) rate_amount: f32,
    pub(super) hold: f32,
    pub(super) steps: f32,
    pub(super) wet: f32,
    pub(super) colour: f32,
    pub(super) colour_hz: Option<(f32, bool)>,
    pub(super) dry: f32,
    pub(super) sets: [Set; 2],
    pub(super) gains: [f32; 2],
    pub(super) current: usize,
    pub(super) fading: bool,
    pub(super) fade_pos: u32,
    pub(super) fade_len: u32,
    pub(super) marked: bool,
    pub(super) phase: f32,
    pub(super) lfo_step: f32,
    pub(super) lfo_scale: f32,
    pub(super) depth: f32,
    pub(super) depth_c: f32,
    pub(super) lfo: f32,
    pub(super) hp: [OnePole; 2],
    pub(super) svf: [Svf; 2],
    pub(super) svf_k: SvfCoefs,
    pub(super) tone: [OnePole; 2],
    pub(super) shaper: [Shaper; 2],
    pub(super) attack: f32,
    pub(super) release: f32,
    pub(super) started: bool,
}
pub(super) struct Network {
    pub(super) state: State,
    pub(super) lines: [Ring; 2],
    pub(super) inputs: [Ring; 2],
    pub(super) allpass: [[Ring; 2]; 2],
}

fn capacity(seconds: f64, rate: f64) -> Result<usize, CommandError> {
    let frames = (seconds * rate + 128.0).ceil();
    if !frames.is_finite() || frames > 16_777_216.0 {
        return Err(limit());
    }
    (frames as usize)
        .max(256)
        .checked_next_power_of_two()
        .ok_or_else(limit)
}
impl Network {
    /// A new instance with the defaults at `sample_rate` (the reference host then sets the rate, which resets it).
    pub fn new(sample_rate: u32, model: i32) -> Result<Network, CommandError> {
        let rate = f64::from(sample_rate.max(1));
        let line = capacity(if model == OFF { 0.0 } else { 18.1 }, rate)?;
        let input = capacity(if model == TRUE_STEREO { 3.001 } else { 0.0 }, rate)?;
        let ap = capacity(655.0 / 44100.0, rate)?;
        let line2 = if model == MONO { 256 } else { line };
        let bytes = (line + line2 + input * 2 + ap * 4) * 4 * 33 / 32;
        if bytes > crate::rendering::MAX_CHAIN_BYTES {
            return Err(limit());
        }
        let mut d = Network {
            lines: [Ring::new(line)?, Ring::new(line2)?],
            inputs: [Ring::new(input)?, Ring::new(input)?],
            allpass: [
                [Ring::new(ap)?, Ring::new(ap)?],
                [Ring::new(ap)?, Ring::new(ap)?],
            ],
            state: State {
                rate,
                bpm: 120.0,
                applied: DEFAULTS,
                now: [0.0; PARAMS],
                target: [0.0; PARAMS],
                moving: [false; PARAMS],
                k: 0.0,
                frame: 0,
                input: 0.0,
                kind: 0,
                sync: false,
                keep_pitch: false,
                time: 0.0,
                pan: 0.5,
                w_ll: 1.0,
                w_lr: 0.0,
                w_rr: 1.0,
                w_rl: 0.0,
                glide: 1.0,
                straight: 1.0,
                crossed: 0.0,
                lfo_speed: 0.0,
                depth_setting: 0.0,
                filter_sway: 0.0,
                diff_gain: 0.0,
                diff_size: 0.0,
                saturate: false,
                feedback: 0.0,
                filter: 0,
                cutoff: 0.0,
                resonance: 0.0,
                dist: Distortion::default(),
                rate_amount: 0.0,
                hold: 1.0,
                steps: 0.0,
                wet: 1.0,
                colour: 0.5,
                colour_hz: None,
                dry: 1.0,
                sets: [Set::default(); 2],
                gains: [1.0, 0.0],
                current: 0,
                fading: false,
                fade_pos: 0,
                fade_len: 1,
                marked: false,
                phase: 0.0,
                lfo_step: 0.0,
                lfo_scale: 1.0,
                depth: 0.0,
                depth_c: 0.0,
                lfo: 0.5,
                hp: [high_pass_10hz(); 2],
                svf: [Svf::default(); 2],
                svf_k: SvfCoefs::default(),
                tone: [OnePole::default(); 2],
                shaper: [Shaper::default(); 2],
                attack: 1.0 - 0.1f64.powf(1.0 / 441.0) as f32,
                release: 1.0 - 0.1f64.powf(1.0 / 4410.0) as f32,
                started: false,
            },
        };
        d.rate_changed();
        for (k, value) in DEFAULTS.into_iter().enumerate() {
            let x = norm(k, value);
            d.state.now[k] = x;
            d.state.target[k] = x;
            d.derive(k);
        }
        let t = d.targets(&d.state.sets[0].target);
        d.state.sets = [Set {
            target: t,
            live: t.map(f64::from),
        }; 2];
        d.reset();
        Ok(d)
    }

    /// Applies the settings that differ from the ones applied last, in index order (each as a change of that setting).
    pub fn set(&mut self, s: &Settings) {
        for k in 0..PARAMS {
            if s.v[k] != self.state.applied[k] {
                self.apply(k, s.v[k]);
            }
        }
    }

    /// Applies setting `k` (smoothed settings start moving towards it).
    pub fn apply(&mut self, k: usize, v: i32) {
        self.state.applied[k] = v;
        let x = norm(k, v);
        if SMOOTHED[k] {
            self.state.target[k] = x;
            self.state.moving[k] = self.state.now[k] != x;
        } else {
            self.state.now[k] = x;
            self.state.target[k] = x;
            self.derive(k);
        }
    }

    /// The host's tempo (read once per block): a change marks the delays to be recomputed.
    pub fn set_tempo(&mut self, bpm: f64) {
        if bpm > 0.0 && bpm != self.state.bpm {
            self.state.bpm = bpm;
            self.state.marked = true;
        }
    }

    fn rate_changed(&mut self) {
        let rate = self.state.rate;
        self.state.k = (1.0 - 0.01f64.powf(1.0 / (rate * 0.005))) as f32;
        self.state.depth_c = (1.0 - 0.01f64.powf(1.0 / (0.1 * rate))) as f32;
        self.state.fade_len = ((0.1 * rate).trunc() as u32).max(1);
        for k in [MOD_RATE, SMOOTH, RATE_RED] {
            self.derive(k);
        }
        self.make_filter();
    }

    /// The derived value(s) of setting `k` from its current normalised value.
    fn derive(&mut self, k: usize) {
        let x = self.state.now[k];
        match k {
            INPUT => self.state.input = x,
            TYPE => {
                let t = four_way(x);
                if t != self.state.kind {
                    self.state.kind = t;
                    self.reset();
                }
                self.state.marked = true;
            }
            SYNC => {
                self.state.sync = x >= 0.5;
                self.state.marked = true;
            }
            KEEP_PITCH => {
                let on = x >= 0.5;
                if self.state.keep_pitch && !on {
                    self.state.marked = true;
                }
                self.state.keep_pitch = on;
            }
            TIME => {
                self.state.time = x;
                self.state.marked = true;
            }
            OFFSET => {
                self.state.pan = x;
                let t = (x + x) - 1.0;
                if t >= 0.0 {
                    self.state.w_ll = 1.0 - t;
                    self.state.w_lr = (1.0 - self.state.w_ll) * 0.5;
                    self.state.w_rr = 1.0 - self.state.w_lr;
                    self.state.w_rl = 0.0;
                } else {
                    self.state.w_rr = t + 1.0;
                    self.state.w_rl = (1.0 - self.state.w_rr) * 0.5;
                    self.state.w_ll = 1.0 - self.state.w_rl;
                    self.state.w_lr = 0.0;
                }
                self.state.marked = true;
            }
            SMOOTH => {
                self.state.glide = if x == 0.0 {
                    1.0
                } else {
                    1.0 - 0.01f64.powf(1.0 / (f64::from(x) * self.state.rate))
                }
            }
            SPREAD => {
                self.state.straight = x * 0.5 + 0.5;
                self.state.crossed = 1.0 - self.state.straight;
            }
            MOD_RATE => {
                self.state.lfo_speed = x * 0.99f32;
                let period = lfo_period(self.state.lfo_speed);
                self.state.lfo_step = 1.0 / (self.state.rate as f32 * period);
                // Over the slowest period as stored (single): 8.872 in double is a step off at some speeds.
                self.state.lfo_scale = (period / LFO_PERIODS[0]).sqrt();
            }
            MOD_TIME => self.state.depth_setting = (f64::from(x) * f64::from(x)) as f32 * 0.1,
            MOD_CUTOFF => self.state.filter_sway = x * 0.5,
            DIFF_LEVEL => self.state.diff_gain = x,
            DIFF_SPREAD => {
                self.state.diff_size = x;
                self.state.marked = true;
            }
            DIST_TYPE => self.state.saturate = (x + 0.5).trunc() != 0.0,
            FEEDBACK => self.state.feedback = x * 1.25,
            FILTER => self.state.filter = four_way(x),
            CUTOFF => self.state.cutoff = x,
            RESONANCE => self.state.resonance = x,
            DRIVE => self.state.dist.set_drive(x),
            HARDNESS => self.state.dist.set_hardness(x),
            SYMMETRY => self.state.dist.set_symmetry(x),
            RATE_RED => {
                let t = one_minus(x);
                self.state.rate_amount = t * (t * t);
                self.state.hold = (hold_period(self.state.rate_amount)
                    * (self.state.rate / 44100.0) as f32)
                    .max(1.0);
            }
            BITS => {
                self.state.steps = if x >= 1.0 {
                    0.0
                } else {
                    let t = one_minus(x);
                    let q = (t * t).sqrt().sqrt();
                    2f64.powf(f64::from((1.0 - q) * 14.0 + 2.0)) as f32
                };
            }
            WET => self.state.wet = x,
            TONE => self.set_colour(x),
            DRY => self.state.dry = x,
            _ => {}
        }
    }

    fn set_colour(&mut self, c: f32) {
        self.state.colour = c;
        if c < 0.5 {
            self.design_colour(lookup(&LP, (c + c) * 3.999_900_1), false);
        } else if c > 0.5 {
            self.design_colour(lookup(&HP, ((c - 0.5) + (c - 0.5)) * 3.999_900_1), true);
        } else {
            self.state.colour_hz = None;
            for t in &mut self.state.tone {
                (t.b0, t.b1, t.a1) = (1.0, 0.0, 0.0);
            }
        }
    }

    fn design_colour(&mut self, hz: f32, high: bool) {
        let hz = if hz as f64 >= self.state.rate * 0.5 {
            (self.state.rate * 0.499999) as f32
        } else {
            hz
        };
        self.state.colour_hz = Some((hz, high));
        let w = (1.0 / self.state.rate) as f32 * 3.141_592_7;
        let t = (f64::from(hz) * f64::from(w)).tan() as f32;
        let (b0, b1) = if high {
            let b0 = 1.0 / (t + 1.0);
            (b0, -b0)
        } else {
            let b = t / (t + 1.0);
            (b, b)
        };
        for p in &mut self.state.tone {
            (p.b0, p.b1, p.a1) = (b0, b1, -(t - 1.0) / (t + 1.0));
        }
    }

    /// The filter's coefficients from the cutoff, its LFO sway and the resonance.
    fn make_filter(&mut self) {
        let at = (self.state.filter_sway * self.state.lfo + self.state.cutoff).clamp(0.0, 1.0);
        let q = self.state.resonance.clamp(0.0, 1.0) * 0.5;
        let q = q * q;
        let (hz, k, m) = match self.state.filter {
            LOW_PASS => {
                let k = 1.0 / (q * 4.0 + 0.707);
                (lookup(&LP, at * 3.999), k, [0.0, 0.0, 1.0])
            }
            HIGH_PASS => {
                let k = 1.0 / (q * 4.0 + 0.707);
                (lookup(&HP, at * 3.999), k, [1.0, -k, -1.0])
            }
            BAND_PASS => {
                let k = 1.0 / (q * 4.6 + 0.107);
                (lookup(&LP, at * 3.6), k, [0.0, k, 0.0])
            }
            _ => {
                self.state.svf_k.off = true;
                return;
            }
        };
        let hz = hz.min((0.49 * self.state.rate) as f32);
        let w = (3.1415 / self.state.rate) as f32;
        // The product in single, the tangent in double.
        let g = f64::from(hz * w).tan() as f32;
        let a1 = 1.0 / ((g + k) * g + 1.0);
        let a2 = a1 * g;
        self.state.svf_k = SvfCoefs {
            a1,
            a2,
            a3: a2 * g,
            m,
            off: false,
        };
    }

    /// The four delays' integer targets for the current settings (Off makes only A and keeps the rest).
    fn targets(&self, old: &[i32; 4]) -> [i32; 4] {
        let sec = delay_seconds(self.state.time, self.state.sync, self.state.bpm);
        let d = (sec * self.state.rate).trunc() as i32;
        let df = d as f32;
        let rate = self.state.rate as f32;
        let scale = (self.state.rate / 44100.0) as f32;
        let a = ((df / (rate * 0.015)).min(1.0) * (scale * (self.state.diff_size * 590.0 + 65.0)))
            .max(1.0)
            .trunc() as i32;
        let mut t = *old;
        t[0] = a;
        let w = ((self.state.pan - 0.5).abs() + (self.state.pan - 0.5).abs()) * 0.5;
        match self.state.kind {
            MONO => {
                let ra = (df - a as f32).max(1.0).trunc() as i32;
                let rb = ((df - (1.0 - w) * df) + ra as f32).trunc() as i32;
                (t[1], t[2]) = if self.state.pan < 0.5 {
                    (rb, ra)
                } else {
                    (ra, rb)
                };
            }
            TRUE_STEREO | PING_PONG => {
                t[1] = (df - a as f32).max(1.0).trunc() as i32;
                let o = ((w * df) * w).max(1.0).trunc() as i32;
                (t[2], t[3]) = if self.state.pan >= 0.5 {
                    (o, 1)
                } else {
                    (1, o)
                };
            }
            _ => {}
        }
        t
    }

    /// Empties the lines and filters; the live delays take their targets, then the targets are made again.
    pub fn reset(&mut self) {
        for r in self.lines.iter_mut().chain(self.inputs.iter_mut()) {
            r.clear();
        }
        for side in &mut self.allpass {
            side.iter_mut().for_each(Ring::clear);
        }
        self.state.svf = [Svf::default(); 2];
        self.state.hp.iter_mut().for_each(OnePole::clear);
        for s in &mut self.state.shaper {
            s.held = 0.0;
        }
        for s in &mut self.state.sets {
            s.live = s.target.map(f64::from);
        }
        for i in 0..2 {
            self.state.sets[i].target = self.targets(&self.state.sets[i].target);
        }
    }

    /// One smoothing step of every moving setting.
    fn smooth(&mut self) {
        for k in 0..PARAMS {
            if !self.state.moving[k] {
                continue;
            }
            let mut n =
                self.state.now[k] + (self.state.target[k] - self.state.now[k]) * self.state.k;
            if (n - self.state.target[k]).abs() < 0.0001 {
                n = self.state.target[k];
                self.state.moving[k] = false;
            }
            self.state.now[k] = n;
            self.derive(k);
        }
    }

    /// The delay times before the frame: glide (Keep pitch off) or crossfade (on).
    fn update_times(&mut self) {
        if !self.state.keep_pitch {
            self.state.gains = [1.0, 0.0];
            if self.state.marked {
                self.state.marked = false;
                self.state.sets[0].target = self.targets(&self.state.sets[0].target);
            }
            let g = self.state.glide;
            let s = &mut self.state.sets[0];
            for i in 0..4 {
                let (t, l) = (f64::from(s.target[i]), s.live[i]);
                if l != t {
                    s.live[i] = if (t - l).abs() < 0.001 {
                        t
                    } else {
                        (t - l) * g + l
                    };
                }
            }
        } else if self.state.marked && !self.state.fading {
            self.state.marked = false;
            let o = 1 - self.state.current;
            let t = self.targets(&self.state.sets[o].target);
            self.state.sets[o] = Set {
                target: t,
                live: t.map(f64::from),
            };
            self.state.fading = true;
            self.state.fade_pos = 0;
        }
    }

    /// After the frame: the crossfade moves on.
    fn advance_fade(&mut self) {
        if !self.state.keep_pitch || !self.state.fading {
            return;
        }
        let t = self.state.fade_pos as f32 * (1.0 / self.state.fade_len as f32);
        let o = 1 - self.state.current;
        self.state.gains[self.state.current] = 1.0 - t;
        self.state.gains[o] = t;
        self.state.fade_pos += 1;
        if self.state.fade_pos >= self.state.fade_len {
            self.state.current = o;
            self.state.fading = false;
            self.state.fade_pos = 0;
        }
    }

    /// Both sets' reads of `ring` at their delay `i` (plus a sway in frames), added by their gains.
    #[inline]
    fn tap(&self, ring: &Ring, i: usize, sway: f32, mono: bool) -> f32 {
        let d = |s: &Set| {
            if mono {
                s.live[i] as f32 + sway
            } else {
                (f64::from(sway) + s.live[i]) as f32
            }
        };
        ring.read(d(&self.state.sets[0])) * self.state.gains[0]
            + ring.read(d(&self.state.sets[1])) * self.state.gains[1]
    }

    /// The diffusion all-passes of `side`, one per set, added by the sets' gains.
    #[inline]
    fn diffuse(&mut self, side: usize, x: f32) -> f32 {
        let h = self.state.diff_gain * 0.499;
        let mut out = [0.0f32; 2];
        for (s, o) in out.iter_mut().enumerate() {
            let ring = &mut self.allpass[side][s];
            let z = ring.read(self.state.sets[s].live[0] as f32);
            ring.write(x - z * h);
            *o = (1.0 - h * h) * z + h * x;
        }
        out[1] * self.state.gains[1] + out[0] * self.state.gains[0]
    }

    /// Rate reduction, bit reduction, then the saturator or the limiter.
    #[inline]
    fn shape(&mut self, side: usize, x: f32) -> f32 {
        let s = &mut self.state.shaper[side];
        let mut y = x;
        if self.state.rate_amount > 0.0 {
            s.count += 1.0;
            if s.count >= self.state.hold {
                s.held = y;
                if s.count < 0.0 || self.state.hold <= 0.0 {
                    s.count = 0.0;
                } else {
                    let r = s.count / self.state.hold;
                    #[allow(clippy::assign_op_pattern)] // Retain reference operand order.
                    {
                        s.count = (r - r.trunc()) * s.count;
                    }
                }
            } else {
                y = s.held;
            }
        }
        if self.state.steps > 0.0 {
            y = (self.state.steps * y).trunc() / self.state.steps;
        }
        if self.state.saturate {
            self.state.dist.saturate(y)
        } else {
            let mut v = y / self.state.dist.ceiling;
            let a = v.abs();
            let d = a - s.env;
            s.env = ftz(s.env
                + d * if a >= s.env {
                    self.state.attack
                } else {
                    self.state.release
                });
            if s.env > 1.0 {
                v /= s.env;
            }
            self.state.dist.ceiling * v
        }
    }

    #[inline]
    fn out_mix(&self, yl: f32, yr: f32, il: f32, ir: f32) -> (f32, f32) {
        let l = (self.state.crossed * yr + self.state.straight * yl) * self.state.wet
            + (il * self.state.dry) * 1.0;
        let r = (self.state.crossed * yl + self.state.straight * yr) * self.state.wet
            + (ir * self.state.dry) * 1.0;
        (l, r)
    }

    /// One stereo frame.
    #[inline]
    pub fn frame(&mut self, il: f32, ir: f32) -> (f32, f32) {
        let il = if il.abs() <= 9.999_999_8e-14 { 0.0 } else { il };
        let ir = if ir.abs() <= 9.999_999_8e-14 { 0.0 } else { ir };
        if self.state.frame.is_multiple_of(8) {
            self.smooth();
        }
        self.update_times();
        let old = self.state.phase;
        let p = old + self.state.lfo_step;
        self.state.phase = p - p.trunc();
        // On Windows, `f32::sin` uses the C runtime's `sinf`, matching the reference values.
        self.state.lfo = ((old * 6.283_185_5).sin() + 1.0) * 0.5;
        if self.state.kind != OFF {
            let half = 0.5 * self.state.rate as f32;
            self.state.depth = ftz(((self.state.depth_setting * half) * self.state.lfo_scale
                - self.state.depth)
                * self.state.depth_c
                + self.state.depth);
        }
        if self.state.frame.is_multiple_of(4) {
            self.make_filter();
        }
        let sway = (self.state.lfo + 0.5) * self.state.depth;
        let out = match self.state.kind {
            MONO => {
                let m = ((il + ir) * 0.5) * self.state.input;
                let e1 = self.tap(&self.lines[0], 1, sway, true);
                let e2 = self.tap(&self.lines[0], 2, sway, true);
                let feed = if self.state.pan < 0.5 { e2 } else { e1 };
                let x = self.state.hp[0].run(self.state.feedback * feed) + m;
                let x = self.shape(0, x);
                let x = self.state.svf[0].run(x, &self.state.svf_k);
                let x = self.diffuse(0, x);
                self.lines[0].write(x);
                let yl = self.state.tone[0].run(e1);
                let yr = self.state.tone[1].run(e2);
                self.out_mix(yl, yr, il, ir)
            }
            TRUE_STEREO => {
                let e = [
                    self.tap(&self.lines[0], 1, sway, false),
                    self.tap(&self.lines[1], 1, sway, false),
                ];
                let inp = [il, ir];
                let mut x = [0.0f32; 2];
                for s in 0..2 {
                    self.inputs[s].write(inp[s]);
                    let n = self.tap(&self.inputs[s], 2 + s, 0.0, false);
                    let v = n * self.state.input + self.state.hp[s].run(e[s] * self.state.feedback);
                    let v = self.state.svf[s].run(v, &self.state.svf_k);
                    let v = self.shape(s, v);
                    x[s] = self.diffuse(s, v);
                }
                self.lines[0].write(x[0]);
                self.lines[1].write(x[1]);
                let yl = self.state.tone[0].run(e[0]);
                let yr = self.state.tone[1].run(e[1]);
                self.out_mix(yl, yr, il, ir)
            }
            PING_PONG => {
                let e = [
                    self.tap(&self.lines[0], 1, sway, false),
                    self.tap(&self.lines[1], 1, sway, false),
                ];
                let to = [
                    ir * self.state.w_rl + il * self.state.w_ll,
                    ir * self.state.w_rr + il * self.state.w_lr,
                ];
                let mut x = [0.0f32; 2];
                for s in 0..2 {
                    let v =
                        to[s] * self.state.input + self.state.hp[s].run(e[s] * self.state.feedback);
                    let v = self.state.svf[s].run(v, &self.state.svf_k);
                    let v = self.shape(s, v);
                    x[s] = self.diffuse(s, v);
                }
                // Each side feeds the other's line.
                self.lines[1].write(x[0]);
                self.lines[0].write(x[1]);
                let y = [self.state.tone[0].run(e[0]), self.state.tone[1].run(e[1])];
                let mut o = [
                    self.state.straight * y[0] + self.state.crossed * y[1],
                    self.state.straight * y[1] + self.state.crossed * y[0],
                ];
                if self.state.saturate {
                    for (s, o) in o.iter_mut().enumerate() {
                        let sh = &mut self.state.shaper[s];
                        let a = o.abs();
                        let d = a - sh.env;
                        sh.env = ftz(sh.env
                            + d * if a >= sh.env {
                                self.state.attack
                            } else {
                                self.state.release
                            });
                        if sh.env > 1.0 {
                            *o /= sh.env;
                        }
                    }
                }
                (
                    (il * self.state.dry) * 1.0 + o[0] * self.state.wet,
                    (ir * self.state.dry) * 1.0 + o[1] * self.state.wet,
                )
            }
            _ => {
                let mut y = [0.0f32; 2];
                for (s, v) in [il, ir].into_iter().enumerate() {
                    let f = self.state.svf[s].run(v, &self.state.svf_k);
                    let sh = self.shape(s, f);
                    y[s] = self.state.tone[s].run(sh);
                }
                (
                    (self.state.crossed * y[1] + self.state.straight * y[0]) * self.state.wet
                        + self.state.dry * il,
                    (self.state.crossed * y[0] + self.state.straight * y[1]) * self.state.wet
                        + self.state.dry * ir,
                )
            }
        };
        self.advance_fade();
        self.state.frame += 1;
        out
    }

    /// Runs `frames` of silence through the delay.
    pub fn preroll(&mut self, frames: usize) {
        for _ in 0..frames {
            self.frame(0.0, 0.0);
        }
    }

    /// What the reference host runs between loading a slot and the song: silence, then two resets.
    pub fn start(&mut self) {
        self.preroll(PREROLL);
        self.reset();
        self.reset();
        self.state.started = true;
    }
}
