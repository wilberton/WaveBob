//! Single coalescing preparation worker. Its locks never enter the audio callback.
use super::*;
use std::sync::{Condvar, Mutex};

struct Mailbox {
    task: Option<Box<Plan>>,
    result: Option<Result<Box<Plan>, (u64, CommandError)>>,
    stop: bool,
}
pub(super) struct Worker {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    latest: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    pub fn new(clock: Arc<Shared>) -> Self {
        let mailbox = Arc::new((
            Mutex::new(Mailbox {
                task: None,
                result: None,
                stop: false,
            }),
            Condvar::new(),
        ));
        let latest = Arc::new(AtomicU64::new(0));
        let shared = mailbox.clone();
        let generation = latest.clone();
        let thread = std::thread::Builder::new()
            .name("wavebob-dsp-prepare".into())
            .spawn(move || {
                loop {
                    let mut locked = shared.0.lock().unwrap();
                    while locked.task.is_none() && !locked.stop {
                        locked = shared.1.wait(locked).unwrap();
                    }
                    if locked.stop {
                        break;
                    }
                    let mut plan = locked.task.take().unwrap();
                    drop(locked);
                    let id = plan.generation;
                    let result = prepare_voice(&mut plan, || {
                        generation.load(Ordering::Acquire) != id
                    })
                    .and_then(|_| {
                        // Catch up to a moving transport off the callback. Successive
                        // corrections converge for a processor faster than real time.
                        for _ in 0..4 {
                            if plan.seek.is_some() || !clock.playing.load(Ordering::Acquire) {
                                break;
                            }
                            let Some(source) = &plan.source else {
                                break;
                            };
                            let rate = source.rate();
                            let position = clock.frame.load(Ordering::Acquire);
                            let delta = if position >= plan.prepared_position {
                                position - plan.prepared_position
                            } else if let Some(bounds) = &plan.bounds {
                                bounds.end.saturating_sub(plan.prepared_position)
                                    + position.saturating_sub(bounds.start)
                            } else {
                                0
                            };
                            if delta > rate as u64 * crate::rendering::TAIL_SECONDS {
                                return Err(CommandError::new(
                                    ErrorCode::ResourceLimit,
                                    "Preparation could not catch up within 60 seconds of context",
                                ));
                            }
                            for offset in 0..delta {
                                if offset.is_multiple_of(1024)
                                    && generation.load(Ordering::Acquire) != id
                                {
                                    return Err(CommandError::new(
                                        ErrorCode::Cancelled,
                                        "Preparation superseded",
                                    ));
                                }
                                match source {
                                    Source::Processed { audio, .. } => {
                                        if let Some(voice) = &mut plan.voice {
                                            voice.advance(audio, 1);
                                        }
                                    }
                                    Source::Mix { graph, .. } => {
                                        if let Some(voice) = &mut plan.mix_voice {
                                            voice.advance(graph, 1);
                                        }
                                    }
                                    _ => break,
                                }
                            }
                            plan.prepared_position = position;
                            if delta < 256 {
                                break;
                            }
                        }
                        Ok(())
                    });
                    if generation.load(Ordering::Acquire) != id {
                        continue;
                    }
                    let result = result.map(|_| plan).map_err(|error| (id, error));
                    let mut locked = shared.0.lock().unwrap();
                    if !locked.stop && generation.load(Ordering::Acquire) == id {
                        locked.result = Some(result);
                    }
                }
            })
            .ok();
        Self {
            mailbox,
            latest,
            thread,
        }
    }
    pub fn submit(&self, plan: Box<Plan>) -> Result<(), CommandError> {
        if self.thread.is_none() {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Cannot start DSP preparation worker",
            ));
        }
        self.latest.store(plan.generation, Ordering::Release);
        self.mailbox.0.lock().unwrap().task = Some(plan);
        self.mailbox.1.notify_one();
        Ok(())
    }
    pub fn take(&self) -> Option<Result<Box<Plan>, (u64, CommandError)>> {
        self.mailbox.0.lock().unwrap().result.take()
    }
    pub fn supersede(&self, generation: u64) {
        self.latest.store(generation, Ordering::Release);
        self.mailbox.0.lock().unwrap().task = None;
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.latest.fetch_add(1, Ordering::Release);
        self.mailbox.0.lock().unwrap().stop = true;
        self.mailbox.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub(super) fn prepare_voice(
    plan: &mut Plan,
    cancelled: impl Fn() -> bool,
) -> Result<(), CommandError> {
    if let Some(Source::Processed {
        audio,
        state,
        tempo,
    }) = &plan.source
    {
        let mut position = plan.prepared_position.min(audio.frames());
        if let Some(bounds) = &plan.bounds
            && (position < bounds.start || position >= bounds.end)
        {
            position = bounds.start;
        }
        plan.voice = Some(crate::rendering::Voice::prepare(
            state,
            audio,
            *tempo,
            position,
            plan.bounds.clone(),
            &cancelled,
        )?);
        plan.prepared_position = position;
    }
    if let Some(Source::Mix { graph, .. }) = &plan.source {
        let mut position = plan.prepared_position;
        if let Some(bounds) = &plan.bounds
            && (position < bounds.start || position >= bounds.end)
        {
            position = bounds.start;
        }
        plan.mix_voice = Some(crate::audio::mix::Stream::prepare(
            graph,
            position,
            plan.bounds.clone(),
            cancelled,
        )?);
        plan.prepared_position = position;
    }
    Ok(())
}
