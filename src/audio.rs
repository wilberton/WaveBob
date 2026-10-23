//! Immutable decoded audio and revision-local waveform acceleration.
use crate::documents::Peak;
use crate::{CommandError, ErrorCode};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const BLOCK: usize = 256;
const MAX_VALUES: usize = 128 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct Audio {
    pub rate: u32,
    pub channels: u16,
    pub encoding: String,
    pub samples: Arc<[f32]>,
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
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(CommandError::new(
            ErrorCode::UnsupportedFormat,
            "This milestone supports mono/stereo 16-bit PCM WAV files",
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
    for (index, sample) in reader.samples::<i16>().enumerate() {
        if index.is_multiple_of(16384) {
            if cancel.load(Ordering::Acquire) {
                return Err(CommandError::new(ErrorCode::Cancelled, "Import cancelled"));
            }
            progress(((index as u64 * 90) / length as u64) as u8);
        }
        samples.push(sample.map_err(error)? as f32 / 32768.0);
    }
    if samples.len() != length || !length.is_multiple_of(spec.channels as usize) {
        return Err(CommandError::new(
            ErrorCode::DecodeFailed,
            "Incomplete sample frames in WAV data",
        ));
    }
    let mut peaks = Vec::new();
    let frames = length / spec.channels as usize;
    for channel in 0..spec.channels as usize {
        let mut levels = vec![Vec::new()];
        for first in (0..frames).step_by(BLOCK) {
            if cancel.load(Ordering::Acquire) {
                return Err(CommandError::new(ErrorCode::Cancelled, "Import cancelled"));
            }
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for frame in first..(first + BLOCK).min(frames) {
                let sample = samples[frame * spec.channels as usize + channel];
                min = min.min(sample);
                max = max.max(sample);
            }
            levels[0].push((min, max));
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
        progress(90 + ((channel + 1) * 9 / spec.channels as usize) as u8);
    }
    Ok(Audio {
        rate: spec.sample_rate,
        channels: spec.channels,
        encoding: "wav_pcm16".into(),
        samples: samples.into(),
        peaks,
    })
}
