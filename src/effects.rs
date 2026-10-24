//! Explicit editing targets and deterministic offline sample processing.
use crate::{
    CommandError, ErrorCode,
    audio::{self, Audio},
    documents::FrameRange,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditTarget {
    Whole {
        document_id: String,
        revision_id: String,
    },
    Range {
        document_id: String,
        revision_id: String,
        start_frame: u64,
        end_frame: u64,
    },
}
impl EditTarget {
    pub(crate) fn ids(&self) -> (&str, &str) {
        match self {
            Self::Whole {
                document_id,
                revision_id,
            }
            | Self::Range {
                document_id,
                revision_id,
                ..
            } => (document_id, revision_id),
        }
    }
    pub(crate) fn range(&self, frames: u64) -> Result<FrameRange, CommandError> {
        let range = match self {
            Self::Whole { .. } => FrameRange {
                start: 0,
                end: frames,
            },
            Self::Range {
                start_frame,
                end_frame,
                ..
            } => FrameRange {
                start: *start_frame,
                end: *end_frame,
            },
        };
        if range.start >= range.end || range.end > frames {
            Err(invalid(
                "Effects require a nonempty target within the document",
            ))
        } else {
            Ok(range)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Gain { db: f64 },
    FadeIn,
    FadeOut,
    Reverse,
}
impl Default for Effect {
    fn default() -> Self {
        Self::Gain { db: 0.0 }
    }
}
impl Effect {
    pub(crate) fn validate(&self) -> Result<(), CommandError> {
        if let Self::Gain { db } = self
            && (!db.is_finite() || !(-120.0..=60.0).contains(db))
        {
            return Err(invalid("Gain must be finite and between -120 and +60 dB"));
        }
        Ok(())
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Gain { .. } => "Gain",
            Self::FadeIn => "Fade in",
            Self::FadeOut => "Fade out",
            Self::Reverse => "Reverse",
        }
    }
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyEffectParams {
    pub target: EditTarget,
    pub effect: Effect,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct HistoryInfo {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub undo_count: usize,
    pub redo_count: usize,
}
pub(crate) fn process(
    source: &Audio,
    range: FrameRange,
    effect: &Effect,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<Audio, CommandError> {
    effect.validate()?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(source.samples.len())
        .map_err(|_| {
            CommandError::new(ErrorCode::ResourceLimit, "Unable to allocate effect result")
        })?;
    for chunk in source.samples.chunks(16384) {
        if cancel.load(Ordering::Acquire) {
            return Err(CommandError::new(ErrorCode::Cancelled, "Effect cancelled"));
        }
        samples.extend_from_slice(chunk);
    }
    let channels = source.channels as usize;
    let count = range.end - range.start;
    let constant_gain = if let Effect::Gain { db } = effect {
        10.0_f64.powf(db / 20.0)
    } else {
        1.0
    };
    for frame in range.start..range.end {
        if frame.is_multiple_of(16384) {
            if cancel.load(Ordering::Acquire) {
                return Err(CommandError::new(ErrorCode::Cancelled, "Effect cancelled"));
            }
            progress(((frame - range.start) * 89 / count) as u8);
        }
        let position = if count == 1 {
            1.0
        } else {
            (frame - range.start) as f64 / (count - 1) as f64
        };
        let gain = match effect {
            Effect::Gain { .. } => constant_gain,
            Effect::FadeIn => position,
            Effect::FadeOut => {
                if count == 1 {
                    1.0
                } else {
                    1.0 - position
                }
            }
            Effect::Reverse => 1.0,
        };
        let input = if matches!(effect, Effect::Reverse) {
            range.end - 1 - (frame - range.start)
        } else {
            frame
        };
        for channel in 0..channels {
            let value = source.samples[input as usize * channels + channel] as f64 * gain;
            if !value.is_finite() || value.abs() > f32::MAX as f64 {
                return Err(invalid(
                    "Effect exceeds the finite floating-point audio range",
                ));
            }
            samples[frame as usize * channels + channel] = value as f32;
        }
    }
    if cancel.load(Ordering::Acquire) {
        return Err(CommandError::new(ErrorCode::Cancelled, "Effect cancelled"));
    }
    audio::prepare(
        source.rate,
        source.channels,
        source.encoding.clone(),
        source.source.clone(),
        samples,
        cancel,
        progress,
    )
}
fn invalid(message: &str) -> CommandError {
    CommandError::new(ErrorCode::InvalidParams, message)
}
