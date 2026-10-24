use super::*;
use crate::{
    effects::EditTarget,
    exports::{self, ExportFormat, ExportParams, OutputRequest, SaveParams},
};
pub(super) struct PendingOutput {
    document_id: String,
    base_revision: String,
    save: bool,
    pub committing: bool,
    error: Option<CommandError>,
}
impl Editor {
    pub(super) fn export_audio(
        &mut self,
        params: ExportParams,
    ) -> Result<CommandResult, CommandError> {
        self.start_output(params, false)
    }
    pub(super) fn save_audio(&mut self, params: SaveParams) -> Result<CommandResult, CommandError> {
        let document = self.document(&params.document_id)?;
        let format=params.format.clone().map(ExportFormat::from).or_else(||document.save_format.clone()).or_else(||ExportFormat::source_wav(&document.encoding)).ok_or_else(||CommandError::new(ErrorCode::InvalidParams,"Saving an MP3 requires explicit MP3 encoding settings; lossless WAV Save As is also available"))?;
        if format.lossy() && params.format.is_none() {
            return Err(CommandError::new(
                ErrorCode::InvalidParams,
                "MP3 Save requires explicit lossy encoding settings on every save",
            ));
        }
        self.start_output(
            ExportParams {
                target: EditTarget::Whole {
                    document_id: params.document_id,
                    revision_id: params.revision_id,
                },
                path: params.path.unwrap_or_else(|| document.path.clone()),
                format,
                sample_rate: params.sample_rate,
                overwrite: params.overwrite,
                overload: params.overload,
            },
            true,
        )
    }
    fn start_output(
        &mut self,
        params: ExportParams,
        save: bool,
    ) -> Result<CommandResult, CommandError> {
        let (id, base) = params.target.ids();
        let (document, audio) = self.playback_audio(id)?;
        if document.revision_id != base {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Audio revision changed",
            ));
        }
        let range = params.target.range(audio.frames())?;
        let rate = params.format.output_rate(audio.rate, params.sample_rate)?;
        if params.path.trim().is_empty() {
            return Err(CommandError::new(
                ErrorCode::InvalidParams,
                "Choose an output path",
            ));
        }
        if self
            .jobs
            .values()
            .filter(|(job, _)| job.status == JobStatus::Running)
            .count()
            >= 2
            || self
                .pending_outputs
                .values()
                .any(|pending| pending.document_id == document.id && pending.save && save)
        {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Audio jobs or a save for this document are already running",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = JobInfo {
            id: id.clone(),
            kind: if save { "Save" } else { "Export" }.into(),
            path: params.path.clone(),
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
            output: None,
        };
        self.pending_outputs.insert(
            id.clone(),
            PendingOutput {
                document_id: document.id,
                base_revision: document.revision_id.clone(),
                save,
                committing: false,
                error: None,
            },
        );
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        let request = OutputRequest {
            path: params.path,
            revision_id: document.revision_id,
            format: params.format,
            rate,
            overwrite: params.overwrite,
            overload: params.overload,
            protected_source: (!save).then_some(document.path),
        };
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(255);
            let notify = || {
                if let Some(wake) = &wake {
                    wake();
                }
            };
            let result = exports::prepare(&audio, range, request, &cancel, |progress| {
                if last.replace(progress) != progress {
                    let _ = sender.send(Event::Progress(id.clone(), progress));
                    notify();
                }
            })
            .and_then(|prepared| {
                let (permit, approved) = mpsc::sync_channel(1);
                sender
                    .send(Event::OutputPrepared(id.clone(), permit))
                    .map_err(|_| {
                        CommandError::new(ErrorCode::Cancelled, "Output session closed")
                    })?;
                notify();
                if !approved.recv().unwrap_or(false) {
                    return Err(CommandError::new(
                        ErrorCode::Cancelled,
                        "Output publication was cancelled or became stale",
                    ));
                }
                prepared.publish(params.overwrite)
            });
            let _ = sender.send(Event::OutputFinished(id, result));
            notify();
        });
        Ok(CommandResult::Job(job.into()))
    }
    pub(super) fn approve_output(
        &mut self,
        id: String,
        permit: mpsc::SyncSender<bool>,
        revision: u64,
    ) {
        let Some(pending) = self.pending_outputs.get(&id) else {
            return;
        };
        let result = if self
            .jobs
            .get(&id)
            .is_none_or(|(job, _)| job.status != JobStatus::Running)
        {
            Err(CommandError::new(
                ErrorCode::Cancelled,
                "Output cancelled before publication",
            ))
        } else if !self
            .document(&pending.document_id)
            .is_ok_and(|document| document.revision_id == pending.base_revision)
        {
            Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Audio changed before output publication",
            ))
        } else if pending.save && revision == u64::MAX {
            Err(CommandError::new(
                ErrorCode::RevisionExhausted,
                "Session revision exhausted before save publication",
            ))
        } else {
            Ok(())
        };
        let pending = self.pending_outputs.get_mut(&id).unwrap();
        pending.committing = result.is_ok();
        pending.error = result.err();
        let _ = permit.send(pending.committing);
    }
    pub(super) fn finish_output(
        &mut self,
        id: String,
        result: Result<exports::ExportSummary, CommandError>,
        revision: &mut u64,
    ) {
        let Some(pending) = self.pending_outputs.remove(&id) else {
            return;
        };
        if self
            .jobs
            .get(&id)
            .is_none_or(|(job, _)| job.status != JobStatus::Running)
        {
            return;
        }
        let result = if let Some(error) = pending.error {
            Err(error)
        } else {
            result
        };
        let mut saved = None;
        let mut saved_error = None;
        if let Ok(summary) = &result
            && pending.save
            && let Some(document) = self
                .documents
                .iter_mut()
                .find(|document| document.id == pending.document_id)
        {
            let mut after = document.clone();
            after.saved_revision_id = Some(pending.base_revision);
            after.dirty = after.saved_revision_id.as_ref() != Some(&after.revision_id);
            after.path = summary.path.clone();
            after.name = std::path::Path::new(&after.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            after.save_format = Some(summary.format.clone());
            if after != *document {
                if let Some(next) = revision.checked_add(1) {
                    *revision = next;
                    *document = after;
                } else {
                    saved_error = Some(CommandError::new(
                        ErrorCode::RevisionExhausted,
                        "Output was written, but session revision exhausted; saved state was not changed",
                    ));
                }
            }
            saved = Some(document.clone());
        }
        let job = &mut self.jobs.get_mut(&id).unwrap().0;
        match result {
            Ok(summary) => {
                job.status = if saved_error.is_some() {
                    JobStatus::Failed
                } else {
                    JobStatus::Succeeded
                };
                job.error = saved_error;
                job.progress = 100;
                job.output = Some(JobOutput::AudioExport(summary));
                job.result = saved;
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
}
