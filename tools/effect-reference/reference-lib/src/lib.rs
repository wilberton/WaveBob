//! Test-only pinned reference; never a WaveBob production dependency.
#![allow(dead_code)]
pub trait Effect {
    fn process(&mut self, samples: &mut [f32]);
    fn reset(&mut self) {}
    fn extra(&self) -> Option<(usize, &[f32])> {
        None
    }
    fn set_tempo(&mut self, _bpm: f64) {}
}
#[rustfmt::skip]
#[path = "../../pinned/delay.rs"]
pub mod delay;

#[rustfmt::skip]
#[path = "../../pinned/chorus.rs"]
pub mod chorus;
fn field(bytes: &[u8], index: usize) -> Option<i32> {
    bytes
        .get(index * 4..index * 4 + 4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
}
fn ints<const N: usize>(bytes: &[u8], index: usize) -> Option<[i32; N]> {
    let mut out = [0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = field(bytes, index + i)?;
    }
    Some(out)
}
fn state_of(values: &[i32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

#[rustfmt::skip]
#[path = "../../pinned/dynamics.rs"]
pub mod dynamics;

extern crate self as os_dsp;
#[rustfmt::skip]
#[path = "../../pinned/glide.rs"]
pub mod glide;
#[rustfmt::skip]
#[path = "../../pinned/balance.rs"]
pub mod balance;

#[rustfmt::skip]
#[path = "../../pinned/level.rs"]
pub mod level;
#[rustfmt::skip]
#[path = "../../pinned/stereo_enhancer.rs"]
pub mod stereo_enhancer;

#[rustfmt::skip]
#[path = "../../pinned/stereo_shaper.rs"]
pub mod stereo_shaper;

#[rustfmt::skip]
#[path = "../../pinned/transient_processor.rs"]
pub mod transient_processor;
