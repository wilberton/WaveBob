//! Explicit structural edit requests and the internal audio clipboard read model.
use crate::effects::EditTarget;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditParams {
    pub target: EditTarget,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteParams {
    pub document_id: String,
    pub revision_id: String,
    pub start_frame: u64,
    pub end_frame: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct ClipboardInfo {
    pub frames: u64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub source_revision_id: Option<String>,
}
