use super::*;
use cpal::{
    FromSample, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{sync::mpsc, thread, time::Duration};

pub(super) struct Native {
    pub updates: mpsc::Receiver<OutputInfo>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Native {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Playback {
    pub fn native(wake: Option<crate::documents::Wake>) -> Self {
        let (mut playback, renderer) = Self::pair(48000);
        playback.state.output = OutputInfo {
            kind: OutputKind::Device,
            status: OutputStatus::Initializing,
            ..Default::default()
        };
        let (sender, updates) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let shared = playback.shared.clone();
        let worker=thread::Builder::new().name("wavebob-audio-device".into()).spawn(move|| {
            let notify=|info| {let _=sender.send(info);if let Some(wake)=&wake {wake();}};
            match open(renderer,shared.clone()) {
                Ok((stream,info))=>{
                    notify(info);
                    while !worker_stop.load(Ordering::Acquire) && !shared.failed.load(Ordering::Acquire) {thread::sleep(Duration::from_millis(20));}
                    if shared.failed.load(Ordering::Acquire) {notify(OutputInfo{kind:OutputKind::Device,status:OutputStatus::Failed,error:Some("Audio device disconnected or failed; reconnect it and retry output".into()),..Default::default()});}
                    drop(stream);
                }
                Err(message)=>notify(OutputInfo{kind:OutputKind::Device,status:OutputStatus::Failed,error:Some(message),..Default::default()}),
            }
        });
        match worker {
            Ok(worker) => {
                playback.native = Some(Native {
                    updates,
                    stop,
                    worker: Some(worker),
                })
            }
            Err(error) => {
                playback.state.output.status = OutputStatus::Failed;
                playback.state.output.error = Some(format!("Start audio device worker: {error}"));
            }
        }
        playback
    }
}

fn open(mut renderer: Renderer, shared: Arc<Shared>) -> Result<(cpal::Stream, OutputInfo), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No default audio output device; connect a device and retry output")?;
    let supported = device
        .default_output_config()
        .map_err(|error| format!("Query audio output configuration: {error}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    if config.channels == 0 || config.sample_rate == 0 {
        return Err("The default audio output configuration is empty".into());
    }
    renderer.rate = config.sample_rate;
    let info = OutputInfo {
        kind: OutputKind::Device,
        status: OutputStatus::Ready,
        name: Some(device.to_string()),
        sample_rate: Some(config.sample_rate),
        channels: Some(config.channels),
        error: None,
    };
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, config, renderer, shared),
        cpal::SampleFormat::F64 => build::<f64>(&device, config, renderer, shared),
        cpal::SampleFormat::I8 => build::<i8>(&device, config, renderer, shared),
        cpal::SampleFormat::I16 => build::<i16>(&device, config, renderer, shared),
        cpal::SampleFormat::I24 => build::<cpal::I24>(&device, config, renderer, shared),
        cpal::SampleFormat::I32 => build::<i32>(&device, config, renderer, shared),
        cpal::SampleFormat::I64 => build::<i64>(&device, config, renderer, shared),
        cpal::SampleFormat::U8 => build::<u8>(&device, config, renderer, shared),
        cpal::SampleFormat::U16 => build::<u16>(&device, config, renderer, shared),
        cpal::SampleFormat::U32 => build::<u32>(&device, config, renderer, shared),
        cpal::SampleFormat::U64 => build::<u64>(&device, config, renderer, shared),
        _ => return Err(format!("Unsupported device sample format: {format}")),
    }
    .map_err(|error| format!("Build audio output stream: {error}"))?;
    stream
        .play()
        .map_err(|error| format!("Start audio output stream: {error}"))?;
    Ok((stream, info))
}
fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut renderer: Renderer,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, cpal::Error> {
    let channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            renderer.begin();
            for frame in data.chunks_mut(channels) {
                let stereo = renderer.next();
                for (channel, value) in frame.iter_mut().enumerate() {
                    let sample = if channels == 1 {
                        ((stereo[0] as f64 + stereo[1] as f64) * 0.5) as f32
                    } else if channel < 2 {
                        stereo[channel]
                    } else {
                        0.0
                    };
                    *value = T::from_sample(sample.clamp(-1.0, 1.0));
                }
            }
            renderer.end();
        },
        move |error| {
            if !matches!(
                error.kind(),
                cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied
            ) {
                shared.failed.store(true, Ordering::Release);
            }
        },
        None,
    )
}
