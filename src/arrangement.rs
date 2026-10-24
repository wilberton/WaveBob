//! Exact arrangement coordinates and read models, independent of any renderer.
use crate::{CommandError, ErrorCode, documents::FrameRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_TIMELINE_FRAME: u64 = 48_000 * 60 * 60 * 24;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Arrangement {
    pub id: String,
    pub revision_id: String,
    pub master_gain_db: f64,
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
            id: uuid::Uuid::new_v4().to_string(),
            master_gain_db: 0.0,
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
            .flat_map(|clip| {
                std::iter::once(&clip.revision_id).chain(clip.processed_revision_id.iter())
            })
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
    pub edit_revision_id: String,
    pub processed_revision_id: Option<String>,
    pub effects: Vec<crate::effects::Effect>,
    pub source_frames: u64,
    pub source_start: u64,
    pub source_end: u64,
    pub source_rate: u32,
    pub channels: u16,
    pub position: u64,
    /// Duration rounded once to the nearest project frame, minimum one frame.
    pub frames: u64,
}
impl Clip {
    /// Preserve the opposite edge in project time, using nearest-frame rounding.
    pub fn trimmed(&self, start: u64, end: u64, project_rate: u32) -> Result<Self, CommandError> {
        if start >= end || end > self.source_frames || self.source_rate == 0 || project_rate == 0 {
            return Err(invalid("Clip source interval is out of range"));
        }
        let delta = start as i128 - self.source_start as i128;
        let shift = (delta.abs() * project_rate as i128 + self.source_rate as i128 / 2)
            / self.source_rate as i128
            * delta.signum();
        let position = self.position as i128 + shift;
        if position < 0 || position > MAX_TIMELINE_FRAME as i128 {
            return Err(invalid("Trimming the left edge would leave the timeline"));
        }
        let mut clip = self.clone();
        clip.position = position as u64;
        clip.source_start = start;
        clip.source_end = end;
        clip.frames =
            crate::audio::resample::frame_count(end - start, self.source_rate, project_rate);
        if clip
            .position
            .checked_add(clip.frames)
            .is_none_or(|end| end > MAX_TIMELINE_FRAME)
        {
            return Err(invalid("Clip exceeds the timeline limit"));
        }
        Ok(clip)
    }
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
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrimClipParams {
    pub clip_id: String,
    pub edit_revision_id: String,
    pub source_start: u64,
    pub source_end: u64,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipEffectParams {
    pub clip_id: String,
    pub edit_revision_id: String,
    pub effect: crate::effects::Effect,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipSamplesParams {
    pub clip_id: String,
    #[serde(default)]
    pub start_frame: u64,
    pub frames: u32,
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
