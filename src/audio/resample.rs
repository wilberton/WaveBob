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
    convert_from(
        input,
        source.rate,
        rate,
        channels,
        cancel,
        progress,
        |frame| {
            let index = (range.start + frame) as usize * source.channels as usize;
            let left = source.samples[index] as f64;
            let right = source.samples[index + usize::from(source.channels == 2)] as f64;
            if channels == 1 && source.channels == 2 {
                [(left + right) * 0.5; 2]
            } else {
                [left, right]
            }
        },
    )
}
pub(crate) fn convert_from(
    input: u64,
    source_rate: u32,
    rate: u32,
    channels: u16,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
    read_frame: impl Fn(u64) -> [f64; 2],
) -> Result<Vec<f32>, CommandError> {
    if source_rate == 0 || rate == 0 || !(1..=2).contains(&channels) || input > i64::MAX as u64 {
        return Err(CommandError::new(
            ErrorCode::InvalidParams,
            "Invalid audio conversion format or duration",
        ));
    }
    let frames = frame_count(input, source_rate, rate) as u128;
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
    let cutoff = (rate as f64 / source_rate as f64).min(1.0) * 0.94;
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
        if rate == source_rate {
            for value in read_frame(output).into_iter().take(channels as usize) {
                result.push(value as f32);
            }
            continue;
        }
        let position = output as f64 * source_rate as f64 / rate as f64;
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
            let samples = read_frame(frame.clamp(0, input.saturating_sub(1) as i64) as u64);
            for (channel, value) in values.iter_mut().enumerate().take(channels as usize) {
                *value += samples[channel] * weight;
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
