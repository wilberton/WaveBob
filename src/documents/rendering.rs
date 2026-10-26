use super::*;
use crate::{processing::Owner, rendering};

pub(super) struct PendingRender {
    owner: Owner,
    identity: String,
}
impl Editor {
    pub(super) fn render_owner(
        &mut self,
        params: rendering::RenderParams,
    ) -> Result<CommandResult, CommandError> {
        let snapshot = self.processing_read(params.owner.clone())?;
        if snapshot.preview_id.is_some() {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Commit or cancel the preview before rendering committed audio",
            ));
        }
        if params
            .expected_render_identity
            .as_ref()
            .is_some_and(|id| id != &snapshot.render_identity)
        {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Owner sound changed",
            ));
        }
        if self
            .jobs
            .values()
            .filter(|(job, _)| job.status == JobStatus::Running)
            .count()
            >= 2
        {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Two audio workers are already running",
            ));
        }
        let (audio, range) = match &params.owner {
            Owner::Document(id) => {
                let (_, audio) = self.playback_audio(id)?;
                let range = FrameRange {
                    start: 0,
                    end: audio.frames(),
                };
                (audio, range)
            }
            Owner::Clip(id) => {
                let (_, audio, range) = self.clip_audio(id)?;
                (audio, range)
            }
        };
        let clip_body = match &params.owner {
            Owner::Clip(id) => self
                .arrangement
                .clip(id)
                .map(|(_, c)| c.source_end - c.source_start),
            _ => None,
        };
        let inherited_cap = match &params.owner {
            Owner::Clip(id) => self
                .arrangement
                .clip(id)
                .is_some_and(|(_, c)| c.bounce.as_ref().is_some_and(|b| b.metadata.tail_capped)),
            _ => false,
        };
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = JobInfo {
            id: id.clone(),
            kind: "Render processing".into(),
            path: String::new(),
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
            output: None,
        };
        self.pending_renders.insert(
            id.clone(),
            PendingRender {
                owner: params.owner,
                identity: snapshot.render_identity.clone(),
            },
        );
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let result = match clip_body {
                Some(body) => rendering::offline_clip(
                    &audio,
                    range,
                    body,
                    &snapshot.committed,
                    snapshot.effect_tempo,
                    &cancel,
                ),
                None => rendering::offline(
                    &audio,
                    range,
                    &snapshot.committed,
                    snapshot.effect_tempo,
                    &cancel,
                ),
            }
            .map(|(audio, mut metadata)| {
                metadata.render_identity = snapshot.render_identity;
                metadata.tail_capped |= inherited_cap;
                (audio, metadata)
            });
            let _ = sender.send(Event::ProcessingRendered(id, result));
            if let Some(wake) = wake {
                wake();
            }
        });
        Ok(CommandResult::Job(job.into()))
    }
    pub(super) fn finish_render(
        &mut self,
        id: String,
        result: Result<(audio::Audio, rendering::RenderMetadata), CommandError>,
    ) {
        let Some(pending) = self.pending_renders.remove(&id) else {
            return;
        };
        if self
            .jobs
            .get(&id)
            .is_none_or(|(job, _)| job.status != JobStatus::Running)
        {
            return;
        }
        let result = result.and_then(|(audio, metadata)| {
            if self.processing_read(pending.owner)?.render_identity != pending.identity {
                return Err(CommandError::new(
                    ErrorCode::StaleRevision,
                    "Processing changed while rendering",
                ));
            }
            if self.retained_bytes() + audio.samples.len() as u64 * 4 > 2 * 1024 * 1024 * 1024 {
                return Err(CommandError::new(
                    ErrorCode::ResourceLimit,
                    "Render exceeds 2 GiB session audio limit",
                ));
            }
            while self.rendered.len() >= 8 {
                let oldest = self.rendered.keys().next().unwrap().clone();
                self.rendered.remove(&oldest);
                self.revisions.remove(&oldest);
            }
            self.revisions
                .insert(metadata.audio_revision_id.clone(), Arc::new(audio));
            self.rendered
                .insert(metadata.audio_revision_id.clone(), metadata.clone());
            Ok(metadata)
        });
        let job = &mut self.jobs.get_mut(&id).unwrap().0;
        match result {
            Ok(metadata) => {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.output = Some(JobOutput::ProcessingRendered(metadata));
            }
            Err(error) => {
                job.status = if error.code == ErrorCode::Cancelled {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Failed
                };
                job.error = Some(error);
            }
        }
    }
    pub(super) fn rendered_samples(
        &self,
        params: rendering::SamplesParams,
    ) -> Result<CommandResult, CommandError> {
        if !self.rendered.contains_key(&params.audio_revision_id) {
            return Err(not_found("rendered audio"));
        }
        let audio = self
            .revisions
            .get(&params.audio_revision_id)
            .ok_or_else(|| not_found("rendered audio"))?;
        let end = params
            .start_frame
            .checked_add(params.frames as u64)
            .ok_or_else(invalid_range)?;
        if !(1..=4096).contains(&params.frames) || end > audio.frames() {
            return Err(invalid_range());
        }
        let channels = (0..audio.channels as usize)
            .map(|channel| {
                (params.start_frame..end)
                    .map(|frame| {
                        audio.samples[frame as usize * audio.channels as usize + channel] as f64
                    })
                    .collect()
            })
            .collect();
        Ok(CommandResult::Samples(SamplesData {
            document_id: String::new(),
            revision_id: params.audio_revision_id,
            start_frame: params.start_frame,
            frames: params.frames,
            sample_rate: audio.rate,
            channels,
        }))
    }
}
