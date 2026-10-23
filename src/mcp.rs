//! MCP framing and content delivery over the existing application dispatcher.

use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::prelude::*;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
    Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Capabilities, CommandError, CommandResult, ErrorCode, Outcome, Response, Session};
use crate::{ipc, protocol};

// Leave room for base64 and metadata under the SDK's default 16 MiB line limit.
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToolArguments {
    /// Command-specific parameters, validated by the shared command layer.
    #[serde(default = "empty_params")]
    params: Value,
    /// Reject a mutation if the core session revision has changed.
    expected_revision: Option<u64>,
    /// Optional guard against sending a request to an unintended session.
    session_id: Option<String>,
}

fn empty_params() -> Value {
    json!({})
}

enum Target {
    Headless(Box<Session>),
    Attached(ipc::Client),
}

impl Target {
    fn dispatch(&mut self, line: &str) -> io::Result<Response> {
        match self {
            Self::Headless(session) => Ok(protocol::dispatch_line(session, line)),
            Self::Attached(client) => client.call(line),
        }
    }
}

/// One fixed session per MCP server process. Reconnection never selects another
/// GUI or creates a replacement headless session after an attachment failure.
pub struct McpServer {
    target: Arc<Mutex<Target>>,
    session_id: String,
    tools: Vec<Tool>,
}

impl McpServer {
    pub fn headless() -> io::Result<Self> {
        Self::new(Target::Headless(Box::default()))
    }

    pub fn attach(directory: &Path, session_id: &str) -> io::Result<Self> {
        Self::new(Target::Attached(ipc::Client::attach(
            directory, session_id,
        )?))
    }

    fn new(mut target: Target) -> io::Result<Self> {
        let response =
            target.dispatch(r#"{"id":"mcp-startup","command":"session.capabilities"}"#)?;
        let Outcome::Ok {
            result: CommandResult::Capabilities(catalogue),
        } = response.outcome
        else {
            return Err(io::Error::other(
                "Target did not return its command catalogue",
            ));
        };
        let tools = command_tools(&catalogue);
        Ok(Self {
            target: Arc::new(Mutex::new(target)),
            session_id: response.session_id,
            tools,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

fn command_tools(catalogue: &Capabilities) -> Vec<Tool> {
    catalogue
        .commands
        .iter()
        .map(|command| {
            let mut schema = schema_for!(ToolArguments).to_value();
            let mut params = command.input_schema.clone();
            // Schemars references are rooted at the document, even when the schema
            // is reused inside the MCP argument envelope (e.g. the mode enum).
            if let Some(definitions) = params.as_object_mut().unwrap().remove("$defs") {
                schema["$defs"] = definitions;
            }
            params.as_object_mut().unwrap().remove("$schema");
            if params
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|fields| !fields.is_empty())
            {
                schema["required"] = json!(["params"]);
            }
            schema["properties"]["params"] = params;
            Tool::new(
                command.name.clone(),
                command.description.clone(),
                schema.as_object().unwrap().clone(),
            )
            .with_raw_output_schema(Arc::new(
                catalogue.response_schema.as_object().unwrap().clone(),
            ))
        })
        .collect()
}

fn tool_result(mut response: Response) -> CallToolResult {
    let image = if let Outcome::Ok {
        result: CommandResult::Capture(capture),
    } = &response.outcome
    {
        match read_image(&capture.path) {
            Ok(bytes) => Some(ContentBlock::image(
                BASE64_STANDARD.encode(bytes),
                &capture.mime_type,
            )),
            Err(error) => {
                response.outcome = Outcome::Error {
                    error: CommandError {
                        code: ErrorCode::IoError,
                        message: format!(
                            "Capture was saved, but MCP image delivery failed: {error}"
                        ),
                        details: Some(json!({"capture":capture})),
                    },
                };
                None
            }
        }
    } else {
        None
    };
    let failed = matches!(response.outcome, Outcome::Error { .. });
    let value = serde_json::to_value(response).expect("Command responses serialize to JSON");
    let mut result = if failed {
        CallToolResult::structured_error(value)
    } else {
        CallToolResult::structured(value)
    };
    if let Some(image) = image {
        result.content.push(image);
    }
    result
}

fn read_image(path: &str) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(io::Error::other(
            "PNG exceeds the 8 MiB MCP image limit; use the saved artifact",
        ));
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(io::Error::other("Capture artifact is not a PNG"));
    }
    Ok(bytes)
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("wavebob", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "All tools target WaveBob session {}. Tool arguments wrap command parameters in params; optional session_id and expected_revision guard the target and core revision. session.capabilities describes implemented commands, including any registered job queries. capture.png returns actual PNG image content plus revision metadata when attached to a GUI.",
                self.session_id
            ))
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|params| params.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "This catalogue has only one page",
                None,
            ));
        }
        Ok(ListToolsResult::with_all_items(self.tools.clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let arguments: ToolArguments = serde_json::from_value(Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|error| {
            ErrorData::invalid_params(error.to_string(), Some(json!({"code":"invalid_params"})))
        })?;
        if let Some(expected) = arguments.session_id
            && expected != self.session_id
        {
            return Err(ErrorData::invalid_params(
                "Session identity mismatch",
                Some(json!({
                    "code":"invalid_request", "expected_session_id":expected, "actual_session_id":self.session_id
                })),
            ));
        }
        let line = json!({
            "id":context.id.to_string(), "command":request.name,
            "params":arguments.params, "expected_revision":arguments.expected_revision
        })
        .to_string();
        let target = Arc::clone(&self.target);
        // IPC waits and image encoding/file reads must not stall MCP's stdio
        // reader. Serialize application dispatch even when clients call in parallel.
        let result = tokio::task::spawn_blocking(move || {
            let mut target = target
                .lock()
                .map_err(|_| io::Error::other("Session dispatcher failed"))?;
            target.dispatch(&line).map(tool_result)
        })
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?
        .map_err(|error| {
            ErrorData::internal_error(
                "WaveBob session transport failed",
                Some(json!({
                    "code":"io_error", "session_id":self.session_id, "message":error.to_string()
                })),
            )
        })?;
        Ok(result.into())
    }
}
