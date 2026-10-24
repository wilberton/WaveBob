//! Exact arrangement coordinates and read models, independent of any renderer.
use crate::{CommandError, ErrorCode, documents::FrameRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_TIMELINE_FRAME: u64 = 48_000 * 60 * 60 * 24;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Arrangement {
    pub revision_id: String,
    pub sample_rate: u32,
    pub tracks: Vec<Track>,
    pub cursor: u64,
    pub selection: Option<FrameRange>,
    pub selected_clip_ids: Vec<String>,
    pub dirty: bool,
    pub saved_revision_id: Option<String>,
}
impl Default for Arrangement {
    fn default() -> Self {
        let revision_id = uuid::Uuid::new_v4().to_string();
        Self {
            saved_revision_id: Some(revision_id.clone()),
            revision_id,
            sample_rate: 48_000,
            tracks: Vec::new(),
            cursor: 0,
            selection: None,
            selected_clip_ids: Vec::new(),
            dirty: false,
        }
    }
}
impl Arrangement {
    pub fn frames(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|track| &track.clips)
            .map(|clip| clip.position + clip.frames)
            .max()
            .unwrap_or(0)
    }
    pub fn clip(&self, id: &str) -> Option<(&Track, &Clip)> {
        self.tracks.iter().find_map(|track| {
            track
                .clips
                .iter()
                .find(|clip| clip.id == id)
                .map(|clip| (track, clip))
        })
    }
    pub(crate) fn retained_revisions(&self) -> impl Iterator<Item = &String> {
        self.tracks
            .iter()
            .flat_map(|track| &track.clips)
            .map(|clip| &clip.revision_id)
    }
    pub(crate) fn validate(&self) -> Result<(), CommandError> {
        if self.tracks.len() > 256
            || self.tracks.iter().map(|t| t.clips.len()).sum::<usize>() > 10_000
        {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Arrangement limit: 256 tracks and 10000 clips",
            ));
        }
        for track in &self.tracks {
            for (i, a) in track.clips.iter().enumerate() {
                if a.position
                    .checked_add(a.frames)
                    .is_none_or(|end| end > MAX_TIMELINE_FRAME)
                {
                    return Err(invalid("Clip exceeds the 24 hour timeline limit"));
                }
                if track.clips.iter().skip(i + 1).any(|b| {
                    a.position < b.position.saturating_add(b.frames)
                        && b.position < a.position.saturating_add(a.frames)
                }) {
                    return Err(invalid(
                        "Overlapping clips are unavailable until automatic crossfades are implemented",
                    ));
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub mute: bool,
    pub solo: bool,
    pub gain_db: f64,
    pub pan: f64,
    pub clips: Vec<Clip>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub document_id: String,
    pub revision_id: String,
    pub source_start: u64,
    pub source_end: u64,
    pub source_rate: u32,
    pub channels: u16,
    pub position: u64,
    /// Duration rounded once to the nearest project frame, minimum one frame.
    pub frames: u64,
}
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddTrackParams {
    pub name: Option<String>,
    pub index: Option<usize>,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackParams {
    pub track_id: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateTrackParams {
    pub track_id: String,
    pub name: Option<String>,
    pub index: Option<usize>,
    pub mute: Option<bool>,
    pub solo: Option<bool>,
    pub gain_db: Option<f64>,
    pub pan: Option<f64>,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InsertClipParams {
    pub track_id: String,
    pub document_id: String,
    pub revision_id: String,
    pub source_start: u64,
    pub source_end: u64,
    pub position: u64,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveClipParams {
    pub clip_id: String,
    pub track_id: String,
    pub position: u64,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipParams {
    pub clip_id: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipWaveformParams {
    pub clip_id: String,
    pub bins: usize,
}
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NavigateParams {
    pub cursor: u64,
    pub selection: Option<FrameRange>,
    #[serde(default)]
    pub selected_clip_ids: Vec<String>,
}
pub(crate) fn invalid(message: &str) -> CommandError {
    CommandError::new(ErrorCode::InvalidParams, message)
}
