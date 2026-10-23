//! The shared application command interface. No window or audio device is required.

mod commands;
#[cfg(feature = "gui")]
pub mod desktop;
pub mod ipc;
pub mod mcp;
pub mod presentation;
pub mod protocol;

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
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
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
#[derive(Debug)]
pub struct Session {
    snapshot: SessionSnapshot,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            snapshot: SessionSnapshot {
                session_id: uuid::Uuid::new_v4().to_string(),
                revision: 0,
                loop_enabled: false,
            },
        }
    }
}

impl Session {
    pub fn snapshot(&self) -> SessionSnapshot {
        self.snapshot.clone()
    }

    pub fn execute(&mut self, request: Request) -> Response {
        if let Err(error) = self.validate(&request) {
            return self.failure(Some(request.id), error);
        }
        let result = match request.command {
            Command::GetSession(_) => Ok(CommandResult::Snapshot(self.snapshot())),
            Command::GetCapabilities(_) => {
                Ok(CommandResult::Capabilities(commands::capabilities(false)))
            }
            Command::SetLoop(params) => self.set_loop(params.enabled),
            Command::ToggleLoop(_) => self.set_loop(!self.snapshot.loop_enabled),
            Command::GetPresentation(_)
            | Command::SetMode(_)
            | Command::RenderBarrier(_)
            | Command::CapturePng(_) => Err(CommandError::new(
                ErrorCode::MissingCapability,
                "This command requires an attached GUI renderer",
            )),
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

        Ok(())
    }

    fn set_loop(&mut self, enabled: bool) -> Result<CommandResult, CommandError> {
        if enabled != self.snapshot.loop_enabled {
            let revision = self.snapshot.revision.checked_add(1).ok_or_else(|| {
                CommandError::new(ErrorCode::RevisionExhausted, "Session revision exhausted")
            })?;
            self.snapshot.loop_enabled = enabled;
            self.snapshot.revision = revision;
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
