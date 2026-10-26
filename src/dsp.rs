//! WaveBob processor constructors and shared reference-derived primitives.
use crate::{
    CommandError,
    processing::{EffectKind, Instance},
};
pub mod balance;
pub mod chorus;
pub mod delay;
pub mod fast_distortion;
pub mod limiter;
pub mod parametric_eq;
pub mod reverb;
pub mod soft_clipper;
pub mod stereo_enhancer;
pub mod stereo_shaper;
pub mod transient_processor;
pub(crate) fn prepare(
    instance: &Instance,
    rate: u32,
    tempo: f64,
) -> Result<Box<dyn crate::rendering::Processor>, CommandError> {
    match instance.kind {
        EffectKind::TransientProcessor => Ok(Box::new(
            transient_processor::TransientProcessor::new(instance, rate),
        )),
        EffectKind::StereoShaper => Ok(Box::new(stereo_shaper::StereoShaper::new(instance, rate)?)),
        EffectKind::StereoEnhancer => Ok(Box::new(stereo_enhancer::StereoEnhancer::new(
            instance, rate,
        )?)),
        EffectKind::Balance => Ok(Box::new(balance::Balance::new(instance, rate))),
        EffectKind::Limiter => Ok(Box::new(limiter::Limiter::new(instance, rate)?)),
        EffectKind::Chorus => Ok(Box::new(chorus::Chorus::new(instance, rate)?)),
        EffectKind::Delay => Ok(Box::new(delay::Delay::new(instance, rate, tempo)?)),
        EffectKind::FastDistortion => Ok(Box::new(fast_distortion::FastDistortion::new(
            instance, rate,
        )?)),
        EffectKind::ParametricEq => Ok(Box::new(parametric_eq::ParametricEq::new(instance, rate))),
        EffectKind::Reverb => Ok(Box::new(reverb::Reverb::new(instance, rate, tempo)?)),
        EffectKind::SoftClipper => Ok(Box::new(soft_clipper::SoftClipper::new(instance, rate))),
    }
}
