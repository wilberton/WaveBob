//! The shared application command interface. No window or audio device is required.

mod audio;
mod commands;
#[cfg(feature = "gui")]
pub mod desktop;
pub mod documents;
pub mod ipc;
pub mod mcp;
#[cfg(feature = "gui")]
mod preferences;
pub mod presentation;
pub mod protocol;
pub mod transport;

pub use commands::{Capabilities, Command, CommandDescriptor, EmptyParams, LoopParams};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;

/// An owned read model; changing it cannot mutate the session.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub loop_enabled: bool,
    pub documents: Vec<documents::DocumentInfo>,
    pub active_document_id: Option<String>,
    pub jobs: Vec<documents::JobInfo>,
}

/// Typed requests are used directly by the GUI. Protocol adapters construct the
/// same request after decoding their transport-specific envelopes.
#[derive(Debug)]
pub struct Request {
    pub id: String,
    pub command: Command,
    pub expected_revision: Option<u64>,
}

impl Request {
    pub fn new(id: impl Into<String>, command: Command) -> Self {
        Self {
            id: id.into(),
            command,
            expected_revision: None,
        }
    }

    pub fn expecting_revision(mut self, revision: u64) -> Self {
        self.expected_revision = Some(revision);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum CommandResult {
    Snapshot(SessionSnapshot),
    Capabilities(Capabilities),
    Presentation(presentation::PresentationSnapshot),
    Rendered(presentation::RenderMetadata),
    Capture(presentation::CaptureResult),
    Document(Box<documents::DocumentInfo>),
    Job(Box<documents::JobInfo>),
    Samples(documents::SamplesData),
    Waveform(documents::WaveformData),
    Transport(Box<transport::TransportSnapshot>),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Ok { result: CommandResult },
    Error { error: CommandError },
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Response {
    pub protocol_version: u32,
    pub id: Option<String>,
    pub session_id: String,
    pub revision: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidJson,
    InvalidRequest,
    UnknownCommand,
    InvalidParams,
    UnsupportedProtocol,
    StaleRevision,
    RevisionExhausted,
    MissingCapability,
    Timeout,
    RenderFailed,
    OutputExists,
    IoError,
    NotFound,
    UnsupportedFormat,
    DecodeFailed,
    ResourceLimit,
    Cancelled,
    Busy,
    DirtyDocument,
    AudioDeviceError,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl CommandError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }
}

/// Sole owner of application state. Exclusive dispatch serializes mutations;
/// transports and views never receive mutable access to the underlying state.
pub struct Session {
    snapshot: SessionSnapshot,
    editor: documents::Editor,
    playback: transport::Playback,
    last_active_document: Option<String>,
    wake: Option<documents::Wake>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            snapshot: SessionSnapshot {
                session_id: uuid::Uuid::new_v4().to_string(),
                revision: 0,
                loop_enabled: false,
                documents: Vec::new(),
                active_document_id: None,
                jobs: Vec::new(),
            },
            editor: documents::Editor::default(),
            playback: transport::Playback::default(),
            last_active_document: None,
            wake: None,
        }
    }
}

impl Session {
    pub fn snapshot(&self) -> SessionSnapshot {
        let mut snapshot = self.snapshot.clone();
        snapshot.documents = self.editor.documents();
        snapshot.active_document_id = self.editor.active();
        snapshot.jobs = self.editor.jobs();
        snapshot
    }

    pub fn poll_jobs(&mut self) {
        self.editor.poll(&mut self.snapshot.revision);
        self.playback.poll();
        let transport = self.playback.snapshot();
        if transport.applied.generation == transport.requested.generation
            && let Some(id) = &transport.applied.document_id
        {
            self.editor
                .update_playback_cursor(id, transport.applied.frame);
        }
        self.synchronize_output(None);
    }
    pub fn set_waker(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.wake = Some(wake.clone());
        self.editor.set_wake(wake);
    }
    pub fn with_manual_output(rate: u32) -> Result<(Self, transport::ManualOutput), CommandError> {
        let (playback, output) = transport::Playback::manual(rate)?;
        Ok((
            Self {
                playback,
                ..Self::default()
            },
            output,
        ))
    }
    pub fn transport_snapshot(&self) -> transport::TransportSnapshot {
        self.playback.snapshot()
    }
    pub fn audio_output_configured(&self) -> bool {
        self.playback.configured()
    }
    #[cfg(feature = "audio-output")]
    pub fn enable_native_output(&mut self) {
        self.playback = transport::Playback::native(self.wake.clone());
    }

    pub fn execute(&mut self, request: Request) -> Response {
        if !matches!(&request.command, Command::CancelJob(_)) {
            self.poll_jobs();
        }
        self.execute_committed(request)
    }

    pub(crate) fn execute_committed(&mut self, request: Request) -> Response {
        if let Err(error) = self.validate(&request) {
            return self.failure(Some(request.id), error);
        }
        let result = match request.command {
            Command::GetSession(_) => Ok(CommandResult::Snapshot(self.snapshot())),
            Command::GetCapabilities(_) => Ok(CommandResult::Capabilities(commands::capabilities(
                false,
                self.playback.configured(),
            ))),
            Command::GetTransport(_) => {
                Ok(CommandResult::Transport(self.playback.snapshot().into()))
            }
            Command::PlayDocument(params) => self.play_document(&params.document_id, false),
            Command::TogglePlayback(params) => self.play_document(&params.document_id, true),
            Command::PausePlayback(_) => self
                .pause_output()
                .map(|_| CommandResult::Transport(self.playback.snapshot().into())),
            Command::SeekDocument(params) => self.seek_document(&params.document_id, params.frame),
            Command::RewindDocument(params) => self.seek_document(&params.document_id, 0),
            Command::RetryOutput(_) => self.retry_output(),
            Command::SetLoop(params) => self.set_loop(params.enabled),
            Command::ToggleLoop(_) => self.set_loop(!self.snapshot.loop_enabled),
            Command::GetPresentation(_)
            | Command::SetMode(_)
            | Command::SetViewport(_)
            | Command::SetLayout(_)
            | Command::TogglePanels(_)
            | Command::RenderBarrier(_)
            | Command::CapturePng(_) => Err(CommandError::new(
                ErrorCode::MissingCapability,
                "This command requires an attached GUI renderer",
            )),
            command => {
                let seek = if let Command::SetCursor(params) = &command {
                    Some((params.document_id.clone(), params.frame))
                } else {
                    None
                };
                let before = self.snapshot.revision;
                let result = self.editor.execute(command, &mut self.snapshot.revision);
                if result.is_ok() {
                    let seek = seek.filter(|(id, frame)| self.output_seek_needed(id, *frame));
                    if seek.is_some() && before == self.snapshot.revision {
                        match self.next_revision() {
                            Ok(revision) => self.snapshot.revision = revision,
                            Err(error) => return self.failure(Some(request.id), error),
                        }
                    }
                    self.synchronize_output(seek);
                }
                result
            }
        };
        match result {
            Ok(result) => self.success(Some(request.id), result),
            Err(error) => self.failure(Some(request.id), error),
        }
    }

    pub(crate) fn validate(&self, request: &Request) -> Result<(), CommandError> {
        if request.id.trim().is_empty() {
            return Err(CommandError::new(
                ErrorCode::InvalidRequest,
                "Request id must not be blank",
            ));
        }
        if request.command.mutates()
            && let Some(expected) = request.expected_revision
            && expected != self.snapshot.revision
        {
            return Err(CommandError {
                code: ErrorCode::StaleRevision,
                message: "Session changed since the expected revision".into(),
                details: Some(serde_json::json!({
                    "expected_revision": expected,
                    "actual_revision": self.snapshot.revision,
                })),
            });
        }

        if request.command.requires_audio_output() && !self.playback.configured() {
            return Err(CommandError::new(
                ErrorCode::MissingCapability,
                "This session has no audio output adapter",
            ));
        }
        if request.command.mutates()
            && self.playback.configured()
            && !matches!(request.command, Command::RetryOutput(_))
        {
            self.playback.can_request()?;
        }

        Ok(())
    }

    fn next_revision(&self) -> Result<u64, CommandError> {
        self.snapshot.revision.checked_add(1).ok_or_else(|| {
            CommandError::new(ErrorCode::RevisionExhausted, "Session revision exhausted")
        })
    }
    fn play_document(&mut self, id: &str, toggle: bool) -> Result<CommandResult, CommandError> {
        self.playback.ready()?;
        let (document, audio) = self.editor.playback_audio(id)?;
        if document.frames == 0 {
            return Err(CommandError::new(
                ErrorCode::InvalidParams,
                "An empty document cannot play",
            ));
        }
        let state = self.playback.snapshot();
        if state.requested.document_id.as_deref() == Some(id) && state.requested.playing {
            if toggle {
                self.pause_output()?;
            }
            return Ok(CommandResult::Transport(self.playback.snapshot().into()));
        }
        let revision = self.next_revision()?;
        let seek = if document.cursor >= document.frames {
            Some(0)
        } else if state.applied.document_id.as_deref() == Some(id)
            && state.applied.audio_revision_id.as_deref() == Some(&document.revision_id)
            && state.applied.frame == document.cursor
            && !state.pending
        {
            None
        } else {
            Some(document.cursor)
        };
        self.playback
            .request(&document, audio, self.snapshot.loop_enabled, true, seek)?;
        self.editor.activate_for_transport(id);
        self.last_active_document = Some(id.into());
        self.snapshot.revision = revision;
        Ok(CommandResult::Transport(self.playback.snapshot().into()))
    }
    pub(crate) fn pause_output(&mut self) -> Result<bool, CommandError> {
        if !self.playback.snapshot().requested.playing {
            return Ok(false);
        }
        let revision = self.next_revision()?;
        let changed = self.playback.pause()?;
        if changed {
            self.snapshot.revision = revision;
        }
        Ok(changed)
    }
    fn seek_document(&mut self, id: &str, frame: u64) -> Result<CommandResult, CommandError> {
        let before = self.snapshot.revision;
        let document = self.editor.seek(id, frame, &mut self.snapshot.revision)?;
        let seek = self.output_seek_needed(id, frame);
        if seek && before == self.snapshot.revision {
            self.snapshot.revision = self.next_revision()?;
        }
        self.synchronize_output(seek.then(|| (id.into(), frame)));
        Ok(CommandResult::Document(document.into()))
    }
    fn output_seek_needed(&self, id: &str, frame: u64) -> bool {
        let state = self.playback.snapshot();
        state.requested.document_id.as_deref() == Some(id)
            && !(state.requested.frame == Some(frame)
                && !state.requested.playing
                && (state.pending || state.applied.frame == frame))
    }
    fn synchronize_output(&mut self, seek: Option<(String, u64)>) {
        let active = self.editor.active();
        if active != self.last_active_document {
            let _ = self.playback.pause();
            self.last_active_document = active;
        }
        if let Some(id) = self.playback.snapshot().requested.document_id {
            match self.editor.playback_audio(&id) {
                Ok((document, audio)) => {
                    let state = self.playback.snapshot();
                    let explicit = seek
                        .as_ref()
                        .filter(|(seek_id, _)| seek_id == &id)
                        .map(|(_, frame)| *frame);
                    let revision_changed =
                        state.requested.audio_revision_id.as_ref() != Some(&document.revision_id);
                    if explicit.is_some()
                        || self
                            .playback
                            .needs_refresh(&document, self.snapshot.loop_enabled)
                    {
                        let position =
                            explicit.or_else(|| revision_changed.then_some(document.cursor));
                        let _ = self.playback.request(
                            &document,
                            audio,
                            self.snapshot.loop_enabled,
                            state.requested.playing,
                            position,
                        );
                    }
                }
                Err(_) => {
                    let _ = self.playback.clear();
                }
            }
        }
    }
    fn retry_output(&mut self) -> Result<CommandResult, CommandError> {
        let revision = self.next_revision()?;
        match self.playback.snapshot().output.kind {
            transport::OutputKind::Manual => self.playback.retry_manual(),
            #[cfg(feature = "audio-output")]
            transport::OutputKind::Device => {
                let old = std::mem::replace(
                    &mut self.playback,
                    transport::Playback::native(self.wake.clone()),
                );
                std::thread::spawn(move || drop(old));
            }
            _ => {
                return Err(CommandError::new(
                    ErrorCode::MissingCapability,
                    "This session has no audio output adapter",
                ));
            }
        }
        self.snapshot.revision = revision;
        Ok(CommandResult::Transport(self.playback.snapshot().into()))
    }

    fn set_loop(&mut self, enabled: bool) -> Result<CommandResult, CommandError> {
        if enabled != self.snapshot.loop_enabled {
            let revision = self.snapshot.revision.checked_add(1).ok_or_else(|| {
                CommandError::new(ErrorCode::RevisionExhausted, "Session revision exhausted")
            })?;
            self.snapshot.loop_enabled = enabled;
            self.snapshot.revision = revision;
            self.synchronize_output(None);
        }
        Ok(CommandResult::Snapshot(self.snapshot()))
    }

    pub(crate) fn failure(&self, id: Option<String>, error: CommandError) -> Response {
        self.response(id, Outcome::Error { error })
    }

    pub(crate) fn success(&self, id: Option<String>, result: CommandResult) -> Response {
        self.response(id, Outcome::Ok { result })
    }

    fn response(&self, id: Option<String>, outcome: Outcome) -> Response {
        Response {
            protocol_version: PROTOCOL_VERSION,
            id,
            session_id: self.snapshot.session_id.clone(),
            revision: self.snapshot.revision,
            outcome,
        }
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}
