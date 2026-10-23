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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct FrameRange {
    pub start: u64,
    pub end: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct DocumentInfo {
    pub id: String,
    pub revision_id: String,
    pub path: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub encoding: String,
    pub dirty: bool,
    pub cursor: u64,
    pub selection: Option<FrameRange>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct JobInfo {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub status: JobStatus,
    pub progress: u8,
    pub result: Option<DocumentInfo>,
    pub error: Option<CommandError>,
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
        }
    }
}
impl Editor {
    pub fn documents(&self) -> Vec<DocumentInfo> {
        self.documents.clone()
    }
    pub fn jobs(&self) -> Vec<JobInfo> {
        self.jobs.values().map(|(info, _)| info.clone()).collect()
    }
    pub fn active(&self) -> Option<String> {
        self.active.clone()
    }
    pub fn set_wake(&mut self, wake: Wake) {
        self.wake = Some(wake);
    }
    pub fn poll(&mut self, revision: &mut u64) {
        for event in self.receiver.try_iter() {
            match event {
                Event::Progress(id, percent) => {
                    if let Some((job, _)) = self.jobs.get_mut(&id)
                        && job.status == JobStatus::Running
                    {
                        job.progress = percent;
                    }
                }
                Event::Finished(id, result) => {
                    if let Some((job, _)) = self.jobs.get_mut(&id)
                        && job.status == JobStatus::Running
                    {
                        match result {
                            Ok(audio) => {
                                let Some(next) = revision.checked_add(1) else {
                                    job.status = JobStatus::Failed;
                                    job.error = Some(CommandError::new(
                                        ErrorCode::RevisionExhausted,
                                        "Session revision exhausted",
                                    ));
                                    continue;
                                };
                                let document = DocumentInfo {
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
                                    dirty: false,
                                    cursor: 0,
                                    selection: None,
                                };
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
            Command::GetJob(params) => Ok(CommandResult::Job(
                self.jobs
                    .get(&params.job_id)
                    .ok_or_else(|| not_found("job"))?
                    .0
                    .clone()
                    .into(),
            )),
            Command::CancelJob(params) => {
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
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let result = audio::decode(std::path::Path::new(&params.path), &cancel, |percent| {
                let _ = sender.send(Event::Progress(id.clone(), percent));
                if let Some(wake) = &wake {
                    wake();
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
