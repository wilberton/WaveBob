//! Device-independent mix inspection and preparation commands.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareMixParams {
    pub revision_id: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MixSamplesParams {
    pub revision_id: String,
    #[serde(default)]
    pub start_frame: u64,
    pub frames: u32,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MixSeekParams {
    pub frame: u64,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MasterGainParams {
    pub gain_db: f64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct MixPrepared {
    pub render_identity: String,
    pub revision_id: String,
    pub frames: u64,
    pub sample_rate: u32,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct EffectiveTrack {
    pub track_id: String,
    pub audible: bool,
    pub gain_db: f64,
    pub pan: f64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct MixState {
    pub required_render_identity: String,
    pub prepared_render_identity: Option<String>,
    pub bounce_cache_bytes: u64,
    pub bounce_cache_entries: usize,
    pub arrangement_id: String,
    pub revision_id: String,
    pub prepared_revision_id: Option<String>,
    pub pending_job_id: Option<String>,
    pub preparation_error: Option<crate::CommandError>,
    pub requested_playing: bool,
    pub peak_level: f64,
    pub overloaded: bool,
    pub master_gain_db: f64,
    pub tracks: Vec<EffectiveTrack>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct MixSamples {
    pub render_identity: String,
    pub revision_id: String,
    pub sample_rate: u32,
    pub start_frame: u64,
    pub frames: u32,
    pub channels: Vec<Vec<f64>>,
    pub peak_level: f64,
    pub overloaded_samples: u64,
}
