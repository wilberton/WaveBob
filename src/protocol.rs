//! Newline-delimited JSON adapter over the typed application interface.

use std::io::{self, BufRead, Write};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::{Command, CommandError, ErrorCode, PROTOCOL_VERSION, Request, Response, Session};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireRequest {
    #[schemars(length(min = 1))]
    id: String,
    #[serde(default = "protocol_version")]
    protocol_version: u32,
    command: String,
    #[serde(default = "empty_params")]
    params: Value,
    #[serde(default)]
    expected_revision: Option<u64>,
}

fn protocol_version() -> u32 {
    PROTOCOL_VERSION
}

fn empty_params() -> Value {
    serde_json::json!({})
}

/// Decode one frame and dispatch it through the same interface as the GUI.
/// Invalid frames yield a correlated error where an id can be recovered.
pub fn dispatch_line(session: &mut Session, line: &str) -> Response {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return session.failure(
                None,
                CommandError::new(ErrorCode::InvalidJson, error.to_string()),
            );
        }
    };
    let id = value.get("id").and_then(Value::as_str).map(str::to_owned);
    let wire: WireRequest = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(error) => {
            return session.failure(
                id,
                CommandError::new(ErrorCode::InvalidRequest, error.to_string()),
            );
        }
    };
    if wire.protocol_version != PROTOCOL_VERSION {
        return session.failure(
            Some(wire.id),
            CommandError::new(
                ErrorCode::UnsupportedProtocol,
                "Supported protocol version is 1",
            ),
        );
    }
    if !wire.params.is_object() {
        return session.failure(
            Some(wire.id),
            CommandError::new(ErrorCode::InvalidParams, "Command params must be an object"),
        );
    }
    match Command::decode(&wire.command, wire.params) {
        Ok(command) => session.execute(Request {
            id: wire.id,
            command,
            expected_revision: wire.expected_revision,
        }),
        Err(error) => session.failure(Some(wire.id), error),
    }
}

/// Process one JSON object per nonblank line, flushing one response per request.
/// A malformed frame does not poison subsequent requests. EOF ends the session.
pub fn run(session: &mut Session, reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
    for frame in reader.split(b'\n') {
        let frame = frame?;
        let response = match std::str::from_utf8(&frame) {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => dispatch_line(session, line),
            Err(error) => session.failure(
                None,
                CommandError::new(ErrorCode::InvalidJson, error.to_string()),
            ),
        };
        serde_json::to_writer(&mut writer, &response)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    Ok(())
}
