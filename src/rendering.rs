//! Shared streaming processor contract and deterministic offline driver.
//! All storage is prepared before `frame`; processing never allocates or locks.
use crate::{
    CommandError, ErrorCode,
    processing::{Envelope, Processing},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) const MAX_CHAIN_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_LATENCY_SECONDS: usize = 1;
pub(crate) const TAIL_SECONDS: u64 = 60;
const QUIET: f32 = 0.00001;
static PREPARED_BYTES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const MAX_PREPARED_BYTES: usize = 256 * 1024 * 1024;
struct MemoryLease(usize);
impl MemoryLease {
    fn acquire(bytes: usize) -> Result<Self, CommandError> {
        use std::sync::atomic::Ordering;
        PREPARED_BYTES
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|next| *next <= MAX_PREPARED_BYTES)
            })
            .map(|_| Self(bytes))
            .map_err(|_| limit("Prepared processing exceeds the shared 256 MiB budget"))
    }
}
impl Drop for MemoryLease {
    fn drop(&mut self) {
        PREPARED_BYTES.fetch_sub(self.0, std::sync::atomic::Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channels {
    Mono,
    Stereo,
}
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct RenderMetadata {
    pub source_body_frames: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub latency_frames: u64,
    pub tail_frames: u64,
    pub tail_capped: bool,

    pub render_identity: String,
    pub audio_revision_id: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderParams {
    pub owner: crate::processing::Owner,
    pub expected_render_identity: Option<String>,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SamplesParams {
    pub audio_revision_id: String,
    pub start_frame: u64,
    pub frames: u32,
}

pub(crate) fn offline(
    source: &crate::audio::Audio,
    range: crate::documents::FrameRange,
    state: &Processing,
    tempo: f64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(crate::audio::Audio, RenderMetadata), CommandError> {
    if range.start > range.end || range.end > source.frames() {
        return Err(crate::processing::invalid(
            "Render source interval is invalid",
        ));
    }
    let body = range.end - range.start;
    state.envelope.validate(body)?;
    let channels = if source.channels == 1 {
        Channels::Mono
    } else {
        Channels::Stereo
    };
    let mut chain = Chain::prepare(state, source.rate, channels, tempo)?;
    let latency = chain.latency() as u64;
    let channels = chain.channels();
    let max_values = 128 * 1024 * 1024;
    if body.saturating_mul(channels as u64) > max_values {
        return Err(limit("Rendered body exceeds 512 MiB PCM limit"));
    }
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(body as usize * channels as usize)
        .map_err(|_| limit("Cannot allocate rendered audio"))?;
    let max = if body == 0 {
        0
    } else {
        body.saturating_add(latency)
            .saturating_add(source.rate as u64 * TAIL_SECONDS)
    };
    for frame in 0..max.max(body + latency) {
        if frame.is_multiple_of(1024) {
            crate::exports::check_cancel(cancel)?;
        }
        let input = if frame < body {
            let offset = (range.start + frame) as usize * source.channels as usize;
            [
                source.samples[offset],
                source.samples[offset + usize::from(source.channels == 2)],
            ]
        } else {
            [0.0; 2]
        };
        let output = chain.frame(input, frame, body);
        if chain.failed {
            return Err(CommandError::new(
                ErrorCode::RenderFailed,
                "Processor produced nonfinite output or state",
            ));
        }
        if frame >= latency {
            if samples.len() as u64 + channels as u64 > max_values {
                return Err(limit("Rendered body and tail exceed 512 MiB PCM limit"));
            }
            if samples.capacity() - samples.len() < channels as usize {
                samples
                    .try_reserve(4096)
                    .map_err(|_| limit("Cannot allocate effect tail"))?;
            }
            samples.extend_from_slice(&output[..channels as usize]);
        }
        if frame + 1 >= body + latency && chain.drained() {
            break;
        }
    }
    let tail_frames = (samples.len() as u64 / channels as u64).saturating_sub(body);
    let metadata = RenderMetadata {
        source_body_frames: body,
        sample_rate: source.rate,
        channels,
        latency_frames: latency,
        tail_frames,
        tail_capped: chain.tail_capped,
        render_identity: String::new(),
        audio_revision_id: uuid::Uuid::new_v4().to_string(),
    };
    let audio = crate::audio::prepare(
        source.rate,
        channels,
        "float32".into(),
        Default::default(),
        samples,
        cancel,
        |_| {},
    )?;
    Ok((audio, metadata))
}

/// Implementations declare computational delay separately from intentional echoes.
/// `activity` bounds retained state, including silent predelay, not just output.
pub(crate) trait Processor: Send + std::any::Any {
    fn frame(&mut self, input: [f32; 2]) -> [f32; 2];
    fn latency(&self) -> usize {
        0
    }
    fn stereo(&self) -> bool {
        false
    }
    fn has_tail(&self) -> bool {
        false
    }
    fn activity(&self) -> f32 {
        0.0
    }
    fn bytes(&self) -> usize {
        0
    }
    fn reset(&mut self);
    /// Continuous controls may transfer state into preallocated replacement
    /// storage and slew their targets over 10 ms. Topology changes return false.
    fn can_retarget(&self, _previous: &dyn Processor) -> bool {
        false
    }
    fn retarget(&mut self, _previous: &mut dyn Processor) {}
}

pub(crate) struct DelayLine {
    values: Vec<[f32; 2]>,
    cursor: usize,
}
impl DelayLine {
    pub fn new(frames: usize) -> Result<Self, CommandError> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(frames)
            .map_err(|_| limit("Cannot allocate processor delay"))?;
        values.resize(frames, [0.0; 2]);
        Ok(Self { values, cursor: 0 })
    }
    pub fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.values.is_empty() {
            return input;
        }
        let output = std::mem::replace(&mut self.values[self.cursor], input);
        self.cursor = (self.cursor + 1) % self.values.len();
        output
    }
    pub fn reset(&mut self) {
        self.values.fill([0.0; 2]);
        self.cursor = 0;
    }
}
struct Stage {
    processor: Box<dyn Processor>,
    bypassed: bool,
    dry: DelayLine,
}
pub(crate) struct Chain {
    tempo: f64,
    state: Processing,
    stages: Vec<Stage>,
    envelope: Envelope,
    bypassed: bool,
    dry: DelayLine,
    latency: usize,
    channels: Channels,
    gain: f64,
    target_gain: f64,
    step: f64,
    remaining: u32,
    rate: u32,
    tail: u64,
    quiet: u64,
    pub failed: bool,
    pub tail_capped: bool,
    active: bool,
    _memory: MemoryLease,
}
impl Chain {
    pub fn prepare(
        state: &Processing,
        rate: u32,
        channels: Channels,
        tempo: f64,
    ) -> Result<Self, CommandError> {
        if !(1..=384000).contains(&rate) {
            return Err(crate::processing::invalid(
                "Processing rate must be 1..384000 Hz",
            ));
        }
        state.validate(u64::MAX)?;
        // Production constructors are added by each algorithm ticket. Unavailable
        // effects are rejected by validation; they are never identity processors.
        if !tempo.is_finite() || !(20.0..=400.0).contains(&tempo) {
            return Err(crate::processing::invalid(
                "Effect tempo must be 20..400 BPM",
            ));
        }
        let mut chain = Self::with_processors(state, rate, channels, Vec::new())?;
        chain.tempo = tempo;
        Ok(chain)
    }
    pub(crate) fn with_processors(
        state: &Processing,
        rate: u32,
        channels: Channels,
        processors: Vec<(Box<dyn Processor>, bool)>,
    ) -> Result<Self, CommandError> {
        let latency: usize = processors.iter().map(|(p, _)| p.latency()).sum();
        let bytes: usize = processors
            .iter()
            .map(|(p, _)| p.bytes() + p.latency() * 8)
            .sum::<usize>()
            + latency * 8;
        if bytes > MAX_CHAIN_BYTES || latency > rate as usize * MAX_LATENCY_SECONDS {
            return Err(limit(
                "Chain exceeds 64 MiB or one second computational latency",
            ));
        }
        let mut stages = Vec::with_capacity(processors.len());
        let mut output_channels = channels;
        for (processor, bypassed) in processors {
            if processor.stereo() {
                output_channels = Channels::Stereo;
            }
            let dry = DelayLine::new(processor.latency())?;
            stages.push(Stage {
                processor,
                bypassed,
                dry,
            });
        }
        let gain = state
            .envelope
            .gain_db
            .map_or(0.0, |db| 10.0_f64.powf(db / 20.0));
        let memory = MemoryLease::acquire(
            bytes + std::mem::size_of::<Self>() + stages.capacity() * std::mem::size_of::<Stage>(),
        )?;
        Ok(Self {
            tempo: 120.0,
            state: state.clone(),
            stages,
            envelope: state.envelope.clone(),
            bypassed: state.bypassed,
            dry: DelayLine::new(latency)?,
            latency,
            channels: output_channels,
            gain,
            target_gain: gain,
            step: 0.0,
            remaining: 0,
            rate,
            tail: 0,
            quiet: 0,
            failed: false,
            tail_capped: false,
            active: false,
            _memory: memory,
        })
    }
    pub fn latency(&self) -> usize {
        self.latency
    }
    pub fn channels(&self) -> u16 {
        if self.channels == Channels::Mono {
            1
        } else {
            2
        }
    }
    pub fn has_tail(&self) -> bool {
        !self.bypassed
            && self
                .stages
                .iter()
                .any(|s| !s.bypassed && s.processor.has_tail())
    }
    #[cfg(test)]
    pub fn smooth_gain_from(&mut self, previous: &Self) {
        self.gain = previous.gain;
        self.remaining = (self.rate / 100).max(1);
        self.step = (self.target_gain - self.gain) / self.remaining as f64;
    }
    pub fn retarget_envelope(&mut self, envelope: &Envelope) {
        if self.envelope == *envelope {
            return;
        }
        self.envelope = envelope.clone();
        self.target_gain = envelope.gain_db.map_or(0.0, |db| 10.0_f64.powf(db / 20.0));
        self.remaining = (self.rate / 100).max(1);
        self.step = (self.target_gain - self.gain) / self.remaining as f64;
    }
    pub fn can_retarget(&self, previous: &Self) -> bool {
        self.tempo == previous.tempo
            && self.channels == previous.channels
            && self.latency == previous.latency
            && self.state.bypassed == previous.state.bypassed
            && self.state.envelope.fade_in_frames == previous.state.envelope.fade_in_frames
            && self.state.envelope.fade_out_frames == previous.state.envelope.fade_out_frames
            && self.stages.len() == previous.stages.len()
            && self.state.effects.len() == self.stages.len()
            && previous.state.effects.len() == previous.stages.len()
            && self
                .state
                .effects
                .iter()
                .zip(&previous.state.effects)
                .zip(self.stages.iter().zip(&previous.stages))
                .all(|((next, old), (stage, prior))| {
                    next.id == old.id
                        && next.kind == old.kind
                        && next.bypassed == old.bypassed
                        && (next.parameters == old.parameters
                            || stage.processor.can_retarget(prior.processor.as_ref()))
                })
    }
    pub fn retarget(&mut self, previous: &mut Self) {
        for (((stage, prior), next), old) in self
            .stages
            .iter_mut()
            .zip(&mut previous.stages)
            .zip(&self.state.effects)
            .zip(&previous.state.effects)
        {
            if next.parameters == old.parameters {
                std::mem::swap(&mut stage.processor, &mut prior.processor);
            } else {
                stage.processor.retarget(prior.processor.as_mut());
            }
            std::mem::swap(&mut stage.dry, &mut prior.dry);
        }
        std::mem::swap(&mut self.dry, &mut previous.dry);
        self.gain = previous.gain;
        self.remaining = (self.rate / 100).max(1);
        self.step = (self.target_gain - self.gain) / self.remaining as f64;
        self.tail = previous.tail;
        self.quiet = previous.quiet;
        self.active = previous.active;
    }
    pub fn reset(&mut self) {
        self.dry.reset();
        for stage in &mut self.stages {
            stage.processor.reset();
            stage.dry.reset();
        }
        self.gain = self.target_gain;
        self.remaining = 0;
        self.tail = 0;
        self.quiet = 0;
        self.failed = false;
        self.tail_capped = false;
        self.active = false;
    }
    pub fn frame(&mut self, input: [f32; 2], source_frame: u64, body: u64) -> [f32; 2] {
        if self.remaining > 0 {
            self.gain += self.step;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.gain = self.target_gain;
            }
        }
        let mut envelope = self.envelope.clone();
        // gain is evaluated separately, so continuous gain updates can slew while
        // frame-anchored fades retain exact reset-render arithmetic.
        envelope.gain_db = Some(0.0);
        let gain = (self.gain * envelope.amplitude(source_frame, body)) as f32;
        let mut value = [input[0] * gain, input[1] * gain];
        let dry = self.dry.frame(value);
        let mut activity = 0.0_f32;
        for stage in &mut self.stages {
            let dry_stage = stage.dry.frame(value);
            value = stage.processor.frame(value);
            if stage.bypassed {
                value = dry_stage;
            } else {
                activity = activity.max(stage.processor.activity());
            }
        }
        if self.bypassed {
            value = dry;
            activity = 0.0;
        }
        if !value.iter().all(|v| v.is_finite()) || !activity.is_finite() {
            self.failed = true;
            return [0.0; 2];
        }
        self.active |= value[0] != 0.0 || value[1] != 0.0 || activity != 0.0;
        if source_frame >= body {
            self.tail += 1;
            let level = value[0].abs().max(value[1].abs()).max(activity);
            if level < QUIET {
                self.quiet += 1;
            } else {
                self.quiet = 0;
            }
            let cap = TAIL_SECONDS * self.rate as u64 + self.latency as u64;
            let taper = (self.rate as u64 / 100).max(1);
            if self.tail > cap.saturating_sub(taper) {
                let gain = cap.saturating_sub(self.tail) as f32 / taper as f32;
                value[0] *= gain;
                value[1] *= gain;
            }
            if self.tail >= cap {
                self.tail_capped = true;
            }
        }
        value
    }
    pub fn drained(&self) -> bool {
        self.failed
            || self.tail_capped
            || !self.active
            || !self.has_tail()
            || self.quiet >= (self.rate as u64 / 10).max(1)
    }
}
pub(crate) fn limit(message: &str) -> CommandError {
    CommandError::new(ErrorCode::ResourceLimit, message)
}

/// Two-frame streaming resampler cache. Stateful DSP runs once per source frame,
/// before the transport's output-rate interpolation. Loop wrapping feeds the
/// beginning into the existing state; pause preserves this entire object.
pub(crate) struct Voice {
    pub chain: Chain,
    pub values: [[f32; 2]; 2],
    input_position: u64,
    source_frames: u64,
    source_start: u64,
    bounds: Option<crate::documents::FrameRange>,
    pub ended: bool,
    last: [f32; 2],
}
impl Voice {
    pub fn retarget(&mut self, previous: &mut Self) -> bool {
        if self.bounds != previous.bounds || !self.chain.can_retarget(&previous.chain) {
            return false;
        }
        self.chain.retarget(&mut previous.chain);
        self.values = previous.values;
        self.input_position = previous.input_position;
        self.last = previous.last;
        self.ended = previous.ended;
        true
    }
    pub fn prepare(
        state: &Processing,
        source: &crate::audio::Audio,
        tempo: f64,
        position: u64,
        bounds: Option<crate::documents::FrameRange>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, CommandError> {
        let channels = if source.channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let chain = Chain::prepare(state, source.rate, channels, tempo)?;
        Self::with_chain(chain, source, position, bounds, cancelled)
    }
    pub fn prepare_range(
        state: &Processing,
        source: &crate::audio::Audio,
        range: &crate::documents::FrameRange,
        tempo: f64,
        position: u64,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, CommandError> {
        let channels = if source.channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let chain = Chain::prepare(state, source.rate, channels, tempo)?;
        Self::with_range(chain, source, range, position, None, cancelled)
    }
    pub(crate) fn with_chain(
        chain: Chain,
        source: &crate::audio::Audio,
        position: u64,
        bounds: Option<crate::documents::FrameRange>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, CommandError> {
        Self::with_range(
            chain,
            source,
            &crate::documents::FrameRange {
                start: 0,
                end: source.frames(),
            },
            position,
            bounds,
            cancelled,
        )
    }
    fn with_range(
        mut chain: Chain,
        source: &crate::audio::Audio,
        range: &crate::documents::FrameRange,
        position: u64,
        bounds: Option<crate::documents::FrameRange>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, CommandError> {
        chain.reset();
        let mut voice = Self {
            chain,
            values: [[0.0; 2]; 2],
            input_position: 0,
            source_frames: range.end - range.start,
            source_start: range.start,
            bounds,
            ended: false,
            last: [0.0; 2],
        };
        // Bound seek context by the same maximum state lifetime as an offline tail.
        let start = if voice.chain.has_tail() {
            position.saturating_sub(source.rate as u64 * TAIL_SECONDS)
        } else {
            position
        };
        voice.input_position = start;
        let target = position.saturating_add(voice.chain.latency() as u64);
        for frame in 0..target.saturating_sub(start) {
            if frame.is_multiple_of(1024) && cancelled() {
                return Err(CommandError::new(
                    ErrorCode::Cancelled,
                    "Processing preparation superseded",
                ));
            }
            voice.read(source);
            if voice.chain.failed {
                return Err(CommandError::new(
                    ErrorCode::RenderFailed,
                    "Processor failed during context preparation",
                ));
            }
        }
        voice.values[0] = voice.read(source);
        voice.values[1] = voice.read(source);
        if voice.chain.failed {
            return Err(CommandError::new(
                ErrorCode::RenderFailed,
                "Processor failed during preparation",
            ));
        }
        Ok(voice)
    }
    fn read(&mut self, source: &crate::audio::Audio) -> [f32; 2] {
        let frame = self.input_position;
        let input = if frame < self.source_frames {
            let offset = (frame + self.source_start) as usize * source.channels as usize;
            [
                source.samples[offset],
                source.samples[offset + usize::from(source.channels == 2)],
            ]
        } else {
            [0.0; 2]
        };
        let mut output = self.chain.frame(input, frame, self.source_frames);
        if frame < self.source_frames + self.chain.latency() as u64 {
            self.last = output;
        } else if !self.chain.has_tail() {
            output = self.last;
        }
        self.input_position = self.input_position.saturating_add(1);
        if let Some(bounds) = &self.bounds {
            if self.input_position >= bounds.end {
                self.input_position = bounds.start;
            }
        } else if frame >= self.source_frames && self.chain.drained() {
            self.ended = true;
        }
        output
    }
    pub fn advance(&mut self, source: &crate::audio::Audio, frames: u64) {
        for _ in 0..frames {
            self.values[0] = self.values[1];
            self.values[1] = self.read(source);
        }
    }
    /// A loop is a new input interval in the existing processor state.
    pub fn wrap(&mut self, source: &crate::audio::Audio, position: u64) {
        self.input_position = position;
        self.ended = false;
        self.values[0] = self.read(source);
        self.values[1] = self.read(source);
    }
}

impl std::fmt::Debug for Voice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice")
            .field("position", &self.input_position)
            .field("ended", &self.ended)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Probe {
        delay: DelayLine,
        frames: usize,
    }
    impl Processor for Probe {
        fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            self.delay.frame(input)
        }
        fn latency(&self) -> usize {
            self.frames
        }
        fn activity(&self) -> f32 {
            self.delay
                .values
                .iter()
                .flatten()
                .map(|x| x.abs())
                .fold(0.0, f32::max)
        }
        fn bytes(&self) -> usize {
            self.frames * 8
        }
        fn reset(&mut self) {
            self.delay.reset();
        }
    }
    #[test]
    fn latency_and_bypass_share_a_clock_without_erasing_intentional_time() {
        for bypassed in [false, true] {
            let state = Processing {
                bypassed,
                ..Processing::default()
            };
            let mut chain = Chain::with_processors(
                &state,
                48000,
                Channels::Mono,
                vec![(
                    Box::new(Probe {
                        delay: DelayLine::new(7).unwrap(),
                        frames: 7,
                    }),
                    false,
                )],
            )
            .unwrap();
            assert_eq!(chain.latency(), 7);
            let output: Vec<_> = (0..20)
                .map(|frame| chain.frame(if frame == 0 { [1.0; 2] } else { [0.0; 2] }, frame, 10))
                .collect();
            assert_eq!(output[7], [1.0; 2]);
            assert!(
                output
                    .iter()
                    .enumerate()
                    .all(|(i, x)| i == 7 || *x == [0.0; 2])
            );
            chain.reset();
            assert_eq!(chain.frame([0.0; 2], 0, 10), [0.0; 2]);
        }
    }
    #[test]
    fn envelope_edges_and_gain_slew_are_deterministic() {
        let mut state = Processing {
            envelope: Envelope {
                gain_db: Some(-6.020599913279624),
                fade_in_frames: 4,
                fade_out_frames: 4,
            },
            ..Processing::default()
        };
        let mut chain = Chain::prepare(&state, 48000, Channels::Mono, 120.0).unwrap();
        let output: Vec<_> = (0..4).map(|n| chain.frame([1.0; 2], n, 4)[0]).collect();
        assert_eq!(output, vec![0.0, 1.0 / 9.0, 1.0 / 9.0, 0.0]);
        state.envelope = Envelope::unity();
        let mut next = Chain::prepare(&state, 48000, Channels::Mono, 120.0).unwrap();
        next.smooth_gain_from(&chain);
        for n in 0..480 {
            next.frame([1.0; 2], n, 1000);
        }
        assert_eq!(next.frame([1.0; 2], 480, 1000), [1.0; 2]);
    }
    struct Sustain {
        value: f32,
        feedback: f32,
        invalid: bool,
    }
    impl Processor for Sustain {
        fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            self.value = input[0] + self.value * self.feedback;
            if self.invalid {
                [f32::NAN; 2]
            } else {
                [self.value; 2]
            }
        }
        fn has_tail(&self) -> bool {
            true
        }
        fn activity(&self) -> f32 {
            self.value.abs()
        }
        fn reset(&mut self) {
            self.value = 0.0;
        }
    }
    fn sustain(feedback: f32, invalid: bool) -> Chain {
        Chain::with_processors(
            &Processing::default(),
            1000,
            Channels::Stereo,
            vec![(
                Box::new(Sustain {
                    value: 0.0,
                    feedback,
                    invalid,
                }),
                false,
            )],
        )
        .unwrap()
    }
    #[test]
    fn quiet_hold_silence_reset_and_capped_tail_are_distinct() {
        let mut silent = sustain(0.9, false);
        silent.frame([0.0; 2], 0, 1);
        assert!(silent.drained(), "A silent path acquires no tail");
        let mut decay = sustain(0.9, false);
        decay.frame([1.0; 2], 0, 1);
        let mut tail = 0;
        while !decay.drained() {
            tail += 1;
            decay.frame([0.0; 2], tail, 1);
            assert!(tail < 1000);
        }
        assert!(tail >= 200 && !decay.tail_capped);
        decay.reset();
        assert!(decay.drained());
        let mut sustained = sustain(1.0, false);
        sustained.frame([1.0; 2], 0, 1);
        let mut last = [1.0; 2];
        for frame in 1..=60000 {
            last = sustained.frame([0.0; 2], frame, 1);
        }
        assert!(sustained.tail_capped);
        assert_eq!(last, [0.0; 2]);
    }
    #[test]
    fn invalid_output_and_resource_limits_are_explicit() {
        let mut bad = sustain(0.0, true);
        assert_eq!(bad.frame([1.0; 2], 0, 1), [0.0; 2]);
        assert!(bad.failed && bad.drained());
        let oversized = Chain::with_processors(
            &Processing::default(),
            48000,
            Channels::Mono,
            vec![(
                Box::new(Probe {
                    delay: DelayLine::new(0).unwrap(),
                    frames: 1 << 30,
                }),
                false,
            )],
        );
        assert!(matches!(
            oversized,
            Err(CommandError {
                code: ErrorCode::ResourceLimit,
                ..
            })
        ));
        assert!(MemoryLease::acquire(MAX_PREPARED_BYTES + 1).is_err());
    }
    struct Slew {
        gain: f64,
        target: f64,
        step: f64,
        remaining: u32,
    }
    impl Processor for Slew {
        fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            if self.remaining > 0 {
                self.gain += self.step;
                self.remaining -= 1;
                if self.remaining == 0 {
                    self.gain = self.target;
                }
            }
            [input[0] * self.gain as f32, input[1] * self.gain as f32]
        }
        fn reset(&mut self) {
            self.gain = self.target;
            self.remaining = 0;
        }
        fn can_retarget(&self, previous: &dyn Processor) -> bool {
            (previous as &dyn std::any::Any).is::<Self>()
        }
        fn retarget(&mut self, previous: &mut dyn Processor) {
            let old = (previous as &mut dyn std::any::Any)
                .downcast_mut::<Self>()
                .unwrap();
            self.gain = old.gain;
            self.remaining = 10;
            self.step = (self.target - self.gain) / 10.0;
        }
    }
    #[test]
    fn prepared_continuous_controls_preserve_state_and_slew_for_ten_milliseconds() {
        let instance = crate::processing::Instance {
            id: uuid::Uuid::new_v4().to_string(),
            kind: crate::processing::EffectKind::SoftClipper,
            bypassed: false,
            parameters: std::collections::BTreeMap::from([(
                "probe_gain".into(),
                crate::processing::ParameterValue::Number(1.0),
            )]),
        };
        let mut state = Processing {
            effects: vec![instance],
            ..Processing::default()
        };
        let make = |state: &Processing, gain: f64| {
            Chain::with_processors(
                state,
                1000,
                Channels::Stereo,
                vec![(
                    Box::new(Slew {
                        gain,
                        target: gain,
                        step: 0.0,
                        remaining: 0,
                    }),
                    false,
                )],
            )
            .unwrap()
        };
        let mut old = make(&state, 1.0);
        assert_eq!(old.frame([1.0; 2], 0, 1000), [1.0; 2]);
        state.effects[0].parameters.insert(
            "probe_gain".into(),
            crate::processing::ParameterValue::Number(0.5),
        );
        let mut next = make(&state, 0.5);
        assert!(next.can_retarget(&old));
        next.retarget(&mut old);
        let values: Vec<_> = (1..=10).map(|n| next.frame([1.0; 2], n, 1000)[0]).collect();
        assert!((values[0] - 0.95).abs() < 1e-6);
        assert_eq!(values[9], 0.5);
        next.reset();
        assert_eq!(next.frame([1.0; 2], 0, 1000), [0.5; 2]);
    }
    struct EchoProbe {
        delay: DelayLine,
    }
    impl Processor for EchoProbe {
        fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            self.delay.frame(input)
        }
        fn has_tail(&self) -> bool {
            true
        }
        fn stereo(&self) -> bool {
            true
        }
        fn activity(&self) -> f32 {
            self.delay
                .values
                .iter()
                .flatten()
                .map(|x| x.abs())
                .fold(0.0, f32::max)
        }
        fn reset(&mut self) {
            self.delay.reset();
        }
    }
    #[test]
    fn streaming_latency_is_compensated_once_and_loop_wrap_preserves_delay_state() {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let source = crate::audio::prepare(
            48000,
            1,
            "float32".into(),
            Default::default(),
            vec![1.0, 0.0, 0.0, 0.0],
            &cancel,
            |_| {},
        )
        .unwrap();
        let computational = Chain::with_processors(
            &Processing::default(),
            48000,
            Channels::Mono,
            vec![(
                Box::new(Probe {
                    delay: DelayLine::new(7).unwrap(),
                    frames: 7,
                }),
                false,
            )],
        )
        .unwrap();
        let mut voice = Voice::with_chain(computational, &source, 0, None, || false).unwrap();
        let output: Vec<_> = (0..4)
            .map(|_| {
                let frame = voice.values[0];
                voice.advance(&source, 1);
                frame[0]
            })
            .collect();
        assert_eq!(output, vec![1.0, 0.0, 0.0, 0.0]);
        let echo = || {
            Chain::with_processors(
                &Processing::default(),
                48000,
                Channels::Mono,
                vec![(
                    Box::new(EchoProbe {
                        delay: DelayLine::new(7).unwrap(),
                    }),
                    false,
                )],
            )
            .unwrap()
        };
        assert_eq!(echo().channels(), 2);
        let mut voice = Voice::with_chain(
            echo(),
            &source,
            0,
            Some(crate::documents::FrameRange { start: 0, end: 4 }),
            || false,
        )
        .unwrap();
        let output: Vec<_> = (0..12)
            .map(|_| {
                let frame = voice.values[0];
                voice.advance(&source, 1);
                frame[0]
            })
            .collect();
        assert!(output[..7].iter().all(|x| *x == 0.0));
        assert_eq!(output[7], 1.0);
        assert_eq!(output[11], 1.0);
        let reset = Voice::with_chain(echo(), &source, 0, None, || false).unwrap();
        assert_eq!(reset.values[0], [0.0; 2]);
    }
    #[test]
    fn differently_delayed_paths_align_in_the_arrangement_crossfade() {
        use crate::arrangement::{Arrangement, Clip, Track};
        use std::sync::Arc;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut pcm = vec![0.0; 256];
        pcm[64] = 1.0;
        let source = crate::audio::prepare(
            48000,
            1,
            "float32".into(),
            Default::default(),
            pcm,
            &cancel,
            |_| {},
        )
        .unwrap();
        let make_chain = |delay| {
            Chain::with_processors(
                &Processing::default(),
                48000,
                Channels::Mono,
                vec![(
                    Box::new(Probe {
                        delay: DelayLine::new(delay).unwrap(),
                        frames: delay,
                    }),
                    false,
                )],
            )
            .unwrap()
        };
        let source = Arc::new(source);
        let clip = |order| Clip {
            id: uuid::Uuid::new_v4().to_string(),
            name: "latency probe".into(),
            document_id: uuid::Uuid::new_v4().to_string(),
            revision_id: uuid::Uuid::new_v4().to_string(),
            edit_revision_id: uuid::Uuid::new_v4().to_string(),
            insertion_order: order,
            processed_revision_id: None,
            effects: Vec::new(),
            source_frames: 256,
            source_start: 0,
            source_end: 256,
            source_rate: 48000,
            channels: 1,
            position: 0,
            frames: 256,
            processing: Processing::default(),
            linked: false,
            full_document: true,
        };
        let first = clip(0);
        let second = clip(1);
        let arrangement = Arrangement {
            next_clip_order: 2,
            tracks: vec![Track {
                id: uuid::Uuid::new_v4().to_string(),
                name: "aligned".into(),
                mute: false,
                solo: false,
                gain_db: 0.0,
                pan: 0.0,
                clips: vec![first.clone(), second.clone()],
            }],
            ..Arrangement::default()
        };
        let inputs = vec![
            crate::audio::mix::Input {
                clip: first,
                audio: source.clone(),
                range: crate::documents::FrameRange { start: 0, end: 256 },
            },
            crate::audio::mix::Input {
                clip: second,
                audio: source,
                range: crate::documents::FrameRange { start: 0, end: 256 },
            },
        ];
        let graph =
            crate::audio::mix::prepare(&arrangement, inputs, None, &cancel, |_| {}).unwrap();
        let gains = arrangement.crossfades().regions[0].gains(64);
        let expected = (gains[0] + gains[1]) * std::f64::consts::FRAC_1_SQRT_2;
        let mut stream = crate::audio::mix::Stream::with_test_chains(
            &graph,
            vec![make_chain(7), make_chain(19)],
        );
        for frame in 0..256 {
            if frame == 64 {
                assert!((stream.values[0][0] as f64 - expected).abs() < 1e-7);
            } else {
                assert_eq!(stream.values[0], [0.0; 2]);
            }
            stream.advance(&graph, 1);
        }
        assert_eq!(graph.frames, 256);
    }
}
