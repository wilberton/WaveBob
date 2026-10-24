//! WaveBob processor constructors and shared reference-derived primitives.
use crate::{
    CommandError, ErrorCode,
    processing::{EffectKind, Instance},
};
pub mod soft_clipper;
pub(crate) fn prepare(
    instance: &Instance,
    rate: u32,
    _tempo: f64,
) -> Result<Box<dyn crate::rendering::Processor>, CommandError> {
    match instance.kind {
        EffectKind::SoftClipper => Ok(Box::new(soft_clipper::SoftClipper::new(instance, rate))),
        _ => Err(CommandError::new(
            ErrorCode::MissingCapability,
            "Processor is not implemented",
        )),
    }
}
