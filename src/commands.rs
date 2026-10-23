use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::presentation::{
    CaptureParams, CaptureResult, PresentationSnapshot, RenderMetadata, RenderParams, SetModeParams,
};
use crate::{CommandError, ErrorCode, PROTOCOL_VERSION, SessionSnapshot};

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopParams {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct CommandDescriptor {
    pub name: String,
    pub description: String,
    pub mutates: bool,
    pub requires_gui: bool,
    /// Schema for the request's params object.
    pub input_schema: Value,
    /// Schema for the successful response's result value.
    pub output_schema: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Capabilities {
    pub protocol_version: u32,
    pub capabilities: Vec<String>,
    pub request_schema: Value,
    pub response_schema: Value,
    pub commands: Vec<CommandDescriptor>,
}

// A command is registered once: its typed variant, wire name, validation,
// description and schemas are generated together for every adapter.
macro_rules! command_registry {
    ($($variant:ident($params:ty) => ($name:literal, $description:literal, $mutates:literal, $gui:literal, $output:ty)),* $(,)?) => {
        #[derive(Clone, Debug)]
        pub enum Command {
            $($variant($params)),*
        }

        impl Command {
            pub fn name(&self) -> &'static str {
                match self { $(Self::$variant(_) => $name),* }
            }

            pub fn mutates(&self) -> bool {
                match self { $(Self::$variant(_) => $mutates),* }
            }

            pub(crate) fn decode(name: &str, params: Value) -> Result<Self, CommandError> {
                match name {
                    $($name => serde_json::from_value::<$params>(params)
                        .map(Self::$variant)
                        .map_err(|error| CommandError::new(ErrorCode::InvalidParams, error.to_string()))),*,
                    _ => Err(CommandError::new(ErrorCode::UnknownCommand, format!("Unknown command: {name}"))),
                }
            }
        }

        fn descriptors() -> Vec<CommandDescriptor> {
            vec![$(CommandDescriptor {
                name: $name.into(),
                description: $description.into(),
                mutates: $mutates,
                requires_gui: $gui,
                input_schema: schema_for!($params).to_value(),
                output_schema: schema_for!($output).to_value(),
            }),*]
        }
    };
}

command_registry! {
    GetSession(EmptyParams) => ("session.get", "Inspect the current session state.", false, false, SessionSnapshot),
    GetCapabilities(EmptyParams) => ("session.capabilities", "Discover implemented commands and their schemas.", false, false, Capabilities),
    SetLoop(LoopParams) => ("transport.loop.set", "Set looping without starting audio playback.", true, false, SessionSnapshot),
    ToggleLoop(EmptyParams) => ("transport.loop.toggle", "Toggle looping without starting audio playback.", true, false, SessionSnapshot),
    GetPresentation(EmptyParams) => ("presentation.get", "Inspect GUI mode and presentation revision.", false, true, PresentationSnapshot),
    SetMode(SetModeParams) => ("presentation.set_mode", "Select waveform or multitrack mode.", true, true, PresentationSnapshot),
    RenderBarrier(RenderParams) => ("presentation.barrier", "Wait for a frame reflecting the requested revisions.", false, true, RenderMetadata),
    CapturePng(CaptureParams) => ("capture.png", "Capture the actual GUI client area as PNG.", false, true, CaptureResult),
}

pub(crate) fn capabilities(gui: bool) -> Capabilities {
    let mut capabilities = vec!["session".into(), "transport.loop".into()];
    if gui {
        capabilities.extend(["presentation".into(), "capture.png".into()]);
    }
    Capabilities {
        protocol_version: PROTOCOL_VERSION,
        capabilities,
        request_schema: schema_for!(crate::protocol::WireRequest).to_value(),
        response_schema: schema_for!(crate::Response).to_value(),
        commands: descriptors()
            .into_iter()
            .filter(|command| gui || !command.requires_gui)
            .collect(),
    }
}
