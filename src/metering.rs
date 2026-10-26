//! Bounded, copyable observations of actual processor input/output and gain.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Column {
    pub input: f32,
    pub output: f32,
    pub level: f32,
    pub gain: f32,
}
/// One source-rate block accumulated without allocating in the audio path.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Block {
    pub frames: u64,
    pub sample_rate: u32,
    pub levels: Column,
}
/// One callback observation; sequence is monotonic within this transport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Observation {
    pub sequence: u64,
    pub frames: u64,
    pub sample_rate: u32,
    pub levels: Column,
}
/// Bounded input/output/detector/gain history for one actual audible instance.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct History {
    pub instance_id: String,
    pub bypassed: bool,
    pub columns: Vec<Observation>,
}
