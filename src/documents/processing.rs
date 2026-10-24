use super::*;
use crate::processing::{self, Owner, Processing, Snapshot};

pub(super) struct Preview {
    id: String,
    owner: Owner,
    base: String,
    state: Processing,
    touched: std::time::Instant,
}
impl Editor {
    fn owner_state(&self, owner: &Owner) -> Result<(Processing, String, u64), CommandError> {
        match owner {
            Owner::Document(id) => {
                let doc = self.document(id)?;
                Ok((doc.processing.clone(), doc.revision_id.clone(), doc.frames))
            }
            Owner::Clip(id) => {
                let (_, clip) = self.arrangement.clip(id).ok_or_else(|| not_found("clip"))?;
                Ok((
                    clip.processing.clone(),
                    crate::projects::checksum(&(
                        &clip.revision_id,
                        &clip.processed_revision_id,
                        clip.source_start,
                        clip.source_end,
                    )),
                    clip.source_end - clip.source_start,
                ))
            }
        }
    }
    #[cfg(feature = "gui")]
    pub(crate) fn active_processing_preview(&self) -> Option<Snapshot> {
        self.preview
            .as_ref()
            .and_then(|preview| self.processing_snapshot(preview.owner.clone()).ok())
            .filter(|snapshot| snapshot.preview_id.is_some())
    }
    pub(super) fn has_active_preview(&self) -> bool {
        self.preview
            .as_ref()
            .and_then(|preview| self.processing_snapshot(preview.owner.clone()).ok())
            .is_some_and(|snapshot| snapshot.preview_id.is_some())
    }
    pub(super) fn audible_arrangement(&self) -> crate::arrangement::Arrangement {
        let mut arrangement = self.arrangement.clone();
        let mut changed = false;
        for clip in arrangement
            .tracks
            .iter_mut()
            .flat_map(|track| &mut track.clips)
        {
            if let Ok(snapshot) = self.processing_snapshot(Owner::Clip(clip.id.clone()))
                && let Some(state) = snapshot.preview
            {
                changed |= !state.same_sound(&clip.processing);
                clip.processing = state;
            }
        }
        if changed {
            let hash = crate::projects::checksum(&(&arrangement.revision_id, &arrangement.tracks));
            arrangement.revision_id =
                uuid::Uuid::from_u128(u128::from_str_radix(&hash[..32], 16).unwrap()).to_string();
        }
        arrangement
    }
    pub fn audible_audio(
        &self,
        id: &str,
    ) -> Result<(DocumentInfo, Arc<audio::Audio>), CommandError> {
        let (mut document, audio) = self.playback_audio(id)?;
        let snapshot = self.processing_snapshot(Owner::Document(id.into()))?;
        document.processing = snapshot.preview.unwrap_or(snapshot.committed);
        Ok((document, audio))
    }
    pub(crate) fn processing_read(&self, owner: Owner) -> Result<Snapshot, CommandError> {
        self.processing_snapshot(owner)
    }
    fn processing_snapshot(&self, owner: Owner) -> Result<Snapshot, CommandError> {
        let (committed, source, frames) = self.owner_state(&owner)?;
        let preview = self.preview.as_ref().filter(|p| {
            p.owner == owner
                && p.base == committed.identity(&source, self.arrangement.effect_tempo)
                && p.touched.elapsed().as_secs() < 60
        });
        Ok(Snapshot {
            render_identity: committed.identity(&source, self.arrangement.effect_tempo),
            committed,
            preview: preview.map(|p| p.state.clone()),
            preview_id: preview.map(|p| p.id.clone()),
            owner,
            effect_tempo: self.arrangement.effect_tempo,
            body_frames: frames,
        })
    }
    fn publish_processing(
        &mut self,
        owner: &Owner,
        mut state: Processing,
        revision: &mut u64,
    ) -> Result<(), CommandError> {
        let (old, _, frames) = self.owner_state(owner)?;
        state.envelope.validate(frames)?;
        if old.same_sound(&state) {
            return Ok(());
        }
        let next = revision.checked_add(1).ok_or_else(|| {
            CommandError::new(ErrorCode::RevisionExhausted, "Session revision exhausted")
        })?;
        state.edit_revision_id = uuid::Uuid::new_v4().to_string();
        match owner {
            Owner::Document(id) => {
                let before = self.document(id)?.clone();
                let mut after = before.clone();
                after.processing = state;
                after.dirty = after.saved_revision_id.as_ref() != Some(&after.revision_id)
                    || !after.processing.same_sound(&after.saved_processing);
                *self.documents.iter_mut().find(|d| d.id == *id).unwrap() = after.clone();
                self.history
                    .push_document(before, after, "Change document processing");
            }
            Owner::Clip(id) => {
                let before = self.arrangement.clone();
                let clip = self
                    .arrangement
                    .tracks
                    .iter_mut()
                    .flat_map(|t| &mut t.clips)
                    .find(|c| c.id == *id)
                    .unwrap();
                clip.processing = state;
                clip.edit_revision_id = uuid::Uuid::new_v4().to_string();
                self.arrangement.revision_id = uuid::Uuid::new_v4().to_string();
                self.arrangement.dirty = true;
                self.history.push_arrangement(
                    before,
                    self.arrangement.clone(),
                    "Change clip processing",
                );
            }
        }
        *revision = next;
        Ok(())
    }
    pub(super) fn processing_command(
        &mut self,
        command: Command,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        if self
            .preview
            .as_ref()
            .is_some_and(|p| p.touched.elapsed().as_secs() >= 60)
        {
            self.preview = None;
        }
        let owner = match command {
            Command::GetProcessing(params) => params.owner,
            Command::EditProcessing(params) => {
                let (mut state, _, frames) = self.owner_state(&params.owner)?;
                if state.edit_revision_id != params.expected_edit_revision {
                    return Err(stale());
                }
                state.apply(params.change, frames)?;
                self.publish_processing(&params.owner, state, revision)?;
                params.owner
            }
            Command::BeginProcessingPreview(params) => {
                let (state, source, _) = self.owner_state(&params.owner)?;
                if state.edit_revision_id != params.expected_edit_revision {
                    return Err(stale());
                }
                self.preview = Some(Preview {
                    id: uuid::Uuid::new_v4().to_string(),
                    owner: params.owner.clone(),
                    base: state.identity(&source, self.arrangement.effect_tempo),
                    state,
                    touched: std::time::Instant::now(),
                });
                params.owner
            }
            Command::UpdateProcessingPreview(params) => {
                let preview = self
                    .preview
                    .as_ref()
                    .filter(|p| p.id == params.preview_id)
                    .ok_or_else(|| not_found("preview"))?;
                let owner = preview.owner.clone();
                let (_, source, frames) = self.owner_state(&owner)?;
                let committed = self.processing_snapshot(owner.clone())?;
                if committed
                    .committed
                    .identity(&source, self.arrangement.effect_tempo)
                    != preview.base
                {
                    self.preview = None;
                    return Err(stale());
                }
                let mut next = preview.state.clone();
                next.apply(params.change, frames)?;
                let preview = self.preview.as_mut().unwrap();
                preview.state = next;
                preview.touched = std::time::Instant::now();
                owner
            }
            Command::CommitProcessingPreview(params) => {
                return self.finish_preview(params.preview_id, true, revision);
            }
            Command::CancelProcessingPreview(params) => {
                return self.finish_preview(params.preview_id, false, revision);
            }
            Command::SetEffectTempo(params) => {
                if !params.bpm.is_finite() || !(20.0..=400.0).contains(&params.bpm) {
                    return Err(processing::invalid("Effect tempo must be 20..400 BPM"));
                }
                if self.arrangement.effect_tempo != params.bpm {
                    advance(revision)?;
                    let before = self.arrangement.clone();
                    self.arrangement.effect_tempo = params.bpm;
                    self.arrangement.revision_id = uuid::Uuid::new_v4().to_string();
                    self.arrangement.dirty = true;
                    for doc in &mut self.documents {
                        if !doc.processing.effects.is_empty() {
                            doc.dirty = true;
                        }
                    }
                    self.history.push_arrangement(
                        before,
                        self.arrangement.clone(),
                        "Change effect tempo",
                    );
                }
                return Ok(CommandResult::Arrangement(self.arrangement.clone().into()));
            }
            Command::MakeClipIndependent(params) => {
                let Owner::Clip(id) = &params.owner else {
                    return Err(processing::invalid("Make independent requires a clip"));
                };
                let (_, clip) = self.arrangement.clip(id).ok_or_else(|| not_found("clip"))?;
                if clip.linked {
                    let doc = self.document(&clip.document_id)?;
                    if doc.revision_id != clip.revision_id
                        || !doc.processing.same_sound(&Processing::default())
                    {
                        return Err(CommandError::new(
                            ErrorCode::Busy,
                            "Refresh the document bounce before detaching this clip",
                        ));
                    }
                    advance(revision)?;
                    let before = self.arrangement.clone();
                    let clip = self
                        .arrangement
                        .tracks
                        .iter_mut()
                        .flat_map(|t| &mut t.clips)
                        .find(|c| c.id == *id)
                        .unwrap();
                    clip.linked = false;
                    clip.edit_revision_id = uuid::Uuid::new_v4().to_string();
                    self.arrangement.revision_id = uuid::Uuid::new_v4().to_string();
                    self.arrangement.dirty = true;
                    self.history.push_arrangement(
                        before,
                        self.arrangement.clone(),
                        "Make clip independent",
                    );
                }
                params.owner
            }
            _ => unreachable!(),
        };
        self.processing_snapshot(owner)
            .map(|state| CommandResult::Processing(state.into()))
    }
    pub(super) fn finish_preview(
        &mut self,
        id: String,
        commit: bool,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        let preview = self
            .preview
            .as_ref()
            .filter(|p| p.id == id)
            .ok_or_else(|| not_found("preview"))?;
        let owner = preview.owner.clone();
        if commit {
            if self.processing_snapshot(owner.clone())?.render_identity != preview.base {
                self.preview = None;
                return Err(stale());
            }
            let state = preview.state.clone();
            self.publish_processing(&owner, state, revision)?;
        }
        self.preview = None;
        self.processing_snapshot(owner)
            .map(|state| CommandResult::Processing(state.into()))
    }
}
fn stale() -> CommandError {
    CommandError::new(ErrorCode::StaleRevision, "Processing owner changed")
}
