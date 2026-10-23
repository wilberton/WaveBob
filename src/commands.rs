use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CommandError, ErrorCode, PROTOCOL_VERSION, SessionSnapshot};

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopParams {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct CommandDescriptor {
    pub name: String,
    pub description: String,
    pub mutates: bool,
    /// Schema for the request's params object.
    pub input_schema: Value,
    /// Schema for the successful response's result value.
    pub output_schema: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
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
    ($($variant:ident($params:ty) => ($name:literal, $description:literal, $mutates:literal, $output:ty)),* $(,)?) => {
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
                input_schema: schema_for!($params).to_value(),
                output_schema: schema_for!($output).to_value(),
            }),*]
        }
    };
}

command_registry! {
    GetSession(EmptyParams) => ("session.get", "Inspect the current session state.", false, SessionSnapshot),
    GetCapabilities(EmptyParams) => ("session.capabilities", "Discover implemented commands and their schemas.", false, Capabilities),
    SetLoop(LoopParams) => ("transport.loop.set", "Set looping without starting audio playback.", true, SessionSnapshot),
    ToggleLoop(EmptyParams) => ("transport.loop.toggle", "Toggle looping without starting audio playback.", true, SessionSnapshot),
}

pub(crate) fn capabilities() -> Capabilities {
    Capabilities {
        protocol_version: PROTOCOL_VERSION,
        capabilities: vec!["session".into(), "transport.loop".into()],
        request_schema: schema_for!(crate::protocol::WireRequest).to_value(),
        response_schema: schema_for!(crate::Response).to_value(),
        commands: descriptors(),
    }
}
