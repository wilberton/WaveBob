//! Ogg Vorbis import with strict page validation before decoding.
use super::*;
use std::{
    fs::File,
    io::{BufReader, Read},
};
use symphonia::core::{
    checksum::Crc32,
    codecs::audio::{AudioDecoderOptions, well_known::CODEC_ID_VORBIS},
    formats::{FormatOptions, TrackType, probe::Hint},
    io::{MediaSourceStream, Monitor},
    meta::MetadataOptions,
};

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::new(ErrorCode::DecodeFailed, message)
}
fn io(error: std::io::Error) -> CommandError {
    CommandError::new(
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            ErrorCode::DecodeFailed
        } else {
            ErrorCode::IoError
        },
        error.to_string(),
    )
}
fn cancelled(cancel: &AtomicBool) -> Result<(), CommandError> {
    if cancel.load(Ordering::Acquire) {
        Err(CommandError::new(ErrorCode::Cancelled, "Import cancelled"))
    } else {
        Ok(())
    }
}

// Demuxers may recover from bad pages or treat incomplete pages as EOF. Imports
// must instead fail atomically. Keep this pass bounded to one maximum-size page.
fn validate(path: &Path, cancel: &AtomicBool, progress: &impl Fn(u8)) -> Result<(), CommandError> {
    let file = File::open(path).map_err(io)?;
    let size = file.metadata().map_err(io)?.len();
    let mut reader = BufReader::new(file);
    let mut position = 0;
    let mut serial = None;
    let mut sequence = 0_u32;
    let mut ended = false;
    let mut continuation = false;
    let mut body = vec![0; 255 * 255];
    while position < size {
        cancelled(cancel)?;
        if ended {
            return Err(CommandError::new(
                ErrorCode::UnsupportedFormat,
                "Chained Ogg streams are not supported",
            ));
        }
        let mut header = [0; 27];
        reader.read_exact(&mut header).map_err(io)?;
        if &header[..4] != b"OggS" || header[4] != 0 || header[5] & !7 != 0 {
            return Err(invalid("Invalid Ogg page header"));
        }
        let page_serial = u32::from_le_bytes(header[14..18].try_into().unwrap());
        if serial.is_some_and(|serial| serial != page_serial) {
            return Err(CommandError::new(
                ErrorCode::UnsupportedFormat,
                "Multiplexed Ogg streams are not supported",
            ));
        }
        let first = serial.is_none();
        if (header[5] & 2 != 0) != first
            || (header[5] & 1 != 0) != continuation
            || u32::from_le_bytes(header[18..22].try_into().unwrap()) != sequence
        {
            return Err(invalid("Missing or out-of-order Ogg page"));
        }
        serial = Some(page_serial);
        sequence = sequence.wrapping_add(1);
        let mut segments = [0; 255];
        let segments = &mut segments[..header[26] as usize];
        reader.read_exact(segments).map_err(io)?;
        let length: usize = segments.iter().map(|n| *n as usize).sum();
        reader.read_exact(&mut body[..length]).map_err(io)?;
        let expected = u32::from_le_bytes(header[22..26].try_into().unwrap());
        header[22..26].fill(0);
        let mut crc = Crc32::new(0);
        crc.process_buf_bytes(&header);
        crc.process_buf_bytes(segments);
        crc.process_buf_bytes(&body[..length]);
        if crc.crc() != expected {
            return Err(invalid("Ogg page checksum mismatch"));
        }
        if first && !body[..length].starts_with(b"\x01vorbis") {
            return Err(CommandError::new(
                ErrorCode::UnsupportedFormat,
                "Supported Ogg audio is mono/stereo Vorbis",
            ));
        }
        continuation = segments.last().is_some_and(|n| *n == 255);
        ended = header[5] & 4 != 0;
        if ended && continuation {
            return Err(invalid("Incomplete final Ogg packet"));
        }
        position += (27 + segments.len() + length) as u64;
        progress((position * 15 / size.max(1)) as u8);
    }
    if !ended {
        return Err(invalid("Ogg stream is empty or missing its final page"));
    }
    Ok(())
}

pub(super) fn decode(
    path: &Path,
    cancel: &AtomicBool,
    progress: &impl Fn(u8),
) -> Result<Audio, CommandError> {
    validate(path, cancel, progress)?;
    cancelled(cancel)?;
    let error = |error: symphonia::core::errors::Error| invalid(error.to_string());
    let source =
        MediaSourceStream::new(Box::new(File::open(path).map_err(io)?), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(error)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| invalid("No Ogg audio track"))?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| invalid("No Ogg audio codec parameters"))?;
    let rate = params.sample_rate.unwrap_or(0);
    let channels = params.channels.as_ref().map_or(0, |c| c.count()) as u16;
    if params.codec != CODEC_ID_VORBIS || rate == 0 || ![1, 2].contains(&channels) {
        return Err(CommandError::new(
            ErrorCode::UnsupportedFormat,
            "Supported Ogg audio is mono/stereo Vorbis",
        ));
    }
    let id = track.id;
    let expected = track.num_frames;
    if expected.is_some_and(|frames| frames > MAX_VALUES as u64 / channels as u64) {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Decoded audio exceeds the current 512 MiB import limit",
        ));
    }
    let details = SourceDetails {
        encoder_delay: track.delay,
        encoder_padding: track.padding,
        ..SourceDetails::default()
    };
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().verify(true))
        .map_err(error)?;
    let mut samples = Vec::<f32>::new();
    while let Some(packet) = format.next_packet().map_err(error)? {
        cancelled(cancel)?;
        if packet.track_id != id {
            return Err(invalid("Ogg audio track changed"));
        }
        let decoded = decoder.decode(&packet).map_err(error)?;
        if decoded.spec().rate() != rate || decoded.spec().channels().count() != channels as usize {
            return Err(invalid("Decoded Ogg format changed"));
        }
        let count = decoded.samples_interleaved();
        if count > MAX_VALUES - samples.len() {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Decoded audio exceeds the current 512 MiB import limit",
            ));
        }
        samples
            .try_reserve(count)
            .map_err(|e| CommandError::new(ErrorCode::ResourceLimit, e.to_string()))?;
        let first = samples.len();
        samples.resize(first + count, 0.0);
        decoded.copy_to_slice_interleaved(&mut samples[first..]);
        if samples[first..].iter().any(|value| !value.is_finite()) {
            return Err(invalid("Decoded Ogg contains non-finite samples"));
        }
        if let Some(frames) = expected {
            progress(
                15 + (samples.len() as u64 * 75 / frames.saturating_mul(channels as u64).max(1))
                    .min(75) as u8,
            );
        }
    }
    if samples.is_empty() || !samples.len().is_multiple_of(channels as usize) {
        return Err(invalid("Ogg contains no complete audio frames"));
    }
    if expected.is_some_and(|frames| samples.len() as u64 / channels as u64 != frames) {
        return Err(invalid("Decoded Ogg duration does not match its stream"));
    }
    prepare(
        rate,
        channels,
        "ogg_vorbis".into(),
        details,
        samples,
        cancel,
        progress,
    )
}
