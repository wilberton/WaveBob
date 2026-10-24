use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::{AuditionVolumeParams, BrowserSnapshot, PathParams};
use crate::documents::*;
use crate::effects::{ApplyEffectParams, HistoryInfo};
use crate::presentation::{
    CaptureParams, CaptureResult, PresentationRevisionParams, PresentationSnapshot, RenderMetadata,
    RenderParams, SetLayoutParams, SetModeParams, SetViewportParams,
};
use crate::transport::TransportSnapshot;
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
    pub requires_audio_output: bool,
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
    ($($variant:ident($params:ty) => ($name:literal, $description:literal, $mutates:literal, $gui:literal, $device:literal, $output:ty)),* $(,)?) => {
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

            pub fn requires_audio_output(&self) -> bool {
                match self { $(Self::$variant(_) => $device),* }
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
                requires_audio_output: $device,
                input_schema: schema_for!($params).to_value(),
                output_schema: schema_for!($output).to_value(),
            }),*]
        }
    };
}

command_registry! {
    GetSession(EmptyParams) => ("session.get", "Inspect the current session state.", false, false, false, SessionSnapshot),
    GetCapabilities(EmptyParams) => ("session.capabilities", "Discover implemented commands and their schemas.", false, false, false, Capabilities),
    SetLoop(LoopParams) => ("transport.loop.set", "Set looping without starting audio playback.", true, false, false, SessionSnapshot),
    ToggleLoop(EmptyParams) => ("transport.loop.toggle", "Toggle looping without starting audio playback.", true, false, false, SessionSnapshot),
    OpenDocument(OpenParams) => ("documents.open", "Import an audio file as a background job without changing its source.", true, false, false, JobInfo),
    GetDocument(DocumentParams) => ("documents.get", "Inspect an open audio document and its immutable revision.", false, false, false, DocumentInfo),
    ActivateDocument(DocumentParams) => ("documents.activate", "Choose the active audio document.", true, false, false, DocumentInfo),
    CloseDocument(CloseParams) => ("documents.close", "Close an audio document; dirty work requires explicit discard intent.", true, false, false, DocumentInfo),
    SetCursor(CursorParams) => ("documents.cursor.set", "Place the edit cursor and clear the selection.", true, false, false, DocumentInfo),
    SetSelection(SelectionParams) => ("documents.selection.set", "Set a normalized half-open frame selection across all channels.", true, false, false, DocumentInfo),
    SelectAll(DocumentParams) => ("documents.selection.all", "Select all frames and place the cursor at the start.", true, false, false, DocumentInfo),
    ClearSelection(DocumentParams) => ("documents.selection.clear", "Clear the selection without moving the cursor.", true, false, false, DocumentInfo),
    ReadSamples(SamplesParams) => ("documents.samples", "Read a bounded range of actual decoded channel samples.", false, false, false, SamplesData),
    GetWaveform(WaveformParams) => ("documents.waveform", "Query exact per-channel waveform extrema for a frame interval.", false, false, false, WaveformData),
    GetJob(JobParams) => ("jobs.get", "Inspect background operation progress, result or failure.", false, false, false, JobInfo),
    CancelJob(JobParams) => ("jobs.cancel", "Cancel unfinished background work without publishing partial audio.", true, false, false, JobInfo),
    GetTransport(EmptyParams) => ("transport.get", "Inspect requested and applied playback, position and device state.", false, false, false, TransportSnapshot),
    PlayDocument(DocumentParams) => ("transport.play", "Play the specified document from its cursor; already playing is a no-op.", true, false, true, TransportSnapshot),
    TogglePlayback(DocumentParams) => ("transport.play_pause", "Play or pause the specified document.", true, false, true, TransportSnapshot),
    PausePlayback(EmptyParams) => ("transport.pause", "Pause playback at the applied position without changing selection.", true, false, true, TransportSnapshot),
    SeekDocument(CursorParams) => ("transport.seek", "Seek a document without clearing its selection or changing playing/paused state.", true, false, false, DocumentInfo),
    RewindDocument(DocumentParams) => ("transport.rewind", "Seek a document to absolute start without changing its playing/paused state.", true, false, false, DocumentInfo),
    RetryOutput(EmptyParams) => ("transport.output.retry", "Retry recoverable audio device initialization or failure.", true, false, true, TransportSnapshot),
    GetBrowser(EmptyParams) => ("browser.get", "Inspect folder contents, selected file and independent audition.", false, false, false, BrowserSnapshot),
    BrowseFolder(PathParams) => ("browser.navigate", "List a folder asynchronously, retaining navigation after errors.", true, false, false, BrowserSnapshot),
    SelectBrowserFile(PathParams) => ("browser.select", "Select a listed file without opening or auditioning it.", true, false, false, BrowserSnapshot),
    OpenBrowserFile(PathParams) => ("browser.open", "Stop audition and open the specified browser audio file.", true, false, false, JobInfo),
    StartAudition(PathParams) => ("audition.start", "Decode and preview WAV or MP3 without creating an audio document.", true, false, true, BrowserSnapshot),
    StopAudition(EmptyParams) => ("audition.stop", "Cancel or stop browser preview without resuming editor playback.", true, false, false, BrowserSnapshot),
    SetAuditionVolume(AuditionVolumeParams) => ("audition.volume.set", "Set independent browser preview volume from zero to one.", true, false, false, BrowserSnapshot),
    ApplyEffect(ApplyEffectParams) => ("effects.apply", "Apply gain, fade or reverse to an explicit immutable audio target as one undoable background edit.", true, false, false, JobInfo),
    GetHistory(EmptyParams) => ("history.get", "Inspect chronological session undo and redo availability.", false, false, false, HistoryInfo),
    Undo(EmptyParams) => ("history.undo", "Undo the latest committed audio edit.", true, false, false, HistoryInfo),
    Redo(EmptyParams) => ("history.redo", "Redo the latest undone audio edit.", true, false, false, HistoryInfo),
    GetPresentation(EmptyParams) => ("presentation.get", "Inspect GUI mode and presentation revision.", false, true, false, PresentationSnapshot),
    SetMode(SetModeParams) => ("presentation.set_mode", "Select waveform or multitrack mode.", true, true, false, PresentationSnapshot),
    SetViewport(SetViewportParams) => ("presentation.viewport.set", "Set a document's visible frame interval without changing its selection or audio.", true, true, false, PresentationSnapshot),
    SetLayout(SetLayoutParams) => ("presentation.layout.set", "Set panel widths, visibility and library/browser split.", true, true, false, PresentationSnapshot),
    TogglePanels(PresentationRevisionParams) => ("presentation.panels.toggle", "Hide both panels if either is visible; otherwise restore both.", true, true, false, PresentationSnapshot),
    RenderBarrier(RenderParams) => ("presentation.barrier", "Wait for a frame reflecting the requested revisions.", false, true, false, RenderMetadata),
    CapturePng(CaptureParams) => ("capture.png", "Capture the actual GUI client area as PNG.", false, true, false, CaptureResult),
}

pub(crate) fn capabilities(gui: bool, device: bool) -> Capabilities {
    let mut capabilities = vec![
        "session".into(),
        "transport.loop".into(),
        "documents".into(),
        "waveform".into(),
        "jobs".into(),
        "selection".into(),
        "transport".into(),
        "browser".into(),
        "effects".into(),
        "history".into(),
    ];
    if device {
        capabilities.push("audio.output".into());
    }
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
            .filter(|command| {
                (gui || !command.requires_gui) && (device || !command.requires_audio_output)
            })
            .collect(),
    }
}
