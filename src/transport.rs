//! Prepared playback with bounded, allocation-free callback handoff.
//! Native output and the deterministic manual adapter use the same renderer.
use crate::{
    CommandError, ErrorCode,
    audio::Audio,
    documents::{DocumentInfo, FrameRange},
};
mod preparation;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatus {
    #[default]
    Disabled,
    Initializing,
    Ready,
    Failed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    #[default]
    Disabled,
    Manual,
    Device,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct OutputInfo {
    pub kind: OutputKind,
    pub status: OutputStatus,
    pub name: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct RequestedTransport {
    pub document_id: Option<String>,
    pub audio_revision_id: Option<String>,
    pub generation: u64,
    /// None requests preservation of the callback's current position.
    pub frame: Option<u64>,
    pub playing: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct AppliedTransport {
    pub document_id: Option<String>,
    pub audio_revision_id: Option<String>,
    pub generation: u64,
    pub frame: u64,
    pub playing: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct TransportSnapshot {
    pub output: OutputInfo,
    pub requested: RequestedTransport,
    pub applied: AppliedTransport,
    pub pending: bool,
    pub callback_count: u64,
    /// Peak absolute PCM level in the most recently applied output block, before device clipping.
    pub peak_level: f64,
    pub processing: ProcessingHandoff,
    /// At most 128 recent stereo output frames; read on the control thread.
    pub analyzer: Vec<[f32; 2]>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct ProcessingHandoff {
    pub requested_generation: u64,
    pub applied_generation: u64,
    pub pending: bool,
    pub preparation_error: Option<CommandError>,
    pub latency_frames: u64,
    pub tail_capped: bool,
    pub tail_active: bool,
}

struct Analysis {
    samples: [AtomicU32; 256],
    length: AtomicU32,
}
impl Default for Analysis {
    fn default() -> Self {
        Self {
            samples: std::array::from_fn(|_| AtomicU32::new(0)),
            length: AtomicU32::new(0),
        }
    }
}
#[derive(Default)]
struct Shared {
    analysis: Analysis,
    sequence: AtomicU64,
    generation: AtomicU64,
    frame: AtomicU64,
    playing: AtomicBool,
    callbacks: AtomicU64,
    document: [AtomicU64; 2],
    revision: [AtomicU64; 2],
    failed: AtomicBool,
    peak: AtomicU32,
    latency: AtomicU64,
    tail_capped: AtomicBool,
    needs_prepare: AtomicBool,
    processing_failed: AtomicBool,
    processing_key: [AtomicU64; 2],
    pause_for_preparation: AtomicBool,
    tail_active: AtomicBool,
}
impl Shared {
    fn publish(
        &self,
        plan: Option<&Plan>,
        peak: f32,
        samples: &[[f32; 2]; 128],
        cursor: usize,
        length: usize,
    ) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.generation
            .store(plan.map_or(0, |plan| plan.generation), Ordering::Relaxed);
        self.frame
            .store(plan.map_or(0, |plan| plan.position), Ordering::Relaxed);
        self.playing
            .store(plan.is_some_and(|plan| plan.playing), Ordering::Relaxed);
        for (values, id) in [
            (&self.document, plan.map_or(0, |plan| plan.ids.0)),
            (&self.revision, plan.map_or(0, |plan| plan.ids.1)),
        ] {
            values[0].store((id >> 64) as u64, Ordering::Relaxed);
            values[1].store(id as u64, Ordering::Relaxed);
        }
        self.latency.store(
            plan.and_then(|p| p.voice.as_ref()).map_or_else(
                || {
                    plan.and_then(|p| p.mix_voice.as_ref().zip(p.source.as_ref()))
                        .map_or(0, |(voice, source)| match source {
                            Source::Mix { graph, .. } => voice.latency(graph),
                            _ => 0,
                        })
                },
                |v| v.chain.latency() as u64,
            ),
            Ordering::Relaxed,
        );
        self.tail_capped.store(
            plan.and_then(|p| p.voice.as_ref())
                .is_some_and(|v| v.chain.tail_capped)
                || plan
                    .and_then(|p| p.mix_voice.as_ref())
                    .is_some_and(|v| v.tail_capped()),
            Ordering::Relaxed,
        );
        self.processing_failed.store(
            plan.and_then(|p| p.voice.as_ref())
                .is_some_and(|v| v.chain.failed)
                || plan
                    .and_then(|p| p.mix_voice.as_ref())
                    .is_some_and(|v| v.failed),
            Ordering::Relaxed,
        );
        self.tail_active.store(
            plan.is_some_and(|p| {
                p.voice
                    .as_ref()
                    .is_some_and(|v| v.chain.has_tail() && !v.ended)
                    && p.source.as_ref().is_some_and(|s| p.position >= s.frames())
            }),
            Ordering::Relaxed,
        );
        let key = plan.map_or(0, |p| p.processing_key);
        self.processing_key[0].store((key >> 64) as u64, Ordering::Relaxed);
        self.processing_key[1].store(key as u64, Ordering::Relaxed);
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.peak.store(peak.to_bits(), Ordering::Relaxed);
        for index in 0..length {
            let frame = samples[(cursor + 128 - length + index) % 128];
            for (channel, value) in frame.into_iter().enumerate() {
                self.analysis.samples[index * 2 + channel]
                    .store(value.to_bits(), Ordering::Relaxed);
            }
        }
        self.analysis.length.store(length as u32, Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }
    fn analyzer(&self) -> Option<Vec<[f32; 2]>> {
        let before = self.sequence.load(Ordering::Acquire);
        if !before.is_multiple_of(2) {
            return None;
        }
        let length = self.analysis.length.load(Ordering::Relaxed).min(128) as usize;
        let frames = (0..length)
            .map(|index| {
                std::array::from_fn(|channel| {
                    f32::from_bits(
                        self.analysis.samples[index * 2 + channel].load(Ordering::Relaxed),
                    )
                })
            })
            .collect();
        std::sync::atomic::fence(Ordering::SeqCst);
        (before == self.sequence.load(Ordering::Acquire)).then_some(frames)
    }
    fn read(&self) -> Option<(AppliedTransport, u64, f64)> {
        let before = self.sequence.load(Ordering::Acquire);
        if !before.is_multiple_of(2) {
            return None;
        }
        let id = |values: &[AtomicU64; 2]| {
            let raw = (values[0].load(Ordering::Relaxed) as u128) << 64
                | values[1].load(Ordering::Relaxed) as u128;
            (raw != 0).then(|| uuid::Uuid::from_u128(raw).to_string())
        };
        let applied = AppliedTransport {
            document_id: id(&self.document),
            audio_revision_id: id(&self.revision),
            generation: self.generation.load(Ordering::Relaxed),
            frame: self.frame.load(Ordering::Relaxed),
            playing: self.playing.load(Ordering::Relaxed),
        };
        let callbacks = self.callbacks.load(Ordering::Relaxed);
        let peak = f32::from_bits(self.peak.load(Ordering::Relaxed)) as f64;
        std::sync::atomic::fence(Ordering::SeqCst);
        (before == self.sequence.load(Ordering::Acquire)).then_some((applied, callbacks, peak))
    }
}

#[derive(Debug)]
pub(crate) enum Source {
    Audio(Arc<Audio>),
    Processed {
        audio: Arc<Audio>,
        state: Arc<crate::processing::Processing>,
        tempo: f64,
    },
    Mix {
        graph: Arc<crate::audio::mix::Graph>,
        frames: u64,
    },
}
impl Clone for Source {
    fn clone(&self) -> Self {
        match self {
            Self::Audio(audio) => Self::Audio(audio.clone()),
            Self::Processed {
                audio,
                state,
                tempo,
            } => Self::Processed {
                audio: audio.clone(),
                state: state.clone(),
                tempo: *tempo,
            },
            Self::Mix { graph, frames } => Self::Mix {
                graph: graph.clone(),
                frames: *frames,
            },
        }
    }
}
impl Source {
    fn frames(&self) -> u64 {
        match self {
            Self::Audio(audio) | Self::Processed { audio, .. } => audio.frames(),
            Self::Mix { frames, .. } => *frames,
        }
    }
    fn rate(&self) -> u32 {
        match self {
            Self::Audio(audio) | Self::Processed { audio, .. } => audio.rate,
            Self::Mix { graph, .. } => graph.rate,
        }
    }
    fn frame(&self, frame: u64) -> [f32; 2] {
        match self {
            Self::Audio(audio) | Self::Processed { audio, .. } => {
                let offset = frame as usize * audio.channels as usize;
                [
                    audio.samples[offset],
                    audio.samples[offset + usize::from(audio.channels == 2)],
                ]
            }
            Self::Mix { graph, .. } => graph.frame(frame),
        }
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Audio(a) | Self::Processed { audio: a, .. },
                Self::Audio(b) | Self::Processed { audio: b, .. },
            ) => Arc::ptr_eq(a, b),
            (Self::Mix { graph: a, .. }, Self::Mix { graph: b, .. }) => {
                a.arrangement_id == b.arrangement_id
            }
            _ => false,
        }
    }
}
#[derive(Debug)]
struct Plan {
    generation: u64,
    source: Option<Source>,
    ids: (u128, u128),
    seek: Option<u64>,
    bounds: Option<FrameRange>,
    playing: bool,
    position: u64,
    phase: u64,
    gain: f32,
    voice: Option<crate::rendering::Voice>,
    prepared_position: u64,
    processing_key: u128,
    mix_voice: Option<crate::audio::mix::Stream>,
}
struct Renderer {
    incoming: rtrb::Consumer<Box<Plan>>,
    retired: rtrb::Producer<Box<Plan>>,
    current: Option<Box<Plan>>,
    shared: Arc<Shared>,
    rate: u32,
    peak: f32,
    outgoing: Option<Box<Plan>>,
    transition: u32,
    transition_total: u32,
    analysis: [[f32; 2]; 128],
    analysis_cursor: usize,
    analysis_len: usize,
    last: [f32; 2],
    pause_from: [f32; 2],
    pause_remaining: u32,
}
impl Renderer {
    fn begin(&mut self) {
        self.peak = 0.0;
        if self
            .shared
            .pause_for_preparation
            .swap(false, Ordering::AcqRel)
        {
            self.pause_from = self.last;
            self.pause_remaining = (self.rate / 1000).max(1);
            if let Some(plan) = &mut self.current {
                plan.playing = false;
            }
            if let Some(plan) = &mut self.outgoing {
                plan.playing = false;
            }
        }
        if self.transition == 0
            && !self.retired.is_full()
            && let Some(old) = self.outgoing.take()
        {
            self.retired.push(old).expect("retirement slot reserved");
        }
        // Return ownership to the control thread before replacing a graph. No buffer
        // (or last Arc) is destroyed by this callback, even under handoff pressure.
        while self.outgoing.is_none() && (self.current.is_none() || !self.retired.is_full()) {
            let Ok(mut next) = self.incoming.pop() else {
                break;
            };
            let carry =
                self.current
                    .as_ref()
                    .is_some_and(|old| match (&old.source, &next.source) {
                        (Some(a), Some(b)) => a.same(b),
                        _ => false,
                    });
            if next.seek.is_none() && carry {
                let old = self.current.as_ref().unwrap();
                next.position = old.position;
                next.phase = old.phase;
            } else {
                next.position = next.seek.unwrap_or(0);
                next.phase = 0;
            }
            if let Some(audio) = &next.source {
                let tail = next
                    .voice
                    .as_ref()
                    .is_some_and(|v| v.chain.has_tail() && !v.ended)
                    || carry
                        && self
                            .current
                            .as_ref()
                            .and_then(|p| p.voice.as_ref())
                            .is_some_and(|v| v.chain.has_tail() && !v.ended);
                if !tail {
                    next.position = next.position.min(audio.frames());
                }
                if audio.frames() == 0 {
                    next.playing = false;
                }
                if next.playing {
                    if let Some(bounds) = &next.bounds {
                        if next.position < bounds.start || next.position >= bounds.end {
                            next.position = bounds.start;
                            next.phase = 0;
                        }
                    } else if next.position >= audio.frames() && !tail {
                        next.playing = false;
                    }
                }
            } else {
                next.playing = false;
            }
            let mut transferred = false;
            if carry
                && next.seek.is_none()
                && let Some(old) = &mut self.current
            {
                if old.processing_key == next.processing_key && old.bounds == next.bounds {
                    std::mem::swap(&mut next.mix_voice, &mut old.mix_voice);
                    transferred = next.mix_voice.is_some();
                } else if let (
                    Some(a),
                    Some(b),
                    Some(Source::Mix { graph, .. }),
                    Some(Source::Mix { graph: prior, .. }),
                ) = (
                    &mut next.mix_voice,
                    &mut old.mix_voice,
                    &next.source,
                    &old.source,
                ) {
                    transferred = a.retarget(b, graph, prior);
                }
            }
            if carry
                && next.seek.is_none()
                && let Some(old) = &mut self.current
            {
                let compatible = match (&old.source, &next.source) {
                    (
                        Some(Source::Processed {
                            state: a,
                            tempo: at,
                            ..
                        }),
                        Some(Source::Processed {
                            state: b,
                            tempo: bt,
                            ..
                        }),
                    ) => {
                        a.effects == b.effects
                            && a.bypassed == b.bypassed
                            && at == bt
                            && a.envelope.fade_in_frames == b.envelope.fade_in_frames
                            && a.envelope.fade_out_frames == b.envelope.fade_out_frames
                    }
                    _ => false,
                };
                if compatible && old.bounds == next.bounds {
                    std::mem::swap(&mut next.voice, &mut old.voice);
                    transferred = true;
                    if let (Some(voice), Some(Source::Processed { state, .. })) =
                        (&mut next.voice, &next.source)
                    {
                        voice.chain.retarget_envelope(&state.envelope);
                    }
                } else if let (Some(next_voice), Some(prior_voice)) =
                    (&mut next.voice, &mut old.voice)
                {
                    transferred = next_voice.retarget(prior_voice);
                }
            }
            if carry
                && next.seek.is_none()
                && let (Some(voice), Some(Source::Processed { audio, .. })) =
                    (&mut next.voice, &next.source)
            {
                let delta = if next.position >= next.prepared_position {
                    next.position - next.prepared_position
                } else if let Some(bounds) = &next.bounds {
                    bounds.end.saturating_sub(next.prepared_position)
                        + next.position.saturating_sub(bounds.start)
                } else {
                    0
                };
                if !transferred {
                    if delta > 4096 {
                        self.shared.needs_prepare.store(true, Ordering::Release);
                        self.retired.push(next).expect("retirement slot reserved");
                        continue;
                    }
                    voice.advance(audio, delta);
                }
            }
            if carry
                && next.seek.is_none()
                && !transferred
                && let (Some(stream), Some(Source::Mix { graph, .. })) =
                    (&mut next.mix_voice, &next.source)
            {
                let delta = if next.position >= next.prepared_position {
                    next.position - next.prepared_position
                } else if let Some(bounds) = &next.bounds {
                    bounds.end.saturating_sub(next.prepared_position)
                        + next.position.saturating_sub(bounds.start)
                } else {
                    0
                };
                if delta > 4096 {
                    self.shared.needs_prepare.store(true, Ordering::Release);
                    self.retired.push(next).expect("retirement slot reserved");
                    continue;
                }
                stream.advance(graph, delta);
            }
            if (matches!(&next.source, Some(Source::Processed { .. })) && next.voice.is_none())
                || (matches!(&next.source, Some(Source::Mix { .. })) && next.mix_voice.is_none())
            {
                self.shared.needs_prepare.store(true, Ordering::Release);
                self.retired.push(next).expect("retirement slot reserved");
                continue;
            }
            if let Some(old) = self.current.replace(next) {
                let new = self.current.as_ref().unwrap();
                if old.playing && !new.playing && new.source.is_some() {
                    self.pause_from = self.last;
                    self.pause_remaining = (self.rate / 1000).max(1);
                }
                let topology = carry
                    && !transferred
                    && old.playing
                    && new.playing
                    && new.seek.is_none()
                    && match (&old.source, &new.source) {
                        (
                            Some(Source::Processed {
                                state: a,
                                tempo: at,
                                ..
                            }),
                            Some(Source::Processed {
                                state: b,
                                tempo: bt,
                                ..
                            }),
                        ) => {
                            a.effects != b.effects
                                || a.bypassed != b.bypassed
                                || at != bt
                                || a.envelope.fade_in_frames != b.envelope.fade_in_frames
                                || a.envelope.fade_out_frames != b.envelope.fade_out_frames
                        }
                        (Some(Source::Mix { .. }), Some(Source::Mix { .. })) => {
                            old.processing_key != new.processing_key
                        }
                        _ => false,
                    };
                if topology {
                    let continuous = matches!((&old.source,&new.source),(Some(Source::Mix{graph:a,..}),Some(Source::Mix{graph:b,..})) if a.gain_change_only(b));
                    self.transition_total = (self.rate / if continuous { 100 } else { 50 }).max(1);
                    self.outgoing = Some(old);
                    self.transition = self.transition_total;
                } else {
                    self.retired.push(old).expect("retirement slot reserved");
                }
            }
        }
        if self.shared.failed.load(Ordering::Acquire)
            && let Some(current) = &mut self.current
        {
            current.playing = false;
        }
    }
    fn next(&mut self) -> [f32; 2] {
        let mut output = self
            .current
            .as_mut()
            .map_or([0.0; 2], |plan| render_plan(plan, self.rate));
        if self.transition > 0 {
            let previous = self
                .outgoing
                .as_mut()
                .map_or([0.0; 2], |plan| render_plan(plan, self.rate));
            let fraction = 1.0 - self.transition as f32 / self.transition_total as f32;
            output = [
                previous[0] + (output[0] - previous[0]) * fraction,
                previous[1] + (output[1] - previous[1]) * fraction,
            ];
            self.transition -= 1;
        }
        if self.pause_remaining > 0 {
            if self
                .current
                .as_ref()
                .is_some_and(|p| !p.playing && p.source.is_some())
            {
                let fraction = (self.pause_remaining - 1) as f32 / (self.rate / 1000).max(1) as f32;
                output = [self.pause_from[0] * fraction, self.pause_from[1] * fraction];
                self.pause_remaining -= 1;
            } else {
                self.pause_remaining = 0;
            }
        }
        self.last = output;
        self.analysis[self.analysis_cursor] = output;
        self.analysis_cursor = (self.analysis_cursor + 1) % 128;
        self.analysis_len = (self.analysis_len + 1).min(128);
        self.peak = self.peak.max(output[0].abs()).max(output[1].abs());
        output
    }
    fn end(&self) {
        self.shared.publish(
            self.current.as_deref(),
            self.peak,
            &self.analysis,
            self.analysis_cursor,
            self.analysis_len,
        );
    }
}

fn processing_key(source: Option<&Source>, bounds: Option<&FrameRange>) -> u128 {
    let value = match source {
        Some(Source::Processed {
            audio,
            state,
            tempo,
        }) => {
            serde_json::json!({"audio":Arc::as_ptr(audio) as usize,"effects":state.effects,"bypass":state.bypassed,"fade_in":state.envelope.fade_in_frames,"fade_out":state.envelope.fade_out_frames,"tempo":tempo})
        }
        Some(Source::Audio(audio)) => serde_json::json!({"dry":Arc::as_ptr(audio) as usize}),
        Some(Source::Mix { graph, .. }) => serde_json::json!({"mix":graph.revision_id}),
        None => return 0,
    };
    u128::from_str_radix(&crate::projects::checksum(&(value, bounds))[..32], 16).unwrap()
}

fn render_plan(plan: &mut Plan, rate: u32) -> [f32; 2] {
    if plan.voice.as_ref().is_some_and(|v| v.chain.failed)
        || plan.mix_voice.as_ref().is_some_and(|v| v.failed)
    {
        plan.playing = false;
    }
    if !plan.playing {
        return [0.0; 2];
    }
    let Some(audio) = &plan.source else {
        return [0.0; 2];
    };
    let first = plan.position;
    let mut second = first.saturating_add(1);
    if let Some(bounds) = &plan.bounds {
        if second >= bounds.end {
            second = bounds.start;
        }
    } else {
        second = second.min(audio.frames().saturating_sub(1));
    }
    let pair = if let Some(stream) = &plan.mix_voice {
        stream.values
    } else if let Some(voice) = &plan.voice {
        voice.values
    } else if first < audio.frames() {
        [audio.frame(first), audio.frame(second)]
    } else {
        [[0.0; 2]; 2]
    };
    let fraction = plan.phase as f32 / rate as f32;
    let sample = |channel: usize| {
        (pair[0][channel] as f64
            + (pair[1][channel] as f64 - pair[0][channel] as f64) * fraction as f64) as f32
            * plan.gain
    };
    let output = [sample(0), sample(1)];
    plan.phase += audio.rate() as u64;
    let advance = plan.phase / rate as u64;
    plan.position = plan.position.saturating_add(advance);
    plan.phase %= rate as u64;
    if let (Some(voice), Source::Processed { audio, .. }) = (&mut plan.voice, audio) {
        voice.advance(audio, advance);
    }
    if let (Some(stream), Source::Mix { graph, .. }) = (&mut plan.mix_voice, audio) {
        stream.advance(graph, advance);
    }
    if let Some(bounds) = &plan.bounds {
        if plan.position >= bounds.end {
            plan.position =
                bounds.start + (plan.position - bounds.start) % (bounds.end - bounds.start);
        }
    } else if plan.position >= audio.frames() && plan.voice.as_ref().is_none_or(|v| v.ended) {
        plan.position = audio.frames();
        plan.phase = 0;
        plan.playing = false;
    }
    output
}

/// Deterministic stereo output adapter. It renders the same prepared graphs as
/// the native callback without a window, clock or physical audio device.
pub struct ManualOutput {
    renderer: Renderer,
}
impl ManualOutput {
    /// The caller supplies storage; this performs no allocation, locks or I/O.
    pub fn render_into(&mut self, output: &mut [[f32; 2]]) {
        self.renderer.begin();
        for frame in output {
            *frame = self.renderer.next();
        }
        self.renderer.end();
    }
    /// Model a device loss without involving host audio hardware.
    pub fn disconnect(&self) {
        self.renderer.shared.failed.store(true, Ordering::Release);
    }
}

struct Control {
    incoming: rtrb::Producer<Box<Plan>>,
    retired: rtrb::Consumer<Box<Plan>>,
}
pub(crate) struct Playback {
    control: Option<Control>,
    shared: Arc<Shared>,
    state: TransportSnapshot,
    pending: Option<Box<Plan>>,
    source: Option<Source>,
    bounds: Option<FrameRange>,
    ids: (u128, u128),
    gain: f32,
    preparation: Option<preparation::Worker>,
    #[cfg(feature = "audio-output")]
    native: Option<native::Native>,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            control: None,
            shared: Arc::new(Shared::default()),
            state: TransportSnapshot::default(),
            pending: None,
            source: None,
            bounds: None,
            ids: (0, 0),
            gain: 1.0,
            preparation: None,
            #[cfg(feature = "audio-output")]
            native: None,
        }
    }
}
impl Playback {
    fn pair(rate: u32) -> (Self, Renderer) {
        let (send, receive) = rtrb::RingBuffer::new(2);
        let (retire, retired) = rtrb::RingBuffer::new(2);
        let shared = Arc::new(Shared::default());
        let controller = Self {
            control: Some(Control {
                incoming: send,
                retired,
            }),
            shared: shared.clone(),
            ..Self::default()
        };
        (
            controller,
            Renderer {
                incoming: receive,
                retired: retire,
                current: None,
                shared,
                rate,
                peak: 0.0,
                outgoing: None,
                transition: 0,
                transition_total: 1,
                analysis: [[0.0; 2]; 128],
                analysis_cursor: 0,
                analysis_len: 0,
                last: [0.0; 2],
                pause_from: [0.0; 2],
                pause_remaining: 0,
            },
        )
    }
    pub fn manual(rate: u32) -> Result<(Self, ManualOutput), CommandError> {
        if rate == 0 {
            return Err(CommandError::new(
                ErrorCode::InvalidParams,
                "An output sample rate is required",
            ));
        }
        let (mut controller, renderer) = Self::pair(rate);
        controller.state.output = OutputInfo {
            kind: OutputKind::Manual,
            status: OutputStatus::Ready,
            name: Some("Deterministic manual output".into()),
            sample_rate: Some(rate),
            channels: Some(2),
            error: None,
        };
        Ok((controller, ManualOutput { renderer }))
    }
    pub fn configured(&self) -> bool {
        self.control.is_some()
    }
    pub fn snapshot(&self) -> TransportSnapshot {
        self.state.clone()
    }
    pub fn can_request(&self) -> Result<(), CommandError> {
        if self.state.requested.generation == u64::MAX {
            Err(CommandError::new(
                ErrorCode::RevisionExhausted,
                "Transport request counter exhausted; retry output",
            ))
        } else {
            Ok(())
        }
    }
    pub fn ready(&self) -> Result<(), CommandError> {
        match self.state.output.status {
            OutputStatus::Ready => Ok(()),
            OutputStatus::Initializing => Err(CommandError::new(
                ErrorCode::Busy,
                "Audio output is initializing",
            )),
            OutputStatus::Disabled => Err(CommandError::new(
                ErrorCode::MissingCapability,
                "This session has no audio output adapter",
            )),
            OutputStatus::Failed => Err(CommandError::new(
                ErrorCode::AudioDeviceError,
                self.state
                    .output
                    .error
                    .clone()
                    .unwrap_or_else(|| "Audio output failed; retry output".into()),
            )),
        }
    }
    pub fn request(
        &mut self,
        document: &DocumentInfo,
        audio: Arc<Audio>,
        looped: bool,
        playing: bool,
        seek: Option<u64>,
    ) -> Result<(), CommandError> {
        self.request_source(document, Source::Audio(audio), looped, playing, seek)
    }
    pub fn request_processed(
        &mut self,
        document: &DocumentInfo,
        audio: Arc<Audio>,
        tempo: f64,
        looped: bool,
        playing: bool,
        seek: Option<u64>,
    ) -> Result<(), CommandError> {
        let source = Source::Processed {
            audio,
            state: Arc::new(document.processing.clone()),
            tempo,
        };
        self.request_source(document, source, looped, playing, seek)
    }
    pub fn request_mix(
        &mut self,
        document: &DocumentInfo,
        graph: Arc<crate::audio::mix::Graph>,
        looped: bool,
        playing: bool,
        seek: Option<u64>,
    ) -> Result<(), CommandError> {
        self.request_source(
            document,
            Source::Mix {
                graph,
                frames: document.frames,
            },
            looped,
            playing,
            seek,
        )
    }
    fn request_source(
        &mut self,
        document: &DocumentInfo,
        source: Source,
        looped: bool,
        playing: bool,
        seek: Option<u64>,
    ) -> Result<(), CommandError> {
        if playing {
            self.ready()?;
        }
        self.can_request()?;
        let ids = (
            uuid::Uuid::parse_str(&document.id).unwrap().as_u128(),
            uuid::Uuid::parse_str(&document.revision_id)
                .unwrap()
                .as_u128(),
        );
        let bounds = looped.then(|| {
            document.selection.clone().unwrap_or(FrameRange {
                start: 0,
                end: document.frames,
            })
        });
        self.gain = 1.0;
        self.submit(Some(source), ids, bounds, playing, seek)
    }
    fn submit(
        &mut self,
        source: Option<Source>,
        ids: (u128, u128),
        bounds: Option<FrameRange>,
        playing: bool,
        seek: Option<u64>,
    ) -> Result<(), CommandError> {
        self.can_request()?;
        let seek = seek.or_else(|| {
            (self.state.requested.generation != self.state.applied.generation)
                .then_some(self.state.requested.frame)
                .flatten()
        });
        let generation = self.state.requested.generation + 1;
        let processing_key = processing_key(source.as_ref(), bounds.as_ref());
        let applied_key = ((self.shared.processing_key[0].load(Ordering::Acquire) as u128) << 64)
            | self.shared.processing_key[1].load(Ordering::Acquire) as u128;
        let reuse = seek.is_none()
            && processing_key == applied_key
            && matches!(
                &source,
                Some(Source::Processed { .. }) | Some(Source::Mix { .. })
            );
        let mut plan = Box::new(Plan {
            generation,
            source: source.clone(),
            ids,
            seek,
            bounds: bounds.clone(),
            playing,
            position: 0,
            phase: 0,
            gain: self.gain,
            voice: None,
            prepared_position: seek.unwrap_or(self.state.applied.frame),
            processing_key,
            mix_voice: None,
        });
        let expensive = matches!(&source,Some(Source::Processed{state,..}) if !state.effects.is_empty())
            || matches!(&source, Some(Source::Mix {graph,..}) if graph.expensive());
        if expensive && !reuse {
            if seek.is_some() {
                self.shared
                    .pause_for_preparation
                    .store(true, Ordering::Release);
            }
            self.preparation
                .get_or_insert_with(|| preparation::Worker::new(self.shared.clone()))
                .submit(plan)?;
        } else {
            if let Some(worker) = &self.preparation {
                worker.supersede(generation);
            }
            if !reuse {
                preparation::prepare_voice(&mut plan, || false)?;
            }
            self.pending = Some(plan);
        }
        self.state.processing.preparation_error = None;
        self.state.processing.requested_generation = generation;
        self.state.requested = RequestedTransport {
            document_id: (ids.0 != 0).then(|| uuid::Uuid::from_u128(ids.0).to_string()),
            audio_revision_id: (ids.1 != 0).then(|| uuid::Uuid::from_u128(ids.1).to_string()),
            generation,
            frame: seek,
            playing,
        };
        self.source = source.clone();
        self.bounds = bounds.clone();
        self.ids = ids;
        self.flush();
        Ok(())
    }
    fn flush(&mut self) {
        if let Some(control) = &mut self.control {
            while let Ok(retired) = control.retired.pop() {
                drop(retired);
            }
            if let Some(plan) = self.pending.take()
                && let Err(rtrb::PushError::Full(plan)) = control.incoming.push(plan)
            {
                self.pending = Some(plan);
            }
        }
        self.state.processing.pending = self.state.processing.requested_generation
            != self.state.processing.applied_generation
            && self.state.processing.preparation_error.is_none();
        self.state.pending = self.state.output.status == OutputStatus::Ready
            && self.state.requested.generation != self.state.applied.generation;
    }
    pub fn pause(&mut self) -> Result<bool, CommandError> {
        if !self.state.requested.playing {
            return Ok(false);
        }
        self.submit(
            self.source.clone(),
            self.ids,
            self.bounds.clone(),
            false,
            None,
        )?;
        Ok(true)
    }
    pub fn clear(&mut self) -> Result<(), CommandError> {
        if self.source.is_some() {
            self.submit(None, (0, 0), None, false, Some(0))?;
        }
        Ok(())
    }
    pub fn set_gain(&mut self, gain: f32) -> Result<(), CommandError> {
        self.can_request()?;
        self.gain = gain;
        if self.source.is_some() {
            self.submit(
                self.source.clone(),
                self.ids,
                self.bounds.clone(),
                self.state.requested.playing,
                None,
            )?;
        }
        Ok(())
    }
    pub fn needs_refresh(
        &self,
        document: &DocumentInfo,
        looped: bool,
        desired_tempo: Option<f64>,
    ) -> bool {
        self.state.requested.document_id.as_ref() == Some(&document.id)
            && (self.state.requested.audio_revision_id.as_ref() != Some(&document.revision_id)
                || match &self.source {
                    Some(Source::Processed { state, tempo, .. }) => {
                        !state.same_sound(&document.processing)
                            || desired_tempo.is_some_and(|value| value != *tempo)
                    }
                    _ => false,
                }
                || self.bounds
                    != looped.then(|| {
                        document.selection.clone().unwrap_or(FrameRange {
                            start: 0,
                            end: document.frames,
                        })
                    }))
    }
    pub fn poll(&mut self) {
        if let Some(result) = self
            .preparation
            .as_ref()
            .and_then(preparation::Worker::take)
        {
            match result {
                Ok(plan) if plan.generation == self.state.requested.generation => {
                    self.pending = Some(plan)
                }
                Err((generation, error)) if generation == self.state.requested.generation => {
                    self.state.processing.preparation_error = Some(error)
                }
                _ => {}
            }
        }
        if self.shared.needs_prepare.swap(false, Ordering::AcqRel) {
            self.shared.processing_key[0].store(0, Ordering::Release);
            self.shared.processing_key[1].store(0, Ordering::Release);
            let _ = self.submit(
                self.source.clone(),
                self.ids,
                self.bounds.clone(),
                self.state.requested.playing,
                None,
            );
        }
        #[cfg(feature = "audio-output")]
        if let Some(native) = &self.native {
            while let Ok(info) = native.updates.try_recv() {
                self.state.output = info;
            }
        }
        if let Some((applied, count, peak)) = self.shared.read() {
            self.state.applied = applied;
            self.state.callback_count = count;
            self.state.peak_level = peak;
            if let Some(analyzer) = self.shared.analyzer() {
                self.state.analyzer = analyzer;
            }
            self.state.processing.applied_generation = self.state.applied.generation;
            self.state.processing.latency_frames = self.shared.latency.load(Ordering::Relaxed);
            self.state.processing.tail_capped = self.shared.tail_capped.load(Ordering::Relaxed);
            self.state.processing.tail_active = self.shared.tail_active.load(Ordering::Relaxed);
            if self.shared.processing_failed.load(Ordering::Relaxed) {
                self.state.processing.preparation_error = Some(CommandError::new(
                    ErrorCode::RenderFailed,
                    "Processor produced nonfinite output",
                ));
            }
        }
        if self.shared.failed.load(Ordering::Acquire) {
            self.state.output.status = OutputStatus::Failed;
            self.state
                .output
                .error
                .get_or_insert_with(|| "Audio device failed; reconnect it and retry output".into());
            self.state.requested.playing = false;
            self.state.applied.playing = false;
        }
        if self.state.applied.generation == self.state.requested.generation
            && !self.state.applied.playing
        {
            self.state.requested.playing = false;
        }
        self.flush();
    }
    pub fn retry_manual(&mut self) {
        self.shared.failed.store(false, Ordering::Release);
        self.state.output.status = OutputStatus::Ready;
        self.state.output.error = None;
    }
}

#[cfg(feature = "audio-output")]
mod native;

#[cfg(test)]
mod processing_tests {
    use super::*;
    #[test]
    fn superseded_or_failed_preparation_keeps_the_previous_audible_graph() {
        let cancel = AtomicBool::new(false);
        let audio = Arc::new(
            crate::audio::prepare(
                48000,
                1,
                "float32".into(),
                Default::default(),
                vec![0.5; 4096],
                &cancel,
                |_| {},
            )
            .unwrap(),
        );
        let mut document:DocumentInfo=serde_json::from_value(serde_json::json!({
            "id":uuid::Uuid::new_v4().to_string(),"revision_id":uuid::Uuid::new_v4().to_string(),"path":"probe.wav","name":"probe","sample_rate":48000,"channels":1,"frames":4096,"encoding":"float32","dirty":false,"cursor":0,"peak_level":0.5,"overloaded_samples":0,"processing":crate::processing::Processing::default(),"saved_processing":crate::processing::Processing::default()
        })).unwrap();
        let (mut playback, mut output) = Playback::manual(48000).unwrap();
        playback
            .request_processed(&document, audio.clone(), 120.0, true, true, Some(0))
            .unwrap();
        let mut frames = [[0.0; 2]; 16];
        output.render_into(&mut frames);
        playback.poll();
        let prior = playback.state.applied.generation;
        document
            .processing
            .effects
            .push(crate::processing::Instance {
                id: uuid::Uuid::new_v4().to_string(),
                kind: crate::processing::EffectKind::Reverb,
                bypassed: false,
                parameters: Default::default(),
            });
        for _ in 0..100 {
            playback
                .request_processed(&document, audio.clone(), 120.0, true, true, None)
                .unwrap();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while playback.state.processing.preparation_error.is_none() {
            playback.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            playback
                .state
                .processing
                .preparation_error
                .as_ref()
                .unwrap()
                .code,
            ErrorCode::MissingCapability
        );
        output.render_into(&mut frames);
        playback.poll();
        assert_eq!(frames, [[0.5; 2]; 16]);
        assert_eq!(playback.state.applied.generation, prior);
        document.processing.effects.clear();
        playback
            .request_processed(&document, audio, 120.0, true, true, None)
            .unwrap();
        output.render_into(&mut frames);
        playback.poll();
        assert!(playback.state.processing.preparation_error.is_none());
        assert_eq!(
            playback.state.applied.generation,
            playback.state.requested.generation
        );
    }
    struct Echo {
        delay: crate::rendering::DelayLine,
        pending: u32,
    }
    impl crate::rendering::Processor for Echo {
        fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            if input.iter().any(|v| *v != 0.0) {
                self.pending = 7;
            } else {
                self.pending = self.pending.saturating_sub(1);
            }
            self.delay.frame(input)
        }
        fn has_tail(&self) -> bool {
            true
        }
        fn activity(&self) -> f32 {
            if self.pending > 0 { 1.0 } else { 0.0 }
        }
        fn bytes(&self) -> usize {
            56
        }
        fn reset(&mut self) {
            self.delay.reset();
            self.pending = 0;
        }
    }
    #[test]
    fn pausing_in_a_tail_preserves_state_and_resumes_without_rewinding_the_body() {
        let cancel = AtomicBool::new(false);
        let audio = Arc::new(
            crate::audio::prepare(
                48000,
                1,
                "float32".into(),
                Default::default(),
                vec![1.0, 0.0, 0.0, 0.0],
                &cancel,
                |_| {},
            )
            .unwrap(),
        );
        let document:DocumentInfo=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"revision_id":uuid::Uuid::new_v4().to_string(),"path":"probe.wav","name":"probe","sample_rate":48000,"channels":1,"frames":4,"encoding":"float32","dirty":false,"cursor":0,"peak_level":1.0,"overloaded_samples":0,"processing":crate::processing::Processing::default(),"saved_processing":crate::processing::Processing::default()})).unwrap();
        let (mut playback, mut output) = Playback::manual(48000).unwrap();
        playback
            .request_processed(&document, audio.clone(), 120.0, false, true, Some(0))
            .unwrap();
        let mut plan = output.renderer.incoming.pop().unwrap();
        let chain = crate::rendering::Chain::with_processors(
            &document.processing,
            48000,
            crate::rendering::Channels::Mono,
            vec![(
                Box::new(Echo {
                    delay: crate::rendering::DelayLine::new(7).unwrap(),
                    pending: 0,
                }),
                false,
            )],
        )
        .unwrap();
        plan.voice =
            Some(crate::rendering::Voice::with_chain(chain, &audio, 0, None, || false).unwrap());
        playback
            .control
            .as_mut()
            .unwrap()
            .incoming
            .push(plan)
            .unwrap();
        output.render_into(&mut [[0.0; 2]; 4]);
        playback.poll();
        assert!(playback.state.processing.tail_active && playback.state.applied.playing);
        playback.pause().unwrap();
        output.render_into(&mut [[0.0; 2]; 64]);
        playback.poll();
        assert_eq!(playback.state.applied.frame, 4);
        assert!(playback.state.processing.tail_active);
        playback
            .request_processed(&document, audio, 120.0, false, true, None)
            .unwrap();
        let mut frames = [[0.0; 2]; 4];
        output.render_into(&mut frames);
        playback.poll();
        assert_eq!(frames, [[0.0; 2], [0.0; 2], [0.0; 2], [1.0; 2]]);
        assert_eq!(playback.state.applied.frame, 8);
    }
    struct Affine {
        gain: f32,
        offset: f32,
    }
    impl crate::rendering::Processor for Affine {
        fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            [
                input[0] * self.gain + self.offset,
                input[1] * self.gain + self.offset,
            ]
        }
        fn reset(&mut self) {}
    }
    #[test]
    fn prepared_bypass_reorder_and_remove_transition_to_the_correct_signal() {
        // Unregistered arithmetic probes exercise host topology, not effect fidelity.
        let cancel = AtomicBool::new(false);
        let audio = Arc::new(
            crate::audio::prepare(
                48000,
                1,
                "float32".into(),
                Default::default(),
                vec![0.5; 8192],
                &cancel,
                |_| {},
            )
            .unwrap(),
        );
        let (mut control, renderer) = Playback::pair(48000);
        let mut output = ManualOutput { renderer };
        let gain = crate::processing::Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: crate::processing::EffectKind::SoftClipper,
            bypassed: false,
            parameters: Default::default(),
        };
        let bias = crate::processing::Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: crate::processing::EffectKind::Balance,
            bypassed: false,
            parameters: Default::default(),
        };
        let mut bypassed = gain.clone();
        bypassed.bypassed = true;
        for (step, effects, expected) in [
            (0, vec![gain.clone(), bias.clone()], 0.35),
            (1, vec![bias.clone(), gain.clone()], 0.3),
            (2, vec![bypassed, bias], 0.6),
            (3, vec![gain], 0.25),
        ] {
            let state = Arc::new(crate::processing::Processing {
                effects: effects.clone(),
                ..Default::default()
            });
            let processors = effects
                .iter()
                .map(|effect| {
                    let value = if effect.kind == crate::processing::EffectKind::SoftClipper {
                        Affine {
                            gain: 0.5,
                            offset: 0.0,
                        }
                    } else {
                        Affine {
                            gain: 1.0,
                            offset: 0.1,
                        }
                    };
                    (
                        Box::new(value) as Box<dyn crate::rendering::Processor>,
                        effect.bypassed,
                    )
                })
                .collect();
            let chain = crate::rendering::Chain::with_processors(
                &state,
                48000,
                crate::rendering::Channels::Mono,
                processors,
            )
            .unwrap();
            let position = output.renderer.current.as_ref().map_or(0, |p| p.position);
            let bounds = Some(FrameRange {
                start: 0,
                end: 8192,
            });
            let voice = crate::rendering::Voice::with_chain(
                chain,
                &audio,
                position,
                bounds.clone(),
                || false,
            )
            .unwrap();
            let source = Some(Source::Processed {
                audio: audio.clone(),
                state,
                tempo: 120.0,
            });
            let key = processing_key(source.as_ref(), bounds.as_ref());
            let plan = Box::new(Plan {
                generation: step + 1,
                source,
                ids: (1, 2),
                seek: if step == 0 { Some(0) } else { None },
                bounds,
                playing: true,
                position: 0,
                phase: 0,
                gain: 1.0,
                voice: Some(voice),
                prepared_position: position,
                mix_voice: None,
                processing_key: key,
            });
            control
                .control
                .as_mut()
                .unwrap()
                .incoming
                .push(plan)
                .unwrap();
            let mut frames = [[0.0; 2]; 1024];
            output.render_into(&mut frames);
            control.poll();
            assert!((frames[1023][0] - expected).abs() < 1e-6);
            assert!(frames.iter().all(|f| f.iter().all(|v| v.is_finite())));
            assert_eq!(control.state.applied.generation, step + 1);
        }
    }
}
