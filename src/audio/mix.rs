//! Prepared project-clock mixer. Resampling/segment construction happens off-thread;
//! reading a frame has no allocation, destruction, locks or filesystem access.
use super::*;
use crate::arrangement::{Arrangement, Clip};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub(crate) struct Input {
    pub clip: Clip,
    pub audio: Arc<Audio>,
    pub range: crate::documents::FrameRange,
}
#[derive(Clone, Debug)]
struct Buffer {
    samples: Arc<[f32]>,
    offset: usize,
    channels: usize,
}
#[derive(Debug)]
struct PreparedClip {
    start: u64,
    end: u64,
    key: String,
    buffer: Buffer,
    gains: [f64; 2],
}
#[derive(Debug)]
struct Segment {
    start: u64,
    end: u64,
    active: Box<[usize]>,
}
#[derive(Debug)]
pub(crate) struct Graph {
    pub revision_id: String,
    pub rate: u32,
    pub frames: u64,
    clips: Vec<PreparedClip>,
    segments: Vec<Segment>,
}
impl Graph {
    pub fn buffers(&self) -> impl Iterator<Item = &Arc<[f32]>> {
        self.clips.iter().map(|clip| &clip.buffer.samples)
    }
    pub fn frame(&self, frame: u64) -> [f32; 2] {
        let i = self
            .segments
            .partition_point(|segment| segment.end <= frame);
        let Some(segment) = self
            .segments
            .get(i)
            .filter(|segment| segment.start <= frame)
        else {
            return [0.0; 2];
        };
        let mut result = [0.0f64; 2];
        for index in &segment.active {
            let clip = &self.clips[*index];
            debug_assert!(frame < clip.end);
            let offset = (frame - clip.start) as usize * clip.buffer.channels + clip.buffer.offset;
            for (channel, value) in result.iter_mut().enumerate() {
                *value += clip.buffer.samples[offset + channel.min(clip.buffer.channels - 1)]
                    as f64
                    * clip.gains[channel];
            }
        }
        [result[0] as f32, result[1] as f32]
    }
}
pub(crate) fn prepare(
    arrangement: &Arrangement,
    inputs: Vec<Input>,
    previous: Option<Arc<Graph>>,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<Graph, CommandError> {
    let mut cache: BTreeMap<String, Buffer> = previous
        .iter()
        .flat_map(|graph| &graph.clips)
        .map(|clip| (clip.key.clone(), clip.buffer.clone()))
        .collect();
    let any_solo = arrangement.tracks.iter().any(|track| track.solo);
    let master = 10.0f64.powf(arrangement.master_gain_db / 20.0);
    let mut clips = Vec::new();
    let mut events = Vec::new();
    let mut bound = 0.0;
    for (index, input) in inputs.iter().enumerate() {
        if cancel.load(Ordering::Acquire) {
            return Err(CommandError::new(
                ErrorCode::Cancelled,
                "Mix preparation cancelled",
            ));
        }
        let (track, _) = arrangement.clip(&input.clip.id).unwrap();
        let key = format!(
            "{}:{}:{}:{}",
            input
                .clip
                .processed_revision_id
                .as_ref()
                .unwrap_or(&input.clip.revision_id),
            input.range.start,
            input.range.end,
            arrangement.sample_rate
        );
        let buffer = if let Some(buffer) = cache.get(&key) {
            buffer.clone()
        } else {
            let buffer = if input.audio.rate == arrangement.sample_rate {
                Buffer {
                    samples: input.audio.samples.clone(),
                    offset: input.range.start as usize * input.audio.channels as usize,
                    channels: input.audio.channels as usize,
                }
            } else {
                Buffer {
                    samples: resample::convert(
                        &input.audio,
                        &input.range,
                        arrangement.sample_rate,
                        input.audio.channels,
                        cancel,
                        |_| {},
                    )?
                    .into(),
                    offset: 0,
                    channels: input.audio.channels as usize,
                }
            };
            cache.insert(key.clone(), buffer.clone());
            buffer
        };
        let gain = master * 10.0f64.powf(track.gain_db / 20.0);
        let gains = if input.audio.channels == 1 {
            let angle = (track.pan + 1.0) * std::f64::consts::FRAC_PI_4;
            [gain * angle.cos(), gain * angle.sin()]
        } else if track.pan >= 0.0 {
            [gain * (track.pan * std::f64::consts::FRAC_PI_2).cos(), gain]
        } else {
            [
                gain,
                gain * (-track.pan * std::f64::consts::FRAC_PI_2).cos(),
            ]
        };
        let audible = !track.mute && (!any_solo || track.solo);
        if audible {
            events.push((input.clip.position, true, index));
            events.push((input.clip.position + input.clip.frames, false, index));
            bound += input.audio.peak_level * gain;
        }
        clips.push(PreparedClip {
            start: input.clip.position,
            end: input.clip.position + input.clip.frames,
            key,
            buffer,
            gains,
        });
        progress(((index + 1) * 80 / inputs.len().max(1)) as u8);
    }
    if !bound.is_finite() || bound > f32::MAX as f64 {
        return Err(CommandError::new(
            ErrorCode::InvalidParams,
            "Mix can exceed finite float32 range; lower gains",
        ));
    }
    events.sort();
    let mut segments = Vec::new();
    let mut active = BTreeSet::new();
    let mut previous_frame = 0;
    for (frame, start, index) in events {
        if frame > previous_frame {
            segments.push(Segment {
                start: previous_frame,
                end: frame,
                active: active
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            });
            previous_frame = frame;
        }
        if start {
            active.insert(index);
        } else {
            active.remove(&index);
        }
    }
    progress(100);
    Ok(Graph {
        revision_id: arrangement.revision_id.clone(),
        rate: arrangement.sample_rate,
        frames: arrangement.frames(),
        clips,
        segments,
    })
}
