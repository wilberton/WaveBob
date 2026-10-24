//! Test-only pinned reference; never a WaveBob production dependency.
#![allow(dead_code)]
pub trait Effect {
    fn process(&mut self, samples: &mut [f32]);
    fn reset(&mut self) {}
    fn set_tempo(&mut self, _bpm: f64) {}
}
#[rustfmt::skip]
#[path = "../../pinned/delay.rs"]
pub mod delay;
