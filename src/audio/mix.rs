//! Prepared project-clock mixer. Resampling/segment construction happens off-thread;
//! reading a frame has no allocation, destruction, locks or filesystem access.
use super::*;
use crate::arrangement::{Arrangement, Clip};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub(crate) struct Input {
    pub document: Option<crate::bounces::DocumentInput>,
    pub clip: Clip,
    pub audio: Arc<Audio>,
    pub range: crate::documents::FrameRange,
}
#[derive(Clone, Debug)]
struct Buffer {
    rate: u32,
    samples: Arc<[f32]>,
    offset: usize,
    channels: usize,
}
#[derive(Debug)]
struct PreparedClip {
    owner_id: String,
    start: u64,
    end: u64,
    body_end: u64,
    tail_capped: bool,
    key: String,
    buffer: Buffer,
    gains: [f64; 2],
    envelopes: Vec<Envelope>,
    stream: Option<StreamingInput>,
}
#[derive(Debug)]
struct StreamingInput {
    audio: Arc<Audio>,
    range: crate::documents::FrameRange,
    state: crate::processing::Processing,
    tempo: f64,
    body: u64,
}
#[derive(Debug)]
struct ClipVoice {
    voice: crate::rendering::Voice,
    position: u64,
    output: [[f64; 2]; 2],
}
/// Mutable playback state is prepared off the callback; the graph remains immutable.
#[derive(Debug)]
pub(crate) struct Stream {
    voices: Vec<Option<ClipVoice>>,
    pub values: [[f32; 2]; 2],
    pub position: u64,
    bounds: Option<crate::documents::FrameRange>,
    pub failed: bool,
    planned_tail_capped: bool,
}
impl Stream {
    pub(crate) fn visit_meters(
        &self,
        visitor: &mut impl FnMut(u128, bool, crate::metering::Block),
    ) {
        for voice in self.voices.iter().flatten() {
            voice.voice.chain.visit_meters(visitor);
        }
    }
    #[cfg(test)]
    pub(crate) fn with_test_chains(graph: &Graph, chains: Vec<crate::rendering::Chain>) -> Self {
        let voices = graph
            .clips
            .iter()
            .zip(chains)
            .map(|(clip, chain)| {
                let input = clip.stream.as_ref().unwrap();
                Some(ClipVoice {
                    voice: crate::rendering::Voice::with_chain(
                        chain,
                        &input.audio,
                        0,
                        None,
                        || false,
                    )
                    .unwrap(),
                    position: 0,
                    output: [[0.0; 2]; 2],
                })
            })
            .collect();
        let mut stream = Self {
            voices,
            values: [[0.0; 2]; 2],
            position: 0,
            bounds: None,
            failed: false,
            planned_tail_capped: graph
                .clips
                .iter()
                .any(|c| c.gains != [0.0; 2] && c.tail_capped),
        };
        stream.values[0] = stream.read(graph);
        stream.step_position();
        stream.values[1] = stream.read(graph);
        stream
    }
    pub fn prepare(
        graph: &Graph,
        position: u64,
        bounds: Option<crate::documents::FrameRange>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, CommandError> {
        let mut voices = Vec::with_capacity(graph.clips.len());
        for clip in &graph.clips {
            let voice = if let Some(input) = &clip.stream {
                let local = position
                    .saturating_sub(clip.start)
                    .min(clip.end - clip.start);
                let source = local * input.audio.rate as u64 / graph.rate as u64;
                Some(ClipVoice {
                    voice: crate::rendering::Voice::prepare_range_body(
                        &input.state,
                        &input.audio,
                        &input.range,
                        input.tempo,
                        source,
                        input.body,
                        &cancelled,
                    )?,
                    position: source,
                    output: [[0.0; 2]; 2],
                })
            } else {
                None
            };
            voices.push(voice);
        }
        let mut stream = Self {
            voices,
            values: [[0.0; 2]; 2],
            position,
            bounds,
            failed: false,
            planned_tail_capped: graph
                .clips
                .iter()
                .any(|c| c.gains != [0.0; 2] && c.tail_capped),
        };
        stream.values[0] = stream.read(graph);
        stream.step_position();
        stream.values[1] = stream.read(graph);
        Ok(stream)
    }
    fn step_position(&mut self) {
        self.position = self.position.saturating_add(1);
        if let Some(bounds) = &self.bounds
            && self.position >= bounds.end
        {
            self.position = bounds.start;
        }
    }
    pub fn advance(&mut self, graph: &Graph, frames: u64) {
        for _ in 0..frames {
            self.values[0] = self.values[1];
            self.step_position();
            self.values[1] = self.read(graph);
        }
    }
    fn read(&mut self, graph: &Graph) -> [f32; 2] {
        let frame = self.position;
        let mut result = graph.frame(frame).map(f64::from);
        for (clip, state) in graph.clips.iter().zip(&mut self.voices) {
            let (Some(input), Some(state)) = (&clip.stream, state) else {
                continue;
            };
            state.output[0] = state.output[1];
            state.output[1] = [0.0; 2];
            if frame < clip.start || frame >= clip.end {
                continue;
            }
            let numerator = (frame - clip.start) * input.audio.rate as u64;
            let target = numerator / graph.rate as u64;
            if target < state.position {
                state.voice.wrap(&input.audio, target);
            } else {
                state.voice.advance(&input.audio, target - state.position);
            }
            state.position = target;
            let fraction = (numerator % graph.rate as u64) as f64 / graph.rate as f64;
            let mut fade = clip.crossfade(frame);
            if clip.tail_capped {
                let taper = (graph.rate as u64 / 100).max(1);
                if frame >= clip.end.saturating_sub(taper) {
                    fade *= clip.end.saturating_sub(frame + 1) as f64 / taper as f64;
                }
            }

            for (channel, value) in result.iter_mut().enumerate() {
                let pair = state.voice.values;
                let sample = pair[0][channel] as f64
                    + (pair[1][channel] as f64 - pair[0][channel] as f64) * fraction;
                state.output[1][channel] = sample * fade;
                *value += state.output[1][channel] * clip.gains[channel];
            }
        }
        self.failed |= result
            .iter()
            .any(|x| !x.is_finite() || x.abs() > f32::MAX as f64)
            || self.voices.iter().flatten().any(|v| v.voice.chain.failed);
        if self.failed {
            [0.0; 2]
        } else {
            result.map(|x| x as f32)
        }
    }
    pub fn latency(&self, graph: &Graph) -> u64 {
        graph
            .clips
            .iter()
            .zip(&self.voices)
            .filter_map(|(clip, voice)| {
                Some(
                    (voice.as_ref()?.voice.chain.latency() as u64 * graph.rate as u64)
                        .div_ceil(clip.stream.as_ref()?.audio.rate as u64),
                )
            })
            .max()
            .unwrap_or(0)
    }
    pub fn tail_capped(&self) -> bool {
        self.planned_tail_capped
            || self
                .voices
                .iter()
                .flatten()
                .any(|v| v.voice.chain.tail_capped)
    }
    pub fn retarget(&mut self, previous: &mut Self, graph: &Graph, old: &Graph) -> bool {
        if self.bounds != previous.bounds || graph.clips.len() != old.clips.len() {
            return false;
        }
        for (a, b) in graph.clips.iter().zip(&old.clips) {
            if a.owner_id != b.owner_id
                || a.start != b.start
                || a.body_end != b.body_end
                || a.key != b.key
                || a.stream.is_some() != b.stream.is_some()
            {
                return false;
            }
        }
        for (next, prior) in self.voices.iter().zip(&previous.voices) {
            if let (Some(next), Some(prior)) = (next, prior)
                && !next.voice.chain.can_retarget(&prior.voice.chain)
            {
                return false;
            }
        }
        for (next, prior) in self.voices.iter_mut().zip(&mut previous.voices) {
            if let (Some(next), Some(prior)) = (next, prior) {
                next.voice.retarget(&mut prior.voice);
                next.position = prior.position;
                next.output = prior.output;
            }
        }
        // Reevaluate cached frames with the new gains on subsequent advances. Chains
        // retain their own continuous parameter ramps and exact source cursor.
        self.position = previous.position;
        self.values = previous.values;
        // Track and master gain are outside the owner processor. Rebuild the two
        // cached mix samples with the new gains without advancing any processor.
        if graph
            .clips
            .iter()
            .zip(&old.clips)
            .any(|(a, b)| a.gains != b.gains)
        {
            for sample in 0..2 {
                let frame = if sample == 1 {
                    self.position
                } else if self
                    .bounds
                    .as_ref()
                    .is_some_and(|b| self.position == b.start)
                {
                    self.bounds.as_ref().unwrap().end - 1
                } else {
                    self.position.saturating_sub(1)
                };
                let mut sum = graph.frame(frame).map(f64::from);
                for (clip, voice) in graph.clips.iter().zip(&self.voices) {
                    if let Some(voice) = voice {
                        for (ch, value) in sum.iter_mut().enumerate() {
                            *value += voice.output[sample][ch] * clip.gains[ch];
                        }
                    }
                }
                self.values[sample] = sum.map(|v| v as f32);
            }
        }
        true
    }
}
impl PreparedClip {
    fn crossfade(&self, frame: u64) -> f64 {
        let frame = frame.min(self.body_end.saturating_sub(1));
        let i = self.envelopes.partition_point(|e| e.region.end <= frame);
        self.envelopes
            .get(i)
            .map_or(1.0, |e| e.region.gains(frame)[usize::from(!e.first)])
    }
}
#[derive(Debug)]
struct Envelope {
    region: crate::arrangement::CrossfadeRegion,
    first: bool,
}
#[derive(Debug)]
struct Segment {
    start: u64,
    end: u64,
    active: Box<[usize]>,
}
#[derive(Debug)]
pub(crate) struct Graph {
    pub bounces: crate::bounces::Cache,
    pub refreshed_clips: Vec<Clip>,
    tail_cache: BTreeMap<String, (u64, bool, u16)>,
    pub revision_id: String,
    pub arrangement_id: String,
    pub rate: u32,
    pub frames: u64,
    clips: Vec<PreparedClip>,
    segments: Vec<Segment>,
}
impl Graph {
    pub fn gain_change_only(&self, other: &Self) -> bool {
        self.clips.len() == other.clips.len()
            && self.clips.iter().zip(&other.clips).all(|(a, b)| {
                let first = a.stream.as_ref().map(|s| &s.state);
                let second = b.stream.as_ref().map(|s| &s.state);
                a.owner_id == b.owner_id
                    && a.key == b.key
                    && a.start == b.start
                    && a.body_end == b.body_end
                    && first.map_or(&[][..], |s| s.effects.as_slice())
                        == second.map_or(&[][..], |s| s.effects.as_slice())
                    && first.is_some_and(|s| s.bypassed) == second.is_some_and(|s| s.bypassed)
                    && first.map_or(0, |s| s.envelope.fade_in_frames)
                        == second.map_or(0, |s| s.envelope.fade_in_frames)
                    && first.map_or(0, |s| s.envelope.fade_out_frames)
                        == second.map_or(0, |s| s.envelope.fade_out_frames)
            })
    }
    pub fn expensive(&self) -> bool {
        self.clips
            .iter()
            .filter_map(|c| c.stream.as_ref())
            .any(|s| !s.state.effects.is_empty())
    }
    pub fn streaming(&self) -> bool {
        self.clips.iter().any(|c| c.stream.is_some())
    }
    pub fn buffers(&self) -> impl Iterator<Item = &Arc<[f32]>> {
        self.clips
            .iter()
            .map(|clip| &clip.buffer.samples)
            .chain(self.bounces.values().map(|a| &a.audio.samples))
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
            if clip.stream.is_some() {
                continue;
            }
            debug_assert!(frame < clip.end);
            let offset = (frame - clip.start) as usize * clip.buffer.channels + clip.buffer.offset;
            let mut envelope = clip.crossfade(frame);
            if clip.tail_capped {
                let taper = (self.rate as u64 / 100).max(1);
                if frame >= clip.end.saturating_sub(taper) {
                    envelope *= clip.end.saturating_sub(frame + 1) as f64 / taper as f64;
                }
            }
            for (channel, value) in result.iter_mut().enumerate() {
                *value += clip.buffer.samples[offset + channel.min(clip.buffer.channels - 1)]
                    as f64
                    * clip.gains[channel]
                    * envelope;
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
    if inputs.len() > 128 {
        return Err(crate::rendering::limit(
            "Arrangements support at most 128 prepared clip voices",
        ));
    }
    let mut bounce_cache = previous
        .as_ref()
        .map_or_else(Default::default, |g| g.bounces.clone());
    for input in &inputs {
        if let Some(reference) = &input.clip.bounce {
            bounce_cache
                .entry(reference.render_identity.clone())
                .or_insert_with(|| {
                    Arc::new(crate::bounces::Asset {
                        reference: reference.clone(),
                        audio: input.audio.clone(),
                    })
                });
        }
    }
    let (arrangement, mut inputs, bounces) =
        crate::bounces::prepare_inputs(arrangement, inputs, bounce_cache, cancel)?;
    let arrangement = &arrangement;
    let mut tail_cache = previous
        .as_ref()
        .map_or_else(BTreeMap::new, |g| g.tail_cache.clone());
    let mut cache: BTreeMap<String, Buffer> = previous
        .iter()
        .flat_map(|graph| &graph.clips)
        .map(|clip| (clip.key.clone(), clip.buffer.clone()))
        .collect();
    let any_solo = arrangement.tracks.iter().any(|track| track.solo);
    let mut envelopes: BTreeMap<String, Vec<Envelope>> = BTreeMap::new();
    for region in arrangement.crossfades().regions {
        envelopes
            .entry(region.first_clip_id.clone())
            .or_default()
            .push(Envelope {
                region: region.clone(),
                first: true,
            });
        envelopes
            .entry(region.second_clip_id.clone())
            .or_default()
            .push(Envelope {
                region,
                first: false,
            });
    }
    for list in envelopes.values_mut() {
        list.sort_by_key(|envelope| envelope.region.start);
    }
    let master = 10.0f64.powf(arrangement.master_gain_db / 20.0);
    let mut clips = Vec::new();
    let mut events = Vec::new();
    let mut bound = 0.0;
    let input_count = inputs.len();
    let mut tail_events = Vec::new();
    for (index, input) in inputs.iter_mut().enumerate() {
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
                .bounce
                .as_ref()
                .map(|b| &b.audio_revision_id)
                .or(input.clip.processed_revision_id.as_ref())
                .unwrap_or(&input.clip.revision_id),
            input.range.start,
            input.range.end,
            arrangement.sample_rate
        );
        let body = input.clip.source_end - input.clip.source_start;
        let boundary = input.clip.position + input.clip.frames - 1;
        let tail_audible = envelopes
            .get(&input.clip.id)
            .and_then(|list| list.get(list.partition_point(|e| e.region.end <= boundary)))
            .is_none_or(|e| e.region.gains(boundary)[usize::from(!e.first)] != 0.0);
        let extent_key = crate::projects::checksum(&(
            &key,
            input
                .clip
                .processing
                .identity(&key, arrangement.effect_tempo),
            body,
            tail_audible,
        ));
        let (extent, capped, output_channels) = if let Some(&extent) = tail_cache.get(&extent_key) {
            extent
        } else {
            let extent = crate::rendering::clip_extent(
                &input.audio,
                &input.range,
                body,
                &input.clip.processing,
                arrangement.effect_tempo,
                cancel,
                tail_audible,
            )?;
            if tail_cache.len() >= 512 {
                let oldest = tail_cache.keys().next().unwrap().clone();
                tail_cache.remove(&oldest);
            }
            tail_cache.insert(extent_key, extent);
            extent
        };
        let output_frames =
            resample::frame_count(extent, input.audio.rate, arrangement.sample_rate);
        input.clip.tail_frames = output_frames.saturating_sub(input.clip.frames);
        input.clip.tail_capped = tail_audible
            && (capped
                || input
                    .clip
                    .bounce
                    .as_ref()
                    .is_some_and(|b| b.metadata.tail_capped));
        input.clip.channels = output_channels;
        let end = input
            .clip
            .position
            .checked_add(output_frames)
            .filter(|end| *end <= crate::arrangement::MAX_TIMELINE_FRAME)
            .ok_or_else(|| {
                crate::rendering::limit("Clip body and tail exceed the timeline limit")
            })?;
        let streaming = input.audio.rate == arrangement.sample_rate
            || !input
                .clip
                .processing
                .same_sound(&crate::processing::Processing::default());
        let buffer = if let Some(buffer) = cache
            .get(&key)
            .filter(|b| streaming || b.rate == arrangement.sample_rate)
        {
            buffer.clone()
        } else {
            let buffer = if streaming {
                Buffer {
                    rate: input.audio.rate,
                    samples: input.audio.samples.clone(),
                    offset: input.range.start as usize * input.audio.channels as usize,
                    channels: input.audio.channels as usize,
                }
            } else {
                Buffer {
                    rate: arrangement.sample_rate,
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
        let mut gains = if output_channels == 1 {
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
        if track.pan == 1.0 {
            gains[0] = 0.0;
        } else if track.pan == -1.0 {
            gains[1] = 0.0;
        }
        let audible = !track.mute && (!any_solo || track.solo);
        if !audible {
            gains = [0.0; 2];
        }
        if audible {
            events.push((input.clip.position, true, index));
            events.push((end, false, index));
            if input.clip.tail_frames > 0 {
                tail_events.push((input.clip.position + input.clip.frames, 1i32));
                tail_events.push((end, -1i32));
            }
            bound += input.audio.peak_level * gain;
        }
        clips.push(PreparedClip {
            owner_id: input.clip.id.clone(),
            envelopes: envelopes.remove(&input.clip.id).unwrap_or_default(),
            start: input.clip.position,
            end,
            body_end: input.clip.position + input.clip.frames,
            tail_capped: input.clip.tail_capped,
            key,
            buffer,
            gains,
            stream: streaming.then(|| StreamingInput {
                audio: input.audio.clone(),
                range: input.range.clone(),
                state: input.clip.processing.clone(),
                tempo: arrangement.effect_tempo,
                body,
            }),
        });
        progress(((index + 1) * 80 / input_count.max(1)) as u8);
    }
    if !bound.is_finite() || bound > f32::MAX as f64 {
        return Err(CommandError::new(
            ErrorCode::InvalidParams,
            "Mix can exceed finite float32 range; lower gains",
        ));
    }
    tail_events.sort_unstable();
    let mut tails = 0;
    for (_, delta) in tail_events {
        tails += delta;
        if tails > 64 {
            return Err(crate::rendering::limit(
                "Mix exceeds the separate 64 simultaneous tail-voice budget",
            ));
        }
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
        bounces,
        tail_cache,
        refreshed_clips: inputs.iter().map(|i| i.clip.clone()).collect(),
        revision_id: arrangement.revision_id.clone(),
        arrangement_id: arrangement.id.clone(),
        rate: arrangement.sample_rate,
        frames: clips
            .iter()
            .filter(|c| c.gains != [0.0; 2])
            .map(|c| c.end)
            .max()
            .unwrap_or(0),
        clips,
        segments,
    })
}
