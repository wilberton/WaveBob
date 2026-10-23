//! MP3 decoding plus a bounded structural pass: truncated frames must not become success.
use super::*;
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
};
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

struct StreamInfo {
    rate: u32,
    channels: u16,
    frames: u64,
    mode: String,
    bitrate: Option<u32>,
}
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

fn scan(
    path: &Path,
    cancel: &AtomicBool,
    progress: &impl Fn(u8),
) -> Result<StreamInfo, CommandError> {
    let file = File::open(path).map_err(io)?;
    let size = file.metadata().map_err(io)?.len();
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut position = 0;
    let mut prefix = [0; 10];
    reader.read_exact(&mut prefix).map_err(io)?;
    if &prefix[..3] == b"ID3" {
        if !(2..=4).contains(&prefix[3]) || prefix[6..].iter().any(|byte| byte & 0x80 != 0) {
            return Err(invalid("Invalid ID3 header"));
        }
        let length = prefix[6..]
            .iter()
            .fold(0_u64, |length, byte| length * 128 + *byte as u64);
        position = 10
            + length
            + if prefix[3] == 4 && prefix[5] & 0x10 != 0 {
                10
            } else {
                0
            };
        if position >= size {
            return Err(invalid("Truncated ID3 tag or empty MP3"));
        }
    }
    reader.seek(SeekFrom::Start(position)).map_err(io)?;
    let mut frames = 0;
    let mut first_format = None;
    let mut first_bitrate = 0;
    let mut variable = false;
    let mut metadata_frame = false;
    let mut declared_frames = None;
    let mut body = [0; 2048];
    while position < size {
        cancelled(cancel)?;
        let mut header = [0; 4];
        reader.read_exact(&mut header).map_err(io)?;
        if &header[..3] == b"TAG" && size - position == 128 {
            break;
        }
        let word = u32::from_be_bytes(header);
        let version = (word >> 19) & 3;
        let bitrate_index = ((word >> 12) & 15) as usize;
        let rate_index = ((word >> 10) & 3) as usize;
        if word & 0xffe00000 != 0xffe00000
            || version == 1
            || (word >> 17) & 3 != 1
            || bitrate_index == 15
            || rate_index == 3
        {
            return Err(invalid("Invalid MPEG Layer III frame header"));
        }
        if bitrate_index == 0 {
            return Err(CommandError::new(
                ErrorCode::UnsupportedFormat,
                "Free-format MP3 is not supported",
            ));
        }
        let rate = [44100, 48000, 32000][rate_index]
            / if version == 3 {
                1
            } else if version == 2 {
                2
            } else {
                4
            };
        let channels = if (word >> 6) & 3 == 3 { 1 } else { 2 };
        let samples = if version == 3 { 1152_u64 } else { 576 };
        let bitrate = if version == 3 {
            [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
            ]
        } else {
            [
                0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
            ]
        }[bitrate_index];
        let length =
            (if version == 3 { 144 } else { 72 }) * bitrate * 1000 / rate + ((word >> 9) & 1);
        if length < 4 || length as usize - 4 > body.len() || position + length as u64 > size {
            return Err(invalid("Truncated MPEG audio frame"));
        }
        let format = (rate, channels, samples);
        if first_format.is_some_and(|first| first != format) {
            return Err(CommandError::new(
                ErrorCode::UnsupportedFormat,
                "MP3 rate/channel changes inside one stream are not supported",
            ));
        }
        first_format = Some(format);
        reader
            .read_exact(&mut body[..length as usize - 4])
            .map_err(io)?;
        if frames == 0 {
            first_bitrate = bitrate;
            let side = if version == 3 {
                if channels == 1 { 17 } else { 32 }
            } else if channels == 1 {
                9
            } else {
                17
            };
            let offset = side + if word & (1 << 16) == 0 { 2 } else { 0 };
            if offset + 12 <= length as usize - 4 {
                let tag = &body[offset..offset + 4];
                if tag == b"Xing" || tag == b"Info" {
                    metadata_frame = true;
                    variable = tag == b"Xing";
                    let flags =
                        u32::from_be_bytes(body[offset + 4..offset + 8].try_into().unwrap());
                    if flags & 1 != 0 {
                        declared_frames = Some(u32::from_be_bytes(
                            body[offset + 8..offset + 12].try_into().unwrap(),
                        ) as u64);
                    }
                }
            }
            if length as usize >= 62 && &body[32..36] == b"VBRI" && body[36..38] == [0, 1] {
                metadata_frame = true;
                variable = true;
                declared_frames = Some(u32::from_be_bytes(body[46..50].try_into().unwrap()) as u64);
            }
        } else if !metadata_frame || frames > 1 {
            variable |= bitrate != first_bitrate;
        } else if !variable {
            first_bitrate = bitrate;
        }
        frames += 1;
        position += length as u64;
        progress((position * 15 / size.max(1)) as u8);
    }
    let (rate, channels, samples) = first_format.ok_or_else(|| invalid("No MP3 frames"))?;
    let audio_frames = frames - u64::from(metadata_frame);
    if declared_frames.is_some_and(|declared| declared != audio_frames) {
        return Err(invalid(
            "MP3 frame count does not match its Xing/Info header",
        ));
    }
    if audio_frames == 0 {
        return Err(invalid("No MP3 audio frames"));
    }
    Ok(StreamInfo {
        rate,
        channels,
        frames: audio_frames * samples,
        mode: if variable { "vbr" } else { "cbr" }.into(),
        bitrate: if variable { None } else { Some(first_bitrate) },
    })
}

pub(super) fn decode(
    path: &Path,
    cancel: &AtomicBool,
    progress: &impl Fn(u8),
) -> Result<Audio, CommandError> {
    let stream = scan(path, cancel, progress)?;
    let error = |error: symphonia::core::errors::Error| invalid(error.to_string());
    let source =
        MediaSourceStream::new(Box::new(File::open(path).map_err(io)?), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
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
        .ok_or_else(|| invalid("No audio track"))?;
    let parameters = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| invalid("No audio codec parameters"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default().verify(true))
        .map_err(error)?;
    let id = track.id;
    let details = SourceDetails {
        bitrate_mode: Some(stream.mode),
        bitrate_kbps: stream.bitrate,
        encoder_delay: track.delay,
        encoder_padding: track.padding,
    };
    let expected = stream
        .frames
        .saturating_sub(track.delay.unwrap_or(0) as u64 + track.padding.unwrap_or(0) as u64);
    let values = expected
        .checked_mul(stream.channels as u64)
        .filter(|count| *count <= MAX_VALUES as u64)
        .ok_or_else(|| {
            CommandError::new(
                ErrorCode::ResourceLimit,
                "Decoded audio exceeds the current 512 MiB import limit",
            )
        })? as usize;
    let mut samples = Vec::<f32>::new();
    samples
        .try_reserve_exact(values)
        .map_err(|error| CommandError::new(ErrorCode::ResourceLimit, error.to_string()))?;
    while let Some(packet) = format.next_packet().map_err(error)? {
        cancelled(cancel)?;
        if packet.track_id != id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(error)?;
        if decoded.spec().rate() != stream.rate
            || decoded.spec().channels().count() != stream.channels as usize
        {
            return Err(invalid("Decoded format changed"));
        }
        let first = samples.len();
        let count = decoded.samples_interleaved();
        if first + count > values {
            return Err(invalid(
                "Decoded MP3 has more frames than its validated stream",
            ));
        }
        samples.resize(first + count, 0.0);
        decoded.copy_to_slice_interleaved(&mut samples[first..]);
        if samples[first..].iter().any(|value| !value.is_finite()) {
            return Err(invalid("Decoded MP3 contains non-finite samples"));
        }
        progress(15 + (samples.len() as u64 * 75 / values.max(1) as u64) as u8);
    }
    if samples.len() != values || samples.is_empty() {
        return Err(invalid(
            "Decoded MP3 length does not match its validated stream",
        ));
    }
    prepare(
        stream.rate,
        stream.channels,
        "mp3".into(),
        details,
        samples,
        cancel,
        progress,
    )
}
