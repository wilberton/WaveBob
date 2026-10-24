//! Immutable decoded audio and revision-local waveform acceleration.
use crate::documents::Peak;
use crate::documents::SourceDetails;
use crate::{CommandError, ErrorCode};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const BLOCK: usize = 256;
const MAX_VALUES: usize = 128 * 1024 * 1024;
pub(crate) mod mix;
mod mp3;
pub(crate) mod resample;

#[derive(Debug)]
pub(crate) struct Audio {
    pub rate: u32,
    pub channels: u16,
    pub encoding: String,
    pub source: SourceDetails,
    pub samples: Arc<[f32]>,
    pub peak_level: f64,
    pub overloaded_samples: u64,
    peaks: Vec<Vec<Vec<(f32, f32)>>>,
}

impl Audio {
    pub fn frames(&self) -> u64 {
        self.samples.len() as u64 / self.channels as u64
    }
    pub fn waveform(&self, start: u64, end: u64, bins: usize) -> Vec<Vec<Peak>> {
        let count = bins.min((end - start) as usize);
        (0..self.channels as usize)
            .map(|channel| {
                (0..count)
                    .map(|bin| {
                        let first = start + (end - start) * bin as u64 / count as u64;
                        let last = start + (end - start) * (bin + 1) as u64 / count as u64;
                        let (min, max) = self.extrema(channel, first as usize, last as usize);
                        Peak {
                            start_frame: first,
                            end_frame: last,
                            min: min as f64,
                            max: max as f64,
                        }
                    })
                    .collect()
            })
            .collect()
    }
    fn extrema(&self, channel: usize, mut start: usize, end: usize) -> (f32, f32) {
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        while start < end {
            let mut consumed = false;
            for level in (0..self.peaks[channel].len()).rev() {
                let span = BLOCK << level;
                if start.is_multiple_of(span) && start + span <= end {
                    let (a, b) = self.peaks[channel][level][start / span];
                    min = min.min(a);
                    max = max.max(b);
                    start += span;
                    consumed = true;
                    break;
                }
            }
            if !consumed {
                let value = self.samples[start * self.channels as usize + channel];
                min = min.min(value);
                max = max.max(value);
                start += 1;
            }
        }
        (min, max)
    }
}

pub(crate) fn decode(
    path: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<Audio, CommandError> {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
    {
        return mp3::decode(path, cancel, &progress);
    }
    let error = |error: hound::Error| match error {
        hound::Error::IoError(error) => CommandError::new(
            if error.kind() == std::io::ErrorKind::UnexpectedEof
                || error.to_string() == "Failed to read enough bytes."
            {
                ErrorCode::DecodeFailed
            } else {
                ErrorCode::IoError
            },
            error.to_string(),
        ),
        error => CommandError::new(ErrorCode::DecodeFailed, error.to_string()),
    };
    let mut reader = hound::WavReader::open(path).map_err(error)?;
    let spec = reader.spec();
    if ![1, 2].contains(&spec.channels)
        || spec.sample_rate == 0
        || !matches!(
            (spec.sample_format, spec.bits_per_sample),
            (hound::SampleFormat::Int, 16 | 24 | 32) | (hound::SampleFormat::Float, 32)
        )
    {
        return Err(CommandError::new(
            ErrorCode::UnsupportedFormat,
            "Supported WAV encodings are mono/stereo PCM 16/24/32-bit or float32",
        ));
    }
    let length = reader.len() as usize;
    if length == 0 {
        return Err(CommandError::new(
            ErrorCode::DecodeFailed,
            "The audio file is empty",
        ));
    }
    if length > MAX_VALUES {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Decoded audio exceeds the current 512 MiB import limit",
        ));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(CommandError::new(ErrorCode::Cancelled, "Import cancelled"));
    }
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(length)
        .map_err(|error| CommandError::new(ErrorCode::ResourceLimit, error.to_string()))?;
    let divisor = 2.0_f64.powi(spec.bits_per_sample as i32 - 1);
    let values: Box<dyn Iterator<Item = Result<f32, hound::Error>> + '_> = match spec.sample_format
    {
        hound::SampleFormat::Int => Box::new(
            reader
                .samples::<i32>()
                .map(move |sample| sample.map(|value| (value as f64 / divisor) as f32)),
        ),
        hound::SampleFormat::Float => Box::new(reader.samples::<f32>()),
    };
    for (index, sample) in values.enumerate() {
        if index.is_multiple_of(16384) {
            if cancel.load(Ordering::Acquire) {
                return Err(CommandError::new(ErrorCode::Cancelled, "Import cancelled"));
            }
            progress(((index as u64 * 90) / length as u64) as u8);
        }
        let value = sample.map_err(error)?;
        if !value.is_finite() {
            return Err(CommandError::new(
                ErrorCode::DecodeFailed,
                "Audio contains non-finite samples",
            ));
        }
        samples.push(value);
    }
    if samples.len() != length || !length.is_multiple_of(spec.channels as usize) {
        return Err(CommandError::new(
            ErrorCode::DecodeFailed,
            "Incomplete sample frames in WAV data",
        ));
    }
    prepare(
        spec.sample_rate,
        spec.channels,
        format!(
            "wav_{}{}",
            if spec.sample_format == hound::SampleFormat::Int {
                "pcm"
            } else {
                "float"
            },
            spec.bits_per_sample
        ),
        SourceDetails::default(),
        samples,
        cancel,
        progress,
    )
}

pub(crate) fn prepare(
    rate: u32,
    channels: u16,
    encoding: String,
    source: SourceDetails,
    samples: Vec<f32>,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<Audio, CommandError> {
    let mut peaks = Vec::new();
    let mut peak_level = 0.0_f64;
    let mut overloaded_samples = 0_u64;
    for chunk in samples.chunks(16384) {
        if cancel.load(Ordering::Acquire) {
            return Err(CommandError::new(
                ErrorCode::Cancelled,
                "Audio preparation cancelled",
            ));
        }
        for sample in chunk {
            peak_level = peak_level.max(sample.abs() as f64);
            overloaded_samples += u64::from(sample.abs() > 1.0);
        }
    }
    let frames = samples.len() / channels as usize;
    for channel in 0..channels as usize {
        let mut levels = vec![Vec::new()];
        for first in (0..frames).step_by(BLOCK) {
            if cancel.load(Ordering::Acquire) {
                return Err(CommandError::new(ErrorCode::Cancelled, "Import cancelled"));
            }
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for frame in first..(first + BLOCK).min(frames) {
                let sample = samples[frame * channels as usize + channel];
                min = min.min(sample);
                max = max.max(sample);
            }
            levels[0].push((min, max));
            if first.is_multiple_of(16384) {
                progress(
                    90 + ((channel * frames + first) * 9 / (frames * channels as usize).max(1))
                        as u8,
                );
            }
        }
        while levels.last().unwrap().len() > 1 {
            let next = levels
                .last()
                .unwrap()
                .chunks(2)
                .map(|pair| {
                    pair.iter()
                        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), (a, b)| {
                            (min.min(*a), max.max(*b))
                        })
                })
                .collect();
            levels.push(next);
        }
        peaks.push(levels);
        progress(90 + ((channel + 1) * 9 / channels as usize) as u8);
    }
    Ok(Audio {
        rate,
        channels,
        encoding,
        source,
        samples: samples.into(),
        peak_level,
        overloaded_samples,
        peaks,
    })
}
