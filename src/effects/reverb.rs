//! A bounded Schroeder network. Every offline target starts with silent state.
use super::*;
struct Delay {
    values: Vec<f64>,
    cursor: usize,
    feedback: f64,
}
impl Delay {
    fn new(seconds: f64, rate: u32, feedback: f64) -> Self {
        Self {
            values: vec![0.0; (seconds * rate as f64).round().max(1.0) as usize],
            cursor: 0,
            feedback,
        }
    }
    fn comb(&mut self, input: f64) -> f64 {
        let output = self.values[self.cursor];
        self.values[self.cursor] = input + output * self.feedback;
        self.cursor = (self.cursor + 1) % self.values.len();
        output
    }
    fn allpass(&mut self, input: f64) -> f64 {
        let output = self.values[self.cursor] - input * self.feedback;
        self.values[self.cursor] = input + output * self.feedback;
        self.cursor = (self.cursor + 1) % self.values.len();
        output
    }
}
struct Network {
    combs: Vec<Delay>,
    diffusers: Vec<Delay>,
}
pub(super) fn apply(
    samples: &mut [f32],
    rate: u32,
    channels: u16,
    (room, decay, wet): (f64, f64, f64),
    cancel: &AtomicBool,
    progress: &dyn Fn(u8),
) -> Result<(), CommandError> {
    if wet == 0.0 {
        return Ok(());
    }
    let scale = 0.5 + room;
    let mut networks = (0..channels)
        .map(|channel| Network {
            combs: [0.0297, 0.0371, 0.0411, 0.0437]
                .into_iter()
                .map(|base| {
                    let seconds = base * scale + channel as f64 * 0.0007;
                    Delay::new(seconds, rate, 10.0_f64.powf(-3.0 * seconds / decay))
                })
                .collect(),
            diffusers: [0.005, 0.0017]
                .into_iter()
                .map(|base| Delay::new(base * scale, rate, 0.5))
                .collect(),
        })
        .collect::<Vec<_>>();
    let frames = samples.len() / channels as usize;
    for (frame, values) in samples.chunks_mut(channels as usize).enumerate() {
        if frame.is_multiple_of(4096) {
            if cancel.load(Ordering::Acquire) {
                return Err(CommandError::new(ErrorCode::Cancelled, "Reverb cancelled"));
            }
            progress((frame * 89 / frames.max(1)) as u8);
        }
        for (sample, network) in values.iter_mut().zip(&mut networks) {
            let input = *sample as f64;
            let mut reverberated = network
                .combs
                .iter_mut()
                .map(|comb| comb.comb(input))
                .sum::<f64>()
                * 0.25;
            for diffuser in &mut network.diffusers {
                reverberated = diffuser.allpass(reverberated);
            }
            let output = input * (1.0 - wet) + reverberated * wet;
            if !output.is_finite() || output.abs() > f32::MAX as f64 {
                return Err(invalid(
                    "Reverb exceeds the finite floating-point audio range",
                ));
            }
            *sample = output as f32;
        }
    }
    Ok(())
}
