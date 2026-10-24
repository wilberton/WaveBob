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
mod saving;

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
    Progress(String, u8),
    Finished(String, Result<audio::Audio, CommandError>),
    Edited(String, Result<Option<audio::Audio>, CommandError>),
    ClipProcessed(String, Result<audio::Audio, CommandError>),
    MixPrepared(String, Result<audio::mix::Graph, CommandError>),
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
        }
    }
}
impl Editor {
    #[cfg(feature = "gui")]
    pub fn check_quit(&self, discard: bool) -> Result<(), CommandError> {
        if !discard
            && (self.arrangement.dirty || self.documents.iter().any(|document| document.dirty))
        {
            return Err(CommandError::new(
                ErrorCode::DirtyDocument,
                "Save dirty documents or specify explicit discard intent before quitting",
            ));
        }
        if !self.pending_outputs.is_empty()
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
        self.documents.clone()
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
                Event::Edited(id, result) => self.finish_edit(id, result, revision),
                Event::ClipProcessed(id, result) => self.finish_clip(id, result, revision),
                Event::MixPrepared(id, result) => self.finish_mix(id, result),
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
        match command {
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
                Ok(CommandResult::Document(document.into()))
            }
            Command::CloseDocument(params) => {
                let document = self.document(&params.document_id)?.clone();
                if document.dirty && !params.discard_changes {
                    return Err(CommandError::new(
                        ErrorCode::DirtyDocument,
                        "Closing this document requires explicit discard intent",
                    ));
                }
                advance(revision)?;
                self.documents.retain(|doc| doc.id != document.id);
                self.history.remove_document(&document.id);
                self.collect_revisions();
                if self.active.as_ref() == Some(&document.id) {
                    self.active = self.documents.last().map(|doc| doc.id.clone());
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
