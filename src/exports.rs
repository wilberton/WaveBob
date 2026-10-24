//! Output settings, bounded encoding and atomic publication, independent of GUI/devices.
use crate::{
    CommandError, ErrorCode,
    audio::{self, Audio},
    documents::FrameRange,
    effects::EditTarget,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
mod mp3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WavEncoding {
    Pcm16,
    #[default]
    Pcm24,
    Pcm32,
    Float32,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExportFormat {
    Wav {
        #[serde(default)]
        encoding: WavEncoding,
    },
    Mp3 {
        #[serde(default = "default_bitrate")]
        bitrate_kbps: u16,
    },
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SaveFormat {
    Wav {
        #[serde(default)]
        encoding: WavEncoding,
    },
    Mp3 {
        bitrate_kbps: u16,
    },
}
impl From<SaveFormat> for ExportFormat {
    fn from(value: SaveFormat) -> Self {
        match value {
            SaveFormat::Wav { encoding } => Self::Wav { encoding },
            SaveFormat::Mp3 { bitrate_kbps } => Self::Mp3 { bitrate_kbps },
        }
    }
}
impl From<ExportFormat> for SaveFormat {
    fn from(value: ExportFormat) -> Self {
        match value {
            ExportFormat::Wav { encoding } => Self::Wav { encoding },
            ExportFormat::Mp3 { bitrate_kbps } => Self::Mp3 { bitrate_kbps },
        }
    }
}
impl Default for ExportFormat {
    fn default() -> Self {
        Self::Wav {
            encoding: WavEncoding::Pcm24,
        }
    }
}
fn default_bitrate() -> u16 {
    192
}
pub fn converted_frames(
    frames: u64,
    source_rate: u32,
    output_rate: u32,
) -> Result<u64, CommandError> {
    if source_rate == 0 || output_rate == 0 {
        Err(invalid("Audio conversion requires nonzero sample rates"))
    } else {
        Ok(audio::resample::frame_count(
            frames,
            source_rate,
            output_rate,
        ))
    }
}
impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Wav { .. } => "wav",
            Self::Mp3 { .. } => "mp3",
        }
    }
    pub fn lossy(&self) -> bool {
        matches!(self, Self::Mp3 { .. })
    }
    pub fn source_wav(encoding: &str) -> Option<Self> {
        Some(Self::Wav {
            encoding: match encoding {
                "wav_pcm16" => WavEncoding::Pcm16,
                "wav_pcm24" => WavEncoding::Pcm24,
                "wav_pcm32" => WavEncoding::Pcm32,
                "wav_float32" => WavEncoding::Float32,
                _ => return None,
            },
        })
    }
    pub fn output_rate(&self, source: u32, requested: Option<u32>) -> Result<u32, CommandError> {
        if let Self::Mp3 { bitrate_kbps } = self
            && ![128, 192, 256, 320].contains(bitrate_kbps)
        {
            return Err(invalid("MP3 bitrate must be 128, 192, 256 or 320 kbit/s"));
        }
        let rate = requested.unwrap_or_else(|| {
            if self.lossy() && ![32000, 44100, 48000].contains(&source) {
                44100
            } else {
                source
            }
        });
        if rate == 0 || rate > 768000 || (self.lossy() && ![32000, 44100, 48000].contains(&rate)) {
            return Err(invalid(
                "MP3 output rates are 32000, 44100 and 48000 Hz; WAV rates must be 1–768000 Hz",
            ));
        }
        Ok(rate)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverloadPolicy {
    #[default]
    Reject,
    Clip,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportParams {
    pub target: EditTarget,
    pub path: String,
    #[serde(default)]
    pub format: ExportFormat,
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub overload: OverloadPolicy,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveParams {
    pub document_id: String,
    pub revision_id: String,
    pub path: Option<String>,
    pub format: Option<SaveFormat>,
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub overload: OverloadPolicy,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MixExportParams {
    pub revision_id: String,
    pub start_frame: u64,
    pub end_frame: u64,
    pub path: String,
    #[serde(default)]
    pub format: ExportFormat,
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub overload: OverloadPolicy,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct ExportSummary {
    pub path: String,
    pub source_revision_id: String,
    pub format: ExportFormat,
    pub source_rate: u32,
    pub sample_rate: u32,
    pub channels: u16,
    pub source_frames: u64,
    #[serde(default)]
    pub source_start_frame: u64,
    #[serde(default)]
    pub source_end_frame: u64,
    pub frames: u64,
    pub lossy: bool,
    pub resampled: bool,
    pub overloaded_samples: u64,
    pub clipped_samples: u64,
    pub bytes: u64,
    pub encoder_delay: Option<u32>,
    pub encoder_padding: Option<u32>,
}
pub(crate) struct PreparedOutput {
    temporary: tempfile::NamedTempFile,
    destination: PathBuf,
    pub summary: ExportSummary,
}
impl PreparedOutput {
    pub fn publish(self, overwrite: bool) -> Result<ExportSummary, CommandError> {
        let result = if overwrite {
            self.temporary.persist(&self.destination)
        } else {
            self.temporary.persist_noclobber(&self.destination)
        };
        result.map_err(|error| {
            CommandError::new(
                if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                    ErrorCode::OutputExists
                } else {
                    ErrorCode::IoError
                },
                format!("Publish output: {}", error.error),
            )
        })?;
        Ok(self.summary)
    }
}
pub(crate) struct OutputRequest {
    pub path: String,
    pub revision_id: String,
    pub format: ExportFormat,
    pub rate: u32,
    pub overwrite: bool,
    pub overload: OverloadPolicy,
    pub protected_source: Option<String>,
    pub protected_sources: Vec<String>,
    pub protected_directories: Vec<String>,
}
pub(crate) fn prepare(
    audio: &Audio,
    range: FrameRange,
    request: OutputRequest,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<PreparedOutput, CommandError> {
    if range.start >= range.end || range.end > audio.frames() {
        return Err(invalid(
            "Output requires a nonempty interval within the audio",
        ));
    }
    let rate = request.rate;
    prepare_from(
        audio.rate,
        audio.channels,
        range.clone(),
        request,
        cancel,
        progress,
        |progress| audio::resample::convert(audio, &range, rate, audio.channels, cancel, progress),
    )
}
pub(crate) fn prepare_mix(
    graph: &audio::mix::Graph,
    range: FrameRange,
    request: OutputRequest,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<PreparedOutput, CommandError> {
    if range.start >= range.end || range.end > crate::arrangement::MAX_TIMELINE_FRAME {
        return Err(invalid(
            "Mix export requires a nonempty project-frame interval within 24 hours",
        ));
    }
    let rate = request.rate;
    if graph.streaming() && range.end - range.start > 64 * 1024 * 1024 {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Streaming mix export exceeds the 512 MiB PCM interval limit",
        ));
    }
    prepare_from(
        graph.rate,
        2,
        range.clone(),
        request,
        cancel,
        progress,
        |progress| {
            let rendered = if graph.streaming() {
                let mut stream = audio::mix::Stream::prepare(graph, range.start, None, || {
                    cancel.load(Ordering::Acquire)
                })?;
                let mut pcm = Vec::with_capacity((range.end - range.start) as usize);
                for frame in range.start..range.end {
                    if frame.is_multiple_of(1024) && cancel.load(Ordering::Acquire) {
                        return Err(CommandError::new(
                            ErrorCode::Cancelled,
                            "Mix export cancelled",
                        ));
                    }
                    pcm.push(stream.values[0]);
                    stream.advance(graph, 1);
                }
                Some(pcm)
            } else {
                None
            };
            audio::resample::convert_from(
                range.end - range.start,
                graph.rate,
                rate,
                2,
                cancel,
                progress,
                |frame| {
                    rendered
                        .as_ref()
                        .map_or_else(
                            || graph.frame(range.start + frame),
                            |pcm| pcm[frame as usize],
                        )
                        .map(f64::from)
                },
            )
        },
    )
}
fn prepare_from(
    source_rate: u32,
    channels: u16,
    range: FrameRange,
    request: OutputRequest,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
    render: impl FnOnce(&dyn Fn(u8)) -> Result<Vec<f32>, CommandError>,
) -> Result<PreparedOutput, CommandError> {
    request
        .format
        .output_rate(source_rate, Some(request.rate))?;
    let path = Path::new(&request.path);
    if path.file_name().is_none()
        || !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(request.format.extension()))
    {
        return Err(invalid(
            "Choose an output filename with the selected format's extension",
        ));
    }
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(io_error)?;
    let destination = parent.join(path.file_name().unwrap());
    if request
        .protected_directories
        .iter()
        .any(|root| parent.starts_with(root))
    {
        return Err(invalid(
            "Managed project assets are immutable; choose an output path outside the asset directory",
        ));
    }
    if !request.overwrite && destination.try_exists().map_err(io_error)? {
        return Err(CommandError::new(
            ErrorCode::OutputExists,
            "Output already exists; explicit overwrite intent is required",
        ));
    }
    if let Some(source) = &request.protected_source
        && Path::new(source)
            .canonicalize()
            .ok()
            .zip(destination.canonicalize().ok())
            .is_some_and(|(source, destination)| source == destination)
    {
        return Err(invalid(
            "Export cannot replace the document's backing file; use whole-document Save with explicit settings",
        ));
    }
    if let Ok(destination) = destination.canonicalize()
        && request.protected_sources.iter().any(|source| {
            Path::new(source)
                .canonicalize()
                .is_ok_and(|source| source == destination)
        })
    {
        return Err(invalid(
            "Export cannot replace an open document's backing file",
        ));
    }
    check_cancel(cancel)?;
    let mut samples = render(&|value| progress(value / 2))?;
    let overloaded_samples = samples.iter().filter(|sample| sample.abs() > 1.0).count() as u64;
    let preserves_overloads = matches!(
        request.format,
        ExportFormat::Wav {
            encoding: WavEncoding::Float32
        }
    );
    if overloaded_samples > 0 && !preserves_overloads && request.overload == OverloadPolicy::Reject
    {
        return Err(invalid(
            "Output contains overloads; choose explicit clipping or 32-bit float WAV",
        ));
    }
    let clipped_samples = if preserves_overloads {
        0
    } else {
        overloaded_samples
    };
    if !preserves_overloads {
        for sample in &mut samples {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }
    let frames = samples.len() as u64 / channels as u64;
    let mut temporary = tempfile::NamedTempFile::new_in(&parent).map_err(io_error)?;
    let (encoder_delay, encoder_padding) = match request.format {
        ExportFormat::Wav { encoding } => {
            write_wav(
                temporary.as_file_mut(),
                &samples,
                request.rate,
                channels,
                encoding,
                cancel,
                |value| progress(40 + value / 2),
            )?;
            (None, None)
        }
        ExportFormat::Mp3 { bitrate_kbps } => {
            let (delay, padding) = mp3::encode(
                temporary.as_file_mut(),
                &samples,
                request.rate,
                channels,
                bitrate_kbps,
                cancel,
                |value| progress(40 + value / 2),
            )?;
            (Some(delay), Some(padding))
        }
    };
    check_cancel(cancel)?;
    temporary.as_file_mut().flush().map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    progress(95);
    let bytes = temporary.as_file().metadata().map_err(io_error)?.len();
    let path = destination
        .to_str()
        .ok_or_else(|| invalid("The output path is not valid Unicode"))?
        .to_string();
    Ok(PreparedOutput {
        temporary,
        destination,
        summary: ExportSummary {
            path,
            source_revision_id: request.revision_id,
            lossy: request.format.lossy(),
            format: request.format,
            source_rate,
            sample_rate: request.rate,
            channels,
            source_frames: range.end - range.start,
            source_start_frame: range.start,
            source_end_frame: range.end,
            frames,
            resampled: source_rate != request.rate,
            overloaded_samples,
            clipped_samples,
            bytes,
            encoder_delay,
            encoder_padding,
        },
    })
}
pub(crate) fn write_wav(
    file: &mut std::fs::File,
    samples: &[f32],
    rate: u32,
    channels: u16,
    encoding: WavEncoding,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<(), CommandError> {
    let bits = match encoding {
        WavEncoding::Pcm16 => 16,
        WavEncoding::Pcm24 => 24,
        WavEncoding::Pcm32 | WavEncoding::Float32 => 32,
    };
    // Hound writes individual sample values. Buffer those writes so large WAV
    // exports and managed assets do not issue a system call per sample. Keep the
    // buffer borrowed so its final flush error is checked before publication.
    let mut buffer = std::io::BufWriter::with_capacity(256 * 1024, file);
    let mut writer = hound::WavWriter::new(
        &mut buffer,
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: if encoding == WavEncoding::Float32 {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        },
    )
    .map_err(|error| encode_error(error.to_string()))?;
    for (index, chunk) in samples.chunks(16384).enumerate() {
        check_cancel(cancel)?;
        for sample in chunk {
            if encoding == WavEncoding::Float32 {
                writer.write_sample(*sample)
            } else {
                let scale = (1_u64 << (bits - 1)) as f64;
                let value = (*sample as f64 * scale).round().clamp(-scale, scale - 1.0) as i32;
                writer.write_sample(value)
            }
            .map_err(|error| encode_error(error.to_string()))?;
        }
        progress((index * 16384 * 100 / samples.len().max(1)).min(100) as u8);
    }
    writer
        .finalize()
        .map_err(|error| encode_error(error.to_string()))?;
    buffer.flush().map_err(io_error)
}
fn invalid(message: &str) -> CommandError {
    CommandError::new(ErrorCode::InvalidParams, message)
}
pub(crate) fn io_error(error: std::io::Error) -> CommandError {
    CommandError::new(ErrorCode::IoError, error.to_string())
}
pub(crate) fn encode_error(message: String) -> CommandError {
    CommandError::new(ErrorCode::EncodeFailed, message)
}
pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), CommandError> {
    if cancel.load(Ordering::Acquire) {
        Err(CommandError::new(ErrorCode::Cancelled, "Output cancelled"))
    } else {
        Ok(())
    }
}
