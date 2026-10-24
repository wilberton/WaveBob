//! Safe, bounded facade over the statically linked LAME 3.100 encoder.
use super::*;
use mp3lame_sys as ffi;
use std::ptr::NonNull;
struct Encoder(NonNull<ffi::lame_global_flags>);
impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            ffi::lame_close(self.0.as_ptr());
        }
    }
}
pub(super) fn encode(
    file: &mut std::fs::File,
    samples: &[f32],
    rate: u32,
    channels: u16,
    bitrate: u16,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<(u32, u32), CommandError> {
    // The encoder owns this handle until Drop. All FFI buffers are initialized,
    // bounded slices, with frame/channel counts checked by output preparation.
    let encoder = Encoder(
        NonNull::new(unsafe { ffi::lame_init() })
            .ok_or_else(|| encode_error("Allocate LAME encoder".into()))?,
    );
    let pointer = encoder.0.as_ptr();
    let frames = samples.len() / channels as usize;
    let settings = unsafe {
        ffi::lame_set_errorf(pointer, None);
        ffi::lame_set_debugf(pointer, None);
        ffi::lame_set_msgf(pointer, None);
        [
            ffi::lame_set_num_channels(pointer, channels as i32),
            ffi::lame_set_in_samplerate(pointer, rate as i32),
            ffi::lame_set_out_samplerate(pointer, rate as i32),
            ffi::lame_set_brate(pointer, bitrate as i32),
            ffi::lame_set_VBR(pointer, ffi::vbr_mode::vbr_off),
            ffi::lame_set_mode(
                pointer,
                if channels == 1 {
                    ffi::MPEG_mode::MONO
                } else {
                    ffi::MPEG_mode::JOINT_STEREO
                },
            ),
            ffi::lame_set_quality(pointer, 2),
            ffi::lame_set_bWriteVbrTag(pointer, 1),
            ffi::lame_set_num_samples(pointer, frames as _),
        ]
    };
    if settings.iter().any(|code| *code < 0) || unsafe { ffi::lame_init_params(pointer) } < 0 {
        return Err(encode_error(
            "Initialize LAME with the requested format".into(),
        ));
    }
    if unsafe { ffi::lame_get_out_samplerate(pointer) } != rate as i32 {
        return Err(encode_error(
            "LAME changed the requested output sample rate".into(),
        ));
    }
    let mut buffer = vec![0_u8; 16384];
    for (index, chunk) in samples.chunks(4096 * channels as usize).enumerate() {
        check_cancel(cancel)?;
        let count = chunk.len() / channels as usize;
        let written = unsafe {
            if channels == 1 {
                ffi::lame_encode_buffer_ieee_float(
                    pointer,
                    chunk.as_ptr(),
                    std::ptr::null(),
                    count as i32,
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                )
            } else {
                ffi::lame_encode_buffer_interleaved_ieee_float(
                    pointer,
                    chunk.as_ptr(),
                    count as i32,
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                )
            }
        };
        write_encoded(file, &buffer, written)?;
        progress((index * 4096 * 100 / frames.max(1)).min(100) as u8);
    }
    let written =
        unsafe { ffi::lame_encode_flush(pointer, buffer.as_mut_ptr(), buffer.len() as i32) };
    write_encoded(file, &buffer, written)?;
    let size = unsafe { ffi::lame_get_lametag_frame(pointer, std::ptr::null_mut(), 0) };
    if size == 0 || size > buffer.len() {
        return Err(encode_error(
            "LAME did not provide bounded gapless metadata".into(),
        ));
    }
    let written =
        unsafe { ffi::lame_get_lametag_frame(pointer, buffer.as_mut_ptr(), buffer.len()) };
    if written != size {
        return Err(encode_error("LAME gapless metadata size changed".into()));
    }
    // LAME reserves a dummy first MPEG frame. Replace it after normal flush,
    // rather than inserting an extra frame or using the no-gap streaming flush.
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    file.write_all(&buffer[..size]).map_err(io_error)?;
    let (delay, padding) = unsafe {
        (
            ffi::lame_get_encoder_delay(pointer),
            ffi::lame_get_encoder_padding(pointer),
        )
    };
    if delay < 0 || padding < 0 {
        return Err(encode_error(
            "LAME returned invalid delay/padding metadata".into(),
        ));
    }
    Ok((delay as u32, padding as u32))
}
fn write_encoded(
    file: &mut std::fs::File,
    buffer: &[u8],
    written: i32,
) -> Result<(), CommandError> {
    if written < 0 || written as usize > buffer.len() {
        return Err(encode_error(format!("LAME encoding failed ({written})")));
    }
    file.write_all(&buffer[..written as usize])
        .map_err(io_error)
}
