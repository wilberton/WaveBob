//! Exact arrangement coordinates and read models, independent of any renderer.
use crate::{CommandError, ErrorCode, documents::FrameRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_TIMELINE_FRAME: u64 = 48_000 * 60 * 60 * 24;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Arrangement {
    pub effect_tempo: f64,
    pub id: String,
    pub revision_id: String,
    pub master_gain_db: f64,
    pub next_clip_order: u64,
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
            effect_tempo: 120.0,
            id: uuid::Uuid::new_v4().to_string(),
            master_gain_db: 0.0,
            next_clip_order: 0,
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
            let mut edges = Vec::with_capacity(track.clips.len() * 2);
            for a in &track.clips {
                if a.frames == 0 {
                    return Err(invalid("A clip must contain at least one project frame"));
                }
                if a.position
                    .checked_add(a.frames)
                    .is_none_or(|end| end > MAX_TIMELINE_FRAME)
                {
                    return Err(invalid("Clip exceeds the 24 hour timeline limit"));
                }
                edges.push((a.position, 1i32));
                edges.push((a.position + a.frames, -1));
            }
            edges.sort_unstable();
            let mut active = 0;
            for (_, delta) in edges {
                active += delta;
                if active > 2 {
                    return Err(invalid(
                        "A track supports at most two simultaneous clips. Move this clip or use another track.",
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
    pub processing: crate::processing::Processing,
    #[serde(default)]
    pub linked: bool,
    #[serde(default)]
    pub full_document: bool,
    pub id: String,
    pub name: String,
    pub document_id: String,
    pub revision_id: String,
    pub edit_revision_id: String,
    pub insertion_order: u64,
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
        clip.full_document = false;
        clip.processing.envelope.fade_in_frames =
            clip.processing.envelope.fade_in_frames.min(end - start);
        clip.processing.envelope.fade_out_frames =
            clip.processing.envelope.fade_out_frames.min(end - start);
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
pub struct DuplicateClipParams {
    pub clip_id: String,
    pub track_id: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrossfadeKind {
    Staggered,
    Contained,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct CrossfadeRegion {
    pub track_id: String,
    pub first_clip_id: String,
    pub second_clip_id: String,
    pub start: u64,
    pub end: u64,
    pub kind: CrossfadeKind,
    pub contained_clip_id: Option<String>,
}
impl CrossfadeRegion {
    pub fn gains(&self, frame: u64) -> [f64; 2] {
        if frame < self.start || frame >= self.end || self.start >= self.end {
            return [1.0; 2];
        }
        let length = self.end - self.start;
        if length == 1 || (self.kind == CrossfadeKind::Contained && length == 2) {
            return [std::f64::consts::FRAC_1_SQRT_2; 2];
        }
        let t = (frame - self.start) as f64 / (length - 1) as f64;
        match self.kind {
            CrossfadeKind::Staggered if frame == self.start => [1.0, 0.0],
            CrossfadeKind::Staggered if frame + 1 == self.end => [0.0, 1.0],
            CrossfadeKind::Staggered => [
                (t * std::f64::consts::FRAC_PI_2).cos(),
                (t * std::f64::consts::FRAC_PI_2).sin(),
            ],
            CrossfadeKind::Contained => {
                let (outer, inner) = if frame == self.start || frame + 1 == self.end {
                    (1.0, 0.0)
                } else if (frame - self.start) * 2 == length - 1 {
                    (0.0, 1.0)
                } else {
                    (
                        (t * std::f64::consts::PI).cos().abs(),
                        (t * std::f64::consts::PI).sin(),
                    )
                };
                if self.contained_clip_id.as_ref() == Some(&self.first_clip_id) {
                    [inner, outer]
                } else {
                    [outer, inner]
                }
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct CrossfadeData {
    pub revision_id: String,
    pub regions: Vec<CrossfadeRegion>,
}
impl Arrangement {
    pub fn crossfades(&self) -> CrossfadeData {
        let mut regions = Vec::new();
        for track in &self.tracks {
            let mut clips = track.clips.iter().collect::<Vec<_>>();
            clips.sort_by_key(|clip| (clip.position, clip.insertion_order));
            let mut active: Vec<&Clip> = Vec::new();
            for clip in clips {
                active.retain(|other| other.position + other.frames > clip.position);
                for previous in &active {
                    let end =
                        (previous.position + previous.frames).min(clip.position + clip.frames);
                    let coincident =
                        previous.position == clip.position && previous.frames == clip.frames;
                    let contained = if coincident {
                        None
                    } else if previous.position + previous.frames >= clip.position + clip.frames {
                        Some(clip.id.clone())
                    } else if previous.position == clip.position {
                        Some(previous.id.clone())
                    } else {
                        None
                    };
                    regions.push(CrossfadeRegion {
                        track_id: track.id.clone(),
                        first_clip_id: previous.id.clone(),
                        second_clip_id: clip.id.clone(),
                        start: clip.position,
                        end,
                        kind: if contained.is_some() {
                            CrossfadeKind::Contained
                        } else {
                            CrossfadeKind::Staggered
                        },
                        contained_clip_id: contained,
                    });
                }
                active.push(clip);
            }
        }
        CrossfadeData {
            revision_id: self.revision_id.clone(),
            regions,
        }
    }
}
