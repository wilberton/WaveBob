//! Versioned portable projects, managed lossless assets and whole-workspace save state.
use crate::{
    arrangement::Arrangement,
    documents::{DocumentInfo, SourceDetails},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub(crate) mod storage;

pub const SCHEMA_VERSION: u32 = 1;
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct ProjectInfo {
    pub path: Option<String>,
    pub revision_id: String,
    pub saved_revision_id: Option<String>,
    pub dirty: bool,
    pub saved_document_revisions: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveProjectParams {
    pub path: String,
    #[serde(default)]
    pub overwrite: bool,
    pub expected_project_revision: Option<String>,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenProjectParams {
    pub path: String,
    #[serde(default)]
    pub discard_changes: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct ProjectSummary {
    pub path: String,
    pub revision_id: String,
    pub schema_version: u32,
    pub documents: usize,
    pub tracks: usize,
    pub clips: usize,
    pub assets: usize,
    pub assets_written: usize,
    pub assets_reused: usize,
    pub managed_assets_path: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StateKey {
    pub documents: Vec<DocumentKey>,
    pub arrangement_revision_id: String,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct DocumentKey {
    pub id: String,
    pub revision_id: String,
    path: String,
    name: String,
    saved_revision_id: Option<String>,
    save_format: Option<crate::exports::ExportFormat>,
    save_sample_rate: Option<u32>,
}
impl StateKey {
    pub fn new(documents: &[DocumentInfo], arrangement: &Arrangement) -> Self {
        Self {
            arrangement_revision_id: arrangement.revision_id.clone(),
            documents: documents
                .iter()
                .map(|doc| DocumentKey {
                    id: doc.id.clone(),
                    revision_id: doc.revision_id.clone(),
                    path: doc.path.clone(),
                    name: doc.name.clone(),
                    saved_revision_id: doc.saved_revision_id.clone(),
                    save_format: doc.save_format.clone(),
                    save_sample_rate: doc.save_sample_rate,
                })
                .collect(),
        }
    }
    pub fn revision(&self) -> String {
        checksum(self)
    }
}
pub(crate) fn checksum(value: &impl Serialize) -> String {
    // Value normalizes object ordering, independent of manifest pretty printing.
    let value = serde_json::to_value(value).expect("finite validated project data");
    format!("{:x}", Sha256::digest(serde_json::to_vec(&value).unwrap()))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectFile {
    schema_version: u32,
    checksum: String,
    payload: Payload,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Payload {
    pub loop_enabled: bool,
    pub documents: Vec<DocumentInfo>,
    pub active_document_id: Option<String>,
    pub arrangement: Arrangement,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Asset {
    pub revision_id: String,
    pub file: String,
    pub checksum: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub encoding: String,
    pub source_details: SourceDetails,
}
