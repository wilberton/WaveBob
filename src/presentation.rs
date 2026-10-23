use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    #[default]
    Waveform,
    Multitrack,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct PresentationSnapshot {
    pub revision: u64,
    pub mode: ViewMode,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetModeParams {
    pub mode: ViewMode,
    pub expected_presentation_revision: Option<u64>,
}

pub fn default_timeout_ms() -> u64 {
    10_000
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderParams {
    pub after_revision: Option<u64>,
    pub after_presentation_revision: Option<u64>,
    #[serde(default = "default_timeout_ms")]
    #[schemars(range(min = 1, max = 60_000))]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureParams {
    pub path: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
    pub after_revision: Option<u64>,
    pub after_presentation_revision: Option<u64>,
    #[serde(default = "default_timeout_ms")]
    #[schemars(range(min = 1, max = 60_000))]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct RenderMetadata {
    pub session_id: String,
    pub rendered_revision: u64,
    pub presentation_revision: u64,
    pub mode: ViewMode,
    pub frame_number: u64,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub pending_resources: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct CaptureResult {
    pub path: String,
    pub mime_type: String,
    pub metadata: RenderMetadata,
}
