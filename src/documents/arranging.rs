use super::*;
use crate::arrangement::*;

impl Editor {
    pub fn arrangement(&self) -> Arrangement {
        self.arrangement.clone()
    }
    pub(super) fn arrange(
        &mut self,
        command: Command,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        if let Command::GetClipWaveform(params) = &command {
            let (_, clip) = self
                .arrangement
                .clip(&params.clip_id)
                .ok_or_else(|| not_found("clip"))?;
            if !(1..=2048).contains(&params.bins) {
                return Err(invalid("Waveform bins must be 1..2048"));
            }
            let (_, audio, range) = self.clip_audio(&clip.id)?;
            return Ok(CommandResult::Waveform(WaveformData {
                document_id: clip.document_id.clone(),
                revision_id: clip.edit_revision_id.clone(),
                start_frame: range.start,
                end_frame: range.end,
                channels: audio.waveform(range.start, range.end, params.bins),
            }));
        }
        let mut next = self.arrangement.clone();
        let label = match command {
            Command::GetArrangement(_) => return Ok(CommandResult::Arrangement(next.into())),
            Command::AddTrack(params) => {
                let index = params.index.unwrap_or(next.tracks.len());
                if index > next.tracks.len() {
                    return Err(invalid("Track index is out of range"));
                }
                let name = params
                    .name
                    .unwrap_or_else(|| format!("Track {}", next.tracks.len() + 1));
                validate_name(&name)?;
                next.tracks.insert(
                    index,
                    Track {
                        id: uuid::Uuid::new_v4().to_string(),
                        name,
                        mute: false,
                        solo: false,
                        gain_db: 0.0,
                        pan: 0.0,
                        clips: Vec::new(),
                    },
                );
                "Add track"
            }
            Command::UpdateTrack(params) => {
                let i = next
                    .tracks
                    .iter()
                    .position(|t| t.id == params.track_id)
                    .ok_or_else(|| not_found("track"))?;
                if let Some(index) = params.index
                    && index >= next.tracks.len()
                {
                    return Err(invalid("Track index is out of range"));
                }
                let track = &mut next.tracks[i];
                if let Some(name) = params.name {
                    validate_name(&name)?;
                    track.name = name;
                }
                if let Some(value) = params.mute {
                    track.mute = value;
                }
                if let Some(value) = params.solo {
                    track.solo = value;
                }
                if let Some(value) = params.gain_db {
                    if !value.is_finite() || !(-120.0..=24.0).contains(&value) {
                        return Err(invalid("Track gain must be -120..24 dB"));
                    }
                    track.gain_db = value;
                }
                if let Some(value) = params.pan {
                    if !value.is_finite() || !(-1.0..=1.0).contains(&value) {
                        return Err(invalid("Pan must be -1..1"));
                    }
                    track.pan = value;
                }
                if let Some(index) = params.index {
                    let track = next.tracks.remove(i);
                    next.tracks.insert(index, track);
                }
                "Change track"
            }
            Command::RemoveTrack(params) => {
                if !next.tracks.iter().any(|t| t.id == params.track_id) {
                    return Err(not_found("track"));
                }
                next.tracks.retain(|t| t.id != params.track_id);
                "Remove track"
            }
            Command::InsertClip(params) => {
                let (doc, audio) = self.audio(&params.document_id, Some(&params.revision_id))?;
                if params.source_start >= params.source_end || params.source_end > audio.frames() {
                    return Err(invalid_range());
                }
                let frames = audio::resample::frame_count(
                    params.source_end - params.source_start,
                    audio.rate,
                    next.sample_rate,
                );
                let track = next
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == params.track_id)
                    .ok_or_else(|| not_found("track"))?;
                let clip = Clip {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: doc.name.clone(),
                    document_id: doc.id.clone(),
                    revision_id: doc.revision_id.clone(),
                    edit_revision_id: uuid::Uuid::new_v4().to_string(),
                    processed_revision_id: None,
                    effects: Vec::new(),
                    source_frames: audio.frames(),
                    source_start: params.source_start,
                    source_end: params.source_end,
                    source_rate: audio.rate,
                    channels: audio.channels,
                    position: params.position,
                    frames,
                };
                next.selected_clip_ids = vec![clip.id.clone()];
                track.clips.push(clip);
                "Insert clip"
            }
            Command::MoveClip(params) => {
                let (track, clip) = next
                    .clip(&params.clip_id)
                    .ok_or_else(|| not_found("clip"))?;
                let origin = track.id.clone();
                let mut clip = clip.clone();
                if clip.position != params.position || origin != params.track_id {
                    clip.edit_revision_id = uuid::Uuid::new_v4().to_string();
                }
                clip.position = params.position;
                if !next.tracks.iter().any(|t| t.id == params.track_id) {
                    return Err(not_found("track"));
                }
                next.tracks
                    .iter_mut()
                    .find(|t| t.id == origin)
                    .unwrap()
                    .clips
                    .retain(|c| c.id != clip.id);
                next.tracks
                    .iter_mut()
                    .find(|t| t.id == params.track_id)
                    .unwrap()
                    .clips
                    .push(clip);
                "Move clip"
            }
            Command::RemoveClip(params) => {
                if next.clip(&params.clip_id).is_none() {
                    return Err(not_found("clip"));
                }
                for track in &mut next.tracks {
                    track.clips.retain(|c| c.id != params.clip_id);
                }
                "Remove clip"
            }
            Command::NavigateArrangement(params) => {
                if params.cursor > MAX_TIMELINE_FRAME
                    || params
                        .selection
                        .as_ref()
                        .is_some_and(|r| r.start >= r.end || r.end > MAX_TIMELINE_FRAME)
                {
                    return Err(invalid_range());
                }
                let mut selected = params.selected_clip_ids;
                selected.sort();
                selected.dedup();
                if selected.iter().any(|id| next.clip(id).is_none()) {
                    return Err(not_found("clip"));
                }
                next.cursor = params.cursor;
                next.selection = params.selection;
                next.selected_clip_ids = selected;
                if next != self.arrangement {
                    advance(revision)?;
                    self.arrangement = next;
                }
                return Ok(CommandResult::Arrangement(self.arrangement.clone().into()));
            }
            _ => unreachable!(),
        };
        for track in &mut next.tracks {
            track
                .clips
                .sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
        }
        let ids: std::collections::BTreeSet<_> = next
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|c| c.id.clone())
            .collect();
        next.selected_clip_ids.retain(|id| ids.contains(id));
        next.validate()?;
        if next != self.arrangement {
            advance(revision)?;
            next.revision_id = uuid::Uuid::new_v4().to_string();
            next.dirty = true;
            self.history
                .push_arrangement(self.arrangement.clone(), next.clone(), label);
            self.arrangement = next;
            self.collect_revisions();
        }
        Ok(CommandResult::Arrangement(self.arrangement.clone().into()))
    }
}
fn validate_name(name: &str) -> Result<(), CommandError> {
    if name.trim().is_empty() || name.chars().count() > 128 {
        Err(invalid("Track name must contain 1..128 characters"))
    } else {
        Ok(())
    }
}
