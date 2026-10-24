//! Prepared playback with bounded, allocation-free callback handoff.
//! Native output and the deterministic manual adapter use the same renderer.
use crate::{
    CommandError, ErrorCode,
    audio::Audio,
    documents::{DocumentInfo, FrameRange},
};
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
}

#[derive(Default)]
struct Shared {
    sequence: AtomicU64,
    generation: AtomicU64,
    frame: AtomicU64,
    playing: AtomicBool,
    callbacks: AtomicU64,
    document: [AtomicU64; 2],
    revision: [AtomicU64; 2],
    failed: AtomicBool,
    peak: AtomicU32,
}
impl Shared {
    fn publish(&self, plan: Option<&Plan>, peak: f32) {
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
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.peak.store(peak.to_bits(), Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
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
    Mix {
        graph: Arc<crate::audio::mix::Graph>,
        frames: u64,
    },
}
impl Clone for Source {
    fn clone(&self) -> Self {
        match self {
            Self::Audio(audio) => Self::Audio(audio.clone()),
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
            Self::Audio(audio) => audio.frames(),
            Self::Mix { frames, .. } => *frames,
        }
    }
    fn rate(&self) -> u32 {
        match self {
            Self::Audio(audio) => audio.rate,
            Self::Mix { graph, .. } => graph.rate,
        }
    }
    fn frame(&self, frame: u64) -> [f32; 2] {
        match self {
            Self::Audio(audio) => {
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
            (Self::Audio(a), Self::Audio(b)) => Arc::ptr_eq(a, b),
            (Self::Mix { graph: a, .. }, Self::Mix { graph: b, .. }) => Arc::ptr_eq(a, b),
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
}
struct Renderer {
    incoming: rtrb::Consumer<Box<Plan>>,
    retired: rtrb::Producer<Box<Plan>>,
    current: Option<Box<Plan>>,
    shared: Arc<Shared>,
    rate: u32,
    peak: f32,
}
impl Renderer {
    fn begin(&mut self) {
        self.peak = 0.0;
        // Return ownership to the control thread before replacing a graph. No buffer
        // (or last Arc) is destroyed by this callback, even under handoff pressure.
        while self.current.is_none() || !self.retired.is_full() {
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
                next.position = next.position.min(audio.frames());
                if audio.frames() == 0 {
                    next.playing = false;
                }
                if next.playing {
                    if let Some(bounds) = &next.bounds {
                        if next.position < bounds.start || next.position >= bounds.end {
                            next.position = bounds.start;
                            next.phase = 0;
                        }
                    } else if next.position >= audio.frames() {
                        next.playing = false;
                    }
                }
            } else {
                next.playing = false;
            }
            if let Some(old) = self.current.replace(next) {
                // One producer; the consumer can only free slots after is_full().
                self.retired.push(old).expect("retirement slot reserved");
            }
        }
        if self.shared.failed.load(Ordering::Acquire)
            && let Some(current) = &mut self.current
        {
            current.playing = false;
        }
    }
    fn next(&mut self) -> [f32; 2] {
        let Some(plan) = &mut self.current else {
            return [0.0; 2];
        };
        if !plan.playing {
            return [0.0; 2];
        }
        let Some(audio) = &plan.source else {
            return [0.0; 2];
        };
        let first = plan.position;
        let mut second = first + 1;
        if let Some(bounds) = &plan.bounds
            && second >= bounds.end
        {
            second = bounds.start;
        } else {
            second = second.min(audio.frames() - 1);
        }
        let fraction = plan.phase as f32 / self.rate as f32;
        let first = audio.frame(first);
        let second = audio.frame(second);
        let sample = |channel: usize| {
            let a = first[channel];
            let b = second[channel];
            (a as f64 + (b as f64 - a as f64) * fraction as f64) as f32
        };
        let output = [sample(0) * plan.gain, sample(1) * plan.gain];
        self.peak = self.peak.max(output[0].abs()).max(output[1].abs());
        plan.phase += audio.rate() as u64;
        plan.position += plan.phase / self.rate as u64;
        plan.phase %= self.rate as u64;
        if let Some(bounds) = &plan.bounds {
            if plan.position >= bounds.end {
                plan.position =
                    bounds.start + (plan.position - bounds.start) % (bounds.end - bounds.start);
            }
        } else if plan.position >= audio.frames() {
            plan.position = audio.frames();
            plan.phase = 0;
            plan.playing = false;
        }
        output
    }
    fn end(&self) {
        self.shared.publish(self.current.as_deref(), self.peak);
    }
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
            #[cfg(feature = "audio-output")]
            native: None,
        }
    }
}
impl Playback {
    fn pair(rate: u32) -> (Self, Renderer) {
        let (send, receive) = rtrb::RingBuffer::new(64);
        let (retire, retired) = rtrb::RingBuffer::new(64);
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
        let generation = self.state.requested.generation + 1;
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
        self.pending = Some(Box::new(Plan {
            generation,
            source,
            ids,
            seek,
            bounds,
            playing,
            position: 0,
            phase: 0,
            gain: self.gain,
        }));
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
    pub fn needs_refresh(&self, document: &DocumentInfo, looped: bool) -> bool {
        self.state.requested.document_id.as_ref() == Some(&document.id)
            && (self.state.requested.audio_revision_id.as_ref() != Some(&document.revision_id)
                || self.bounds
                    != looped.then(|| {
                        document.selection.clone().unwrap_or(FrameRange {
                            start: 0,
                            end: document.frames,
                        })
                    }))
    }
    pub fn poll(&mut self) {
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
