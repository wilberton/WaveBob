//! Deterministic offline conversion shared by paste, export and mix preparation.
use super::*;
use crate::documents::FrameRange;
pub(crate) fn frame_count(input: u64, source_rate: u32, rate: u32) -> u64 {
    if input == 0 {
        0
    } else {
        ((input as u128 * rate as u128 + source_rate as u128 / 2) / source_rate as u128)
            .max(1)
            .min(u64::MAX as u128) as u64
    }
}
pub(crate) fn convert(
    source: &Audio,
    range: &FrameRange,
    rate: u32,
    channels: u16,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<Vec<f32>, CommandError> {
    if rate == 0
        || !(1..=2).contains(&channels)
        || range.start > range.end
        || range.end > source.frames()
    {
        return Err(CommandError::new(
            ErrorCode::InvalidParams,
            "Invalid audio conversion range or format",
        ));
    }
    let input = range.end - range.start;
    let frames = frame_count(input, source.rate, rate) as u128;
    if frames * channels as u128 > MAX_VALUES as u128 {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Converted audio exceeds the current 512 MiB PCM limit",
        ));
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(frames as usize * channels as usize)
        .map_err(|_| {
            CommandError::new(
                ErrorCode::ResourceLimit,
                "Unable to allocate converted audio",
            )
        })?;
    let read = |frame: i64, channel: usize| {
        let frame = range.start + frame.clamp(0, input.saturating_sub(1) as i64) as u64;
        let index = frame as usize * source.channels as usize;
        if source.channels == 2 && channels == 1 {
            (source.samples[index] as f64 + source.samples[index + 1] as f64) * 0.5
        } else {
            source.samples[index + channel.min(source.channels as usize - 1)] as f64
        }
    };
    let cutoff = (rate as f64 / source.rate as f64).min(1.0) * 0.94;
    for output in 0..frames as u64 {
        if output.is_multiple_of(1024) {
            if cancel.load(Ordering::Acquire) {
                return Err(CommandError::new(
                    ErrorCode::Cancelled,
                    "Audio conversion cancelled",
                ));
            }
            progress((output as u128 * 80 / frames.max(1)) as u8);
        }
        if rate == source.rate {
            for channel in 0..channels as usize {
                result.push(read(output as i64, channel) as f32);
            }
            continue;
        }
        let position = output as f64 * source.rate as f64 / rate as f64;
        let center = position.floor() as i64;
        let mut values = [0.0_f64; 2];
        let mut weight_sum = 0.0;
        for frame in center - 31..=center + 32 {
            let distance = position - frame as f64;
            if distance.abs() >= 32.0 {
                continue;
            }
            let x = std::f64::consts::PI * distance * cutoff;
            let sinc = if x.abs() < 1e-12 { 1.0 } else { x.sin() / x };
            let window = 0.42
                + 0.5 * (std::f64::consts::PI * distance / 32.0).cos()
                + 0.08 * (std::f64::consts::TAU * distance / 32.0).cos();
            let weight = sinc * window * cutoff;
            weight_sum += weight;
            for (channel, value) in values.iter_mut().enumerate().take(channels as usize) {
                *value += read(frame, channel) * weight;
            }
        }
        for value in values.into_iter().take(channels as usize) {
            let sample = value / weight_sum;
            if !sample.is_finite() || sample.abs() > f32::MAX as f64 {
                return Err(CommandError::new(
                    ErrorCode::InvalidParams,
                    "Converted audio exceeds finite floating-point range",
                ));
            }
            result.push(sample as f32);
        }
    }
    Ok(result)
}
