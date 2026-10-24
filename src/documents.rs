//! Document read models and background import orchestration. No GUI dependency.
use crate::{Command, CommandError, CommandResult, ErrorCode, audio};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
mod arranging;
mod clip_processing;
mod editing;
mod mixing;
mod processing;
mod projecting;
mod rendering;
mod saving;

fn tab_open_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct FrameRange {
    pub start: u64,
    pub end: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct SourceDetails {
    pub bitrate_mode: Option<String>,
    pub bitrate_kbps: Option<u32>,
    /// Decoder-adjusted delay/padding removed according to metadata, never silence detection.
    pub encoder_delay: Option<u32>,
    pub encoder_padding: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct DocumentInfo {
    pub processing: crate::processing::Processing,
    pub saved_processing: crate::processing::Processing,
    #[serde(default = "tab_open_default")]
    pub tab_open: bool,
    pub id: String,
    pub revision_id: String,
    pub path: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub encoding: String,
    #[serde(default)]
    pub source_details: SourceDetails,
    pub dirty: bool,
    pub cursor: u64,
    pub selection: Option<FrameRange>,
    pub peak_level: f64,
    pub overloaded_samples: u64,
    #[serde(default)]
    pub saved_revision_id: Option<String>,
    #[serde(default)]
    pub save_format: Option<crate::exports::ExportFormat>,
    #[serde(default)]
    pub save_sample_rate: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct JobInfo {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub status: JobStatus,
    pub progress: u8,
    pub result: Option<DocumentInfo>,
    pub error: Option<CommandError>,
    #[serde(default)]
    pub output: Option<JobOutput>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", content = "details", rename_all = "snake_case")]
pub enum JobOutput {
    AudioExport(crate::exports::ExportSummary),
    ClipEdit(crate::arrangement::Clip),
    MixPrepared(crate::mixing::MixPrepared),
    ProjectSaved(crate::projects::ProjectSummary),
    ProjectOpened(crate::projects::ProjectSummary),
    ProcessingRendered(crate::rendering::RenderMetadata),
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Peak {
    pub start_frame: u64,
    pub end_frame: u64,
    pub min: f64,
    pub max: f64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct WaveformData {
    pub document_id: String,
    pub revision_id: String,
    pub start_frame: u64,
    pub end_frame: u64,
    pub channels: Vec<Vec<Peak>>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct SamplesData {
    pub document_id: String,
    pub revision_id: String,
    pub start_frame: u64,
    pub frames: u32,
    pub sample_rate: u32,
    pub channels: Vec<Vec<f64>>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenParams {
    pub path: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentParams {
    pub document_id: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloseParams {
    pub document_id: String,
    #[serde(default)]
    pub discard_changes: bool,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CursorParams {
    pub document_id: String,
    pub frame: u64,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionParams {
    pub document_id: String,
    pub start_frame: u64,
    pub end_frame: u64,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobParams {
    pub job_id: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SamplesParams {
    pub document_id: String,
    pub revision_id: Option<String>,
    #[serde(default)]
    pub start_frame: u64,
    #[schemars(range(min = 1, max = 4096))]
    pub frames: u32,
}
fn default_bins() -> usize {
    1024
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaveformParams {
    pub document_id: String,
    pub revision_id: Option<String>,
    #[serde(default)]
    pub start_frame: u64,
    pub end_frame: Option<u64>,
    #[serde(default = "default_bins")]
    #[schemars(range(min = 1, max = 2048))]
    pub bins: usize,
}

enum Event {
    ProcessingRendered(
        String,
        Result<(audio::Audio, crate::rendering::RenderMetadata), CommandError>,
    ),
    Progress(String, u8),
    Finished(String, Result<audio::Audio, CommandError>),
    Edited(String, Result<Option<audio::Audio>, CommandError>),
    ClipProcessed(String, Result<audio::Audio, CommandError>),
    MixPrepared(String, Result<audio::mix::Graph, CommandError>),
    ProjectSavePrepared(String, mpsc::SyncSender<bool>),
    ProjectSaved(
        String,
        Result<crate::projects::ProjectSummary, CommandError>,
    ),
    ProjectLoaded(
        String,
        Result<Box<crate::projects::storage::LoadedProject>, CommandError>,
    ),
    OutputPrepared(String, mpsc::SyncSender<bool>),
    OutputFinished(String, Result<crate::exports::ExportSummary, CommandError>),
}
pub(crate) type Wake = Arc<dyn Fn() + Send + Sync>;
pub(crate) struct Editor {
    documents: Vec<DocumentInfo>,
    revisions: BTreeMap<String, Arc<audio::Audio>>,
    active: Option<String>,
    jobs: BTreeMap<String, (JobInfo, Arc<AtomicBool>)>,
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
    wake: Option<Wake>,
    pending_edits: BTreeMap<String, editing::PendingEdit>,
    history: editing::History,
    clipboard: Option<editing::Clipboard>,
    pending_outputs: BTreeMap<String, saving::PendingOutput>,
    arrangement: crate::arrangement::Arrangement,
    pending_clips: BTreeMap<String, clip_processing::PendingClip>,
    mix: Option<Arc<audio::mix::Graph>>,
    mix_attempt: Option<String>,
    mix_bases: BTreeMap<String, String>,
    project: projecting::ProjectState,
    pending_projects: BTreeMap<String, projecting::PendingProject>,
    loaded_loop: Option<bool>,
    preview: Option<processing::Preview>,
    pending_renders: BTreeMap<String, rendering::PendingRender>,
    rendered: BTreeMap<String, crate::rendering::RenderMetadata>,
}
impl Default for Editor {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            documents: Vec::new(),
            revisions: BTreeMap::new(),
            active: None,
            jobs: BTreeMap::new(),
            sender,
            receiver,
            wake: None,
            pending_edits: BTreeMap::new(),
            history: editing::History::default(),
            clipboard: None,
            pending_outputs: BTreeMap::new(),
            arrangement: crate::arrangement::Arrangement::default(),
            pending_clips: BTreeMap::new(),
            mix: None,
            mix_attempt: None,
            mix_bases: BTreeMap::new(),
            project: projecting::ProjectState::default(),
            pending_projects: BTreeMap::new(),
            loaded_loop: None,
            preview: None,
            pending_renders: BTreeMap::new(),
            rendered: BTreeMap::new(),
        }
    }
}
impl Editor {
    #[cfg(feature = "gui")]
    pub fn check_quit(&self, discard: bool) -> Result<(), CommandError> {
        if !discard && self.project_info().dirty {
            return Err(CommandError::new(
                ErrorCode::DirtyDocument,
                "Save dirty documents or specify explicit discard intent before quitting",
            ));
        }
        if !self.pending_outputs.is_empty()
            || !self.pending_projects.is_empty()
            || self
                .jobs
                .values()
                .any(|(job, _)| job.status == JobStatus::Running)
        {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Wait for audio jobs to finish or acknowledge cancellation before quitting",
            ));
        }
        Ok(())
    }
    pub fn documents(&self) -> Vec<DocumentInfo> {
        self.documents
            .iter()
            .filter(|doc| doc.tab_open)
            .cloned()
            .collect()
    }
    pub fn jobs(&self) -> Vec<JobInfo> {
        self.jobs.values().map(|(info, _)| info.clone()).collect()
    }
    pub fn active(&self) -> Option<String> {
        self.active.clone()
    }
    pub fn playback_audio(
        &self,
        id: &str,
    ) -> Result<(DocumentInfo, Arc<audio::Audio>), CommandError> {
        let document = self.document(id)?.clone();
        let audio = self.revisions.get(&document.revision_id).unwrap().clone();
        Ok((document, audio))
    }
    pub fn activate_for_transport(&mut self, id: &str) {
        self.active = Some(id.into());
    }
    pub fn update_playback_cursor(&mut self, id: &str, audio_revision: &str, frame: u64) {
        if id == self.arrangement.id && audio_revision == self.arrangement.revision_id {
            self.arrangement.cursor = frame.min(crate::arrangement::MAX_TIMELINE_FRAME);
            return;
        }
        if let Some(document) = self
            .documents
            .iter_mut()
            .find(|doc| doc.id == id && doc.revision_id == audio_revision)
        {
            document.cursor = frame.min(document.frames);
        }
    }
    pub fn seek(
        &mut self,
        id: &str,
        frame: u64,
        revision: &mut u64,
    ) -> Result<DocumentInfo, CommandError> {
        let document = self.document(id)?;
        if frame > document.frames {
            return Err(invalid_range());
        }
        if document.cursor != frame {
            advance(revision)?;
            let audio_revision = document.revision_id.clone();
            self.update_playback_cursor(id, &audio_revision, frame);
        }
        Ok(self.document(id)?.clone())
    }
    pub fn set_wake(&mut self, wake: Wake) {
        self.wake = Some(wake);
    }
    pub fn poll(&mut self, revision: &mut u64) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::ProcessingRendered(id, result) => self.finish_render(id, result),
                Event::Edited(id, result) => self.finish_edit(id, result, revision),
                Event::ClipProcessed(id, result) => self.finish_clip(id, result, revision),
                Event::MixPrepared(id, result) => self.finish_mix(id, result),
                Event::ProjectSavePrepared(id, permit) => {
                    self.approve_project_save(id, permit, *revision)
                }
                Event::ProjectSaved(id, result) => self.finish_project_save(id, result, revision),
                Event::ProjectLoaded(id, result) => self.finish_project_load(id, result, revision),
                Event::OutputPrepared(id, permit) => self.approve_output(id, permit, *revision),
                Event::OutputFinished(id, result) => self.finish_output(id, result, revision),
                Event::Progress(id, percent) => {
                    if let Some((job, _)) = self.jobs.get_mut(&id)
                        && job.status == JobStatus::Running
                    {
                        job.progress = percent;
                    }
                }
                Event::Finished(id, result) => {
                    let allocated = self.retained_bytes();
                    if let Some((job, _)) = self.jobs.get_mut(&id)
                        && job.status == JobStatus::Running
                    {
                        match result {
                            Ok(audio) => {
                                if allocated + audio.samples.len() as u64 * 4
                                    > 2 * 1024 * 1024 * 1024
                                {
                                    job.status = JobStatus::Failed;
                                    job.error = Some(CommandError::new(
                                        ErrorCode::ResourceLimit,
                                        "Session decoded audio exceeds the current 2 GiB limit; close documents before importing more",
                                    ));
                                    continue;
                                }
                                let Some(next) = revision.checked_add(1) else {
                                    job.status = JobStatus::Failed;
                                    job.error = Some(CommandError::new(
                                        ErrorCode::RevisionExhausted,
                                        "Session revision exhausted",
                                    ));
                                    continue;
                                };
                                let mut document = DocumentInfo {
                                    processing: crate::processing::Processing::default(),
                                    saved_processing: crate::processing::Processing::default(),
                                    tab_open: true,
                                    id: uuid::Uuid::new_v4().to_string(),
                                    revision_id: uuid::Uuid::new_v4().to_string(),
                                    path: job.path.clone(),
                                    name: std::path::Path::new(&job.path)
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned(),
                                    sample_rate: audio.rate,
                                    channels: audio.channels,
                                    frames: audio.frames(),
                                    encoding: audio.encoding.clone(),
                                    source_details: audio.source.clone(),
                                    dirty: false,
                                    cursor: 0,
                                    selection: None,
                                    peak_level: audio.peak_level,
                                    overloaded_samples: audio.overloaded_samples,
                                    saved_revision_id: None,
                                    save_format: crate::exports::ExportFormat::source_wav(
                                        &audio.encoding,
                                    ),
                                    save_sample_rate: crate::exports::ExportFormat::source_wav(
                                        &audio.encoding,
                                    )
                                    .map(|_| audio.rate),
                                };
                                document.saved_revision_id = Some(document.revision_id.clone());
                                self.revisions
                                    .insert(document.revision_id.clone(), Arc::new(audio));
                                self.active = Some(document.id.clone());
                                self.documents.push(document.clone());
                                *revision = next;
                                job.status = JobStatus::Succeeded;
                                job.progress = 100;
                                job.result = Some(document);
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
            }
        }
    }
    pub fn execute(
        &mut self,
        command: Command,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        let cancel_preview = command.mutates()
            && !matches!(
                &command,
                Command::UpdateProcessingPreview(_)
                    | Command::CommitProcessingPreview(_)
                    | Command::CancelProcessingPreview(_)
                    | Command::BeginProcessingPreview(_)
                    | Command::CancelJob(_)
                    | Command::PrepareMix(_)
                    | Command::SetCursor(_)
                    | Command::SetSelection(_)
                    | Command::SelectAll(_)
                    | Command::ClearSelection(_)
                    | Command::NavigateArrangement(_)
                    | Command::ActivateDocument(_)
            );
        let result = self.execute_inner(command, revision);
        if result.is_ok() && cancel_preview {
            self.preview = None;
        }
        result
    }
    fn execute_inner(
        &mut self,
        command: Command,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        match command {
            Command::RenderProcessing(params) => self.render_owner(params),
            Command::ReadRenderedSamples(params) => self.rendered_samples(params),
            command @ (Command::GetProcessing(_)
            | Command::EditProcessing(_)
            | Command::BeginProcessingPreview(_)
            | Command::UpdateProcessingPreview(_)
            | Command::CommitProcessingPreview(_)
            | Command::CancelProcessingPreview(_)
            | Command::SetEffectTempo(_)
            | Command::MakeClipIndependent(_)) => self.processing_command(command, revision),
            Command::GetEffectCatalogue(_) => Ok(CommandResult::EffectCatalogue(
                crate::processing::catalogue(),
            )),
            Command::OpenEffectEditor(_)
            | Command::FocusEffectEditor(_)
            | Command::CloseEffectEditor(_)
            | Command::SetEditorGeometry(_) => Err(CommandError::new(
                ErrorCode::MissingCapability,
                "Native effect editors require an attached GUI",
            )),
            Command::PrepareMix(params) => {
                if params.revision_id != self.arrangement.revision_id {
                    Err(CommandError::new(
                        ErrorCode::StaleRevision,
                        "Arrangement changed",
                    ))
                } else {
                    self.ensure_mix(true)
                        .map(|job| CommandResult::Job(job.into()))
                }
            }
            Command::ReadMixSamples(params) => self.mix_samples(params),
            Command::ExportMix(params) => self.export_mix(params),
            Command::TrimClip(params) => self.trim_clip(params),
            Command::ApplyClipEffect(params) => self.process_clip(params),
            Command::ReadClipSamples(params) => self.clip_samples(params),
            command @ (Command::GetArrangement(_)
            | Command::GetCrossfades(_)
            | Command::SetMasterGain(_)
            | Command::AddTrack(_)
            | Command::UpdateTrack(_)
            | Command::RemoveTrack(_)
            | Command::InsertClip(_)
            | Command::DuplicateClip(_)
            | Command::MoveClip(_)
            | Command::RemoveClip(_)
            | Command::NavigateArrangement(_)
            | Command::GetClipWaveform(_)) => self.arrange(command, revision),
            Command::ApplyEffect(params) => self.apply_effect(params),
            Command::GetHistory(_) => Ok(CommandResult::History(self.history.snapshot())),
            Command::Undo(_) => self.restore_history(false, revision),
            Command::Redo(_) => self.restore_history(true, revision),
            Command::GetClipboard(_) => Ok(CommandResult::Clipboard(self.clipboard_info())),
            Command::CopyAudio(params) => self.copy_audio(params, revision),
            Command::CutAudio(params) => self.structural_edit(params, "Cut"),
            Command::DeleteAudio(params) => self.structural_edit(params, "Delete"),
            Command::TrimAudio(params) => self.structural_edit(params, "Trim"),
            Command::PasteAudio(params) => self.paste_audio(params),
            Command::ExportAudio(params) => self.export_audio(params),
            Command::SaveDocument(params) => self.save_audio(params),
            Command::OpenDocument(params) => self.open(params),
            Command::GetDocument(params) => Ok(CommandResult::Document(
                self.document(&params.document_id)?.clone().into(),
            )),
            Command::ActivateDocument(params) => {
                let document = self.document(&params.document_id)?.clone();
                if self.active.as_ref() != Some(&document.id) {
                    advance(revision)?;
                    self.active = Some(document.id.clone());
                }
                self.documents
                    .iter_mut()
                    .find(|d| d.id == document.id)
                    .unwrap()
                    .tab_open = true;
                Ok(CommandResult::Document(document.into()))
            }
            Command::CloseDocument(params) => {
                let document = self.document(&params.document_id)?.clone();
                if document.dirty && !params.discard_changes && !self.document_in_project(&document)
                {
                    return Err(CommandError::new(
                        ErrorCode::DirtyDocument,
                        "Closing this document requires explicit discard intent",
                    ));
                }
                advance(revision)?;
                if self
                    .arrangement
                    .tracks
                    .iter()
                    .flat_map(|t| &t.clips)
                    .any(|c| c.linked && c.document_id == document.id)
                {
                    self.documents
                        .iter_mut()
                        .find(|d| d.id == document.id)
                        .unwrap()
                        .tab_open = false;
                } else {
                    self.documents.retain(|doc| doc.id != document.id);
                    self.history.remove_document(&document.id);
                }
                self.collect_revisions();
                if self.active.as_ref() == Some(&document.id) {
                    self.active = self
                        .documents
                        .iter()
                        .rev()
                        .find(|doc| doc.tab_open)
                        .map(|doc| doc.id.clone());
                }
                Ok(CommandResult::Document(document.into()))
            }
            Command::SetCursor(params) => {
                let mut document = self.document(&params.document_id)?.clone();
                if params.frame > document.frames {
                    return Err(invalid_range());
                }
                document.cursor = params.frame;
                document.selection = None;
                self.commit_navigation(document, revision)
            }
            Command::SetSelection(params) => {
                let mut document = self.document(&params.document_id)?.clone();
                if params.start_frame > document.frames || params.end_frame > document.frames {
                    return Err(invalid_range());
                }
                let (start, end) = (
                    params.start_frame.min(params.end_frame),
                    params.start_frame.max(params.end_frame),
                );
                document.cursor = start;
                document.selection = (start < end).then_some(FrameRange { start, end });
                self.commit_navigation(document, revision)
            }
            Command::SelectAll(params) => {
                let mut document = self.document(&params.document_id)?.clone();
                document.cursor = 0;
                document.selection = (document.frames > 0).then_some(FrameRange {
                    start: 0,
                    end: document.frames,
                });
                self.commit_navigation(document, revision)
            }
            Command::ClearSelection(params) => {
                let mut document = self.document(&params.document_id)?.clone();
                document.selection = None;
                self.commit_navigation(document, revision)
            }
            Command::GetJob(params) => Ok(CommandResult::Job(
                self.jobs
                    .get(&params.job_id)
                    .ok_or_else(|| not_found("job"))?
                    .0
                    .clone()
                    .into(),
            )),
            Command::CancelJob(params) => {
                if self
                    .pending_projects
                    .get(&params.job_id)
                    .is_some_and(|pending| pending.committing)
                {
                    return Err(CommandError::new(
                        ErrorCode::Busy,
                        "Project publication has started; wait for its result",
                    ));
                }
                if self
                    .pending_outputs
                    .get(&params.job_id)
                    .is_some_and(|pending| pending.committing)
                {
                    return Err(CommandError::new(
                        ErrorCode::Busy,
                        "Output publication has started; wait for its result",
                    ));
                }
                let (job, cancel) = self
                    .jobs
                    .get_mut(&params.job_id)
                    .ok_or_else(|| not_found("job"))?;
                if job.status == JobStatus::Running {
                    cancel.store(true, Ordering::Release);
                    job.status = JobStatus::Cancelled;
                }
                Ok(CommandResult::Job(job.clone().into()))
            }
            Command::ReadSamples(params) => {
                let (document, audio) =
                    self.audio(&params.document_id, params.revision_id.as_deref())?;
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
                                audio.samples[frame as usize * audio.channels as usize + channel]
                                    as f64
                            })
                            .collect()
                    })
                    .collect();
                Ok(CommandResult::Samples(SamplesData {
                    document_id: document.id.clone(),
                    revision_id: document.revision_id.clone(),
                    start_frame: params.start_frame,
                    frames: params.frames,
                    sample_rate: audio.rate,
                    channels,
                }))
            }
            Command::GetWaveform(params) => {
                let (document, audio) =
                    self.audio(&params.document_id, params.revision_id.as_deref())?;
                let end = params.end_frame.unwrap_or(audio.frames());
                if !(1..=2048).contains(&params.bins)
                    || params.start_frame > end
                    || end > audio.frames()
                {
                    return Err(invalid_range());
                }
                Ok(CommandResult::Waveform(WaveformData {
                    document_id: document.id.clone(),
                    revision_id: document.revision_id.clone(),
                    start_frame: params.start_frame,
                    end_frame: end,
                    channels: audio.waveform(params.start_frame, end, params.bins),
                }))
            }
            _ => Err(CommandError::new(
                ErrorCode::UnknownCommand,
                "Unsupported document command",
            )),
        }
    }
    fn document(&self, id: &str) -> Result<&DocumentInfo, CommandError> {
        self.documents
            .iter()
            .find(|document| document.id == id)
            .ok_or_else(|| not_found("document"))
    }
    fn commit_navigation(
        &mut self,
        document: DocumentInfo,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        let index = self
            .documents
            .iter()
            .position(|doc| doc.id == document.id)
            .unwrap();
        if document != self.documents[index] {
            advance(revision)?;
            self.documents[index] = document.clone();
        }
        Ok(CommandResult::Document(document.into()))
    }
    fn audio(
        &self,
        id: &str,
        revision: Option<&str>,
    ) -> Result<(&DocumentInfo, &audio::Audio), CommandError> {
        let document = self.document(id)?;
        if revision.is_some_and(|revision| revision != document.revision_id) {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Audio revision changed",
            ));
        }
        Ok((document, self.revisions.get(&document.revision_id).unwrap()))
    }
    fn open(&mut self, params: OpenParams) -> Result<CommandResult, CommandError> {
        if params.path.trim().is_empty() {
            return Err(CommandError::new(
                ErrorCode::InvalidParams,
                "An audio path is required",
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
                "Two imports are already running; wait or cancel one before opening another",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let job = JobInfo {
            id: id.clone(),
            kind: "import".into(),
            path: params.path.clone(),
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
            output: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(255);
            let result = audio::decode(std::path::Path::new(&params.path), &cancel, |percent| {
                if last.replace(percent) != percent {
                    let _ = sender.send(Event::Progress(id.clone(), percent));
                    if let Some(wake) = &wake {
                        wake();
                    }
                }
            });
            let _ = sender.send(Event::Finished(id, result));
            if let Some(wake) = wake {
                wake();
            }
        });
        Ok(CommandResult::Job(job.into()))
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        for (_, cancel) in self.jobs.values() {
            cancel.store(true, Ordering::Release);
        }
        // Close publication permits before joining output workers, so EOF/drop
        // cannot leave a prepared temporary file waiting on the command thread.
        let (_, empty) = mpsc::channel();
        drop(std::mem::replace(&mut self.receiver, empty));
        for pending in self.pending_outputs.values_mut() {
            if let Some(worker) = pending.worker.take() {
                let _ = worker.join();
            }
        }
        for pending in self.pending_projects.values_mut() {
            if let Some(worker) = pending.worker.take() {
                let _ = worker.join();
            }
        }
    }
}
fn not_found(kind: &str) -> CommandError {
    CommandError::new(ErrorCode::NotFound, format!("Unknown {kind} identifier"))
}
fn invalid_range() -> CommandError {
    CommandError::new(
        ErrorCode::InvalidParams,
        "Requested frame range or waveform bin count is out of bounds",
    )
}
fn advance(revision: &mut u64) -> Result<(), CommandError> {
    *revision = revision.checked_add(1).ok_or_else(|| {
        CommandError::new(ErrorCode::RevisionExhausted, "Session revision exhausted")
    })?;
    Ok(())
}

#[cfg(all(test, feature = "gui"))]
mod native_editor_host_tests {
    use super::*;
    use crate::{
        Outcome, Request, Session,
        desktop::DesktopHost,
        presentation::*,
        processing::{EffectKind, Instance, Owner, Processing},
    };
    fn fixture() -> (DesktopHost, Vec<(Owner, String)>) {
        let mut session = Session::default();
        let mut owners = Vec::new();
        for name in ["First", "Second"] {
            let id = uuid::Uuid::new_v4().to_string();
            let revision = uuid::Uuid::new_v4().to_string();
            let effect = uuid::Uuid::new_v4().to_string();
            let mut processing = Processing::default();
            // A shell probe only: unavailable DSP is never registered or played.
            processing.effects.push(Instance {
                id: effect.clone(),
                kind: EffectKind::SoftClipper,
                bypassed: false,
                parameters: Default::default(),
            });
            let document:DocumentInfo = serde_json::from_value(serde_json::json!({"id":id,"revision_id":revision,"name":name,"path":"probe.wav","sample_rate":48000,"channels":1,"frames":128,"encoding":"float32","dirty":false,"cursor":0,"selection":null,"peak_level":0.0,"overloaded_samples":0,"processing":processing,"saved_processing":processing})).unwrap();
            let audio = audio::prepare(
                48000,
                1,
                "float32".into(),
                Default::default(),
                vec![0.0; 128],
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            session.editor.revisions.insert(revision, Arc::new(audio));
            session.editor.documents.push(document);
            owners.push((Owner::Document(id), effect));
        }
        (DesktopHost::with_session(session), owners)
    }
    fn params(owner: &(Owner, String)) -> EditorParams {
        EditorParams {
            owner: owner.0.clone(),
            instance_id: owner.1.clone(),
            expected_presentation_revision: None,
        }
    }
    fn execute(host: &mut DesktopHost, command: Command) {
        let response = host.execute(Request::new("shell-probe", command));
        assert!(
            matches!(response.outcome, Outcome::Ok { .. }),
            "{response:?}"
        );
    }
    #[test]
    fn native_bindings_focus_close_remove_and_undo_preserve_owner_identity() {
        let (mut host, owners) = fixture();
        let initial = host.snapshot().revision;
        execute(&mut host, Command::OpenEffectEditor(params(&owners[0])));
        execute(&mut host, Command::OpenEffectEditor(params(&owners[1])));
        let first = host.presentation().editor_windows[0].clone();
        execute(&mut host, Command::OpenEffectEditor(params(&owners[0])));
        assert_eq!(host.presentation().editor_windows.len(), 2);
        assert_eq!(host.presentation().editor_windows[0].id, first.id);
        assert_eq!(host.presentation().editor_windows[0].focus_generation, 2);
        execute(
            &mut host,
            Command::SetMode(SetModeParams {
                mode: ViewMode::Multitrack,
                expected_presentation_revision: None,
            }),
        );
        assert_eq!(host.presentation().editor_windows[0].owner, owners[0].0);
        let processing = host.processing(owners[1].0.clone()).unwrap();
        execute(
            &mut host,
            Command::BeginProcessingPreview(crate::processing::BeginParams {
                owner: owners[1].0.clone(),
                expected_edit_revision: processing.committed.edit_revision_id.clone(),
            }),
        );
        let preview = host
            .processing(owners[1].0.clone())
            .unwrap()
            .preview_id
            .unwrap();
        let id = host.presentation().editor_windows[1].id.clone();
        host.register_editor_preview(&id, Some(preview));
        execute(&mut host, Command::CloseEffectEditor(params(&owners[1])));
        assert_eq!(host.processing(owners[1].0.clone()).unwrap(), processing);
        assert_eq!(host.snapshot().revision, initial);
        let processing = host.processing(owners[0].0.clone()).unwrap();
        execute(
            &mut host,
            Command::EditProcessing(crate::processing::EditParams {
                owner: owners[0].0.clone(),
                expected_edit_revision: processing.committed.edit_revision_id,
                change: crate::processing::Action::Remove {
                    effect_id: owners[0].1.clone(),
                },
            }),
        );
        host.begin_frame(&eframe::egui::Context::default());
        assert!(host.presentation().editor_windows.is_empty());
        execute(&mut host, Command::Undo(crate::EmptyParams {}));
        assert_eq!(
            host.processing(owners[0].0.clone())
                .unwrap()
                .committed
                .effects
                .len(),
            1
        );
        host.begin_frame(&eframe::egui::Context::default());
        assert!(
            host.presentation().editor_windows.is_empty(),
            "Undo must not reopen windows"
        );
    }
    #[test]
    fn native_editor_validation_and_geometry_are_presentation_only() {
        let (mut host, owners) = fixture();
        let revision = host.snapshot().revision;
        assert!(matches!(
            host.execute(Request::new(
                "focus-closed",
                Command::FocusEffectEditor(params(&owners[0]))
            ))
            .outcome,
            Outcome::Error {
                error: CommandError {
                    code: ErrorCode::NotFound,
                    ..
                }
            }
        ));
        execute(&mut host, Command::OpenEffectEditor(params(&owners[0])));
        let id = host.presentation().editor_windows[0].id.clone();
        execute(
            &mut host,
            Command::SetEditorGeometry(EditorGeometryParams {
                editor_id: id.clone(),
                geometry: WindowGeometry {
                    width: 800.0,
                    height: 500.0,
                    x: Some(-1400.0),
                    y: Some(50.0),
                },
                expected_presentation_revision: None,
            }),
        );
        assert_eq!(host.presentation().layout.editor_geometry[&id].width, 800.0);
        let before = host.presentation();
        let response = host.execute(Request::new(
            "invalid-size",
            Command::SetEditorGeometry(EditorGeometryParams {
                editor_id: id,
                geometry: WindowGeometry {
                    width: f32::NAN,
                    ..Default::default()
                },
                expected_presentation_revision: None,
            }),
        ));
        assert!(matches!(response.outcome, Outcome::Error { .. }));
        assert_eq!(host.presentation(), before);
        assert_eq!(host.snapshot().revision, revision);
        let response = Session::default().execute(Request::new(
            "headless",
            Command::OpenEffectEditor(params(&owners[0])),
        ));
        assert!(matches!(
            response.outcome,
            Outcome::Error {
                error: CommandError {
                    code: ErrorCode::MissingCapability,
                    ..
                }
            }
        ));
    }
}
