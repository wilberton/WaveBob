use super::*;
use crate::arrangement::*;
pub(super) struct PendingClip {
    base: String,
    clip: Clip,
    label: String,
}
impl Editor {
    pub(super) fn clip_audio(
        &self,
        id: &str,
    ) -> Result<(Clip, Arc<audio::Audio>, FrameRange), CommandError> {
        let (_, clip) = self.arrangement.clip(id).ok_or_else(|| not_found("clip"))?;
        if clip.linked {
            let doc = self.document(&clip.document_id)?;
            let current = if let Some(bounce) = &clip.bounce {
                let identity = doc
                    .processing
                    .identity(&doc.revision_id, self.arrangement.effect_tempo);
                bounce.render_identity
                    == crate::projects::checksum(&(identity, clip.source_start, clip.source_end))
                    && bounce.source_revision_id == doc.revision_id
            } else {
                doc.revision_id == clip.revision_id
                    && doc
                        .processing
                        .same_sound(&crate::processing::Processing::default())
            };
            if !current {
                return Err(CommandError::new(
                    ErrorCode::Busy,
                    "Linked source is pending; prepare the current mix before reading or rendering it",
                ));
            }
        }
        let (audio, range) = if let Some(bounce) = &clip.bounce {
            let audio = self
                .revisions
                .get(&bounce.audio_revision_id)
                .ok_or_else(|| not_found("bounce audio"))?
                .clone();
            let range = if clip.linked {
                FrameRange {
                    start: 0,
                    end: audio.frames(),
                }
            } else {
                FrameRange {
                    start: clip.source_start,
                    end: if clip.source_end == bounce.metadata.source_body_frames {
                        audio.frames()
                    } else {
                        clip.source_end
                    },
                }
            };
            (audio, range)
        } else if let Some(processed) = &clip.processed_revision_id {
            let audio = self
                .revisions
                .get(processed)
                .ok_or_else(|| not_found("clip audio"))?
                .clone();
            let frames = audio.frames();
            (
                audio,
                FrameRange {
                    start: 0,
                    end: frames,
                },
            )
        } else {
            (
                self.revisions[&clip.revision_id].clone(),
                FrameRange {
                    start: clip.source_start,
                    end: clip.source_end,
                },
            )
        };
        Ok((clip.clone(), audio, range))
    }
    pub(super) fn clip_samples(
        &self,
        params: ClipSamplesParams,
    ) -> Result<CommandResult, CommandError> {
        let (clip, audio, range) = self.clip_audio(&params.clip_id)?;
        let end = params
            .start_frame
            .checked_add(params.frames as u64)
            .ok_or_else(invalid_range)?;
        if !(1..=4096).contains(&params.frames) || end > range.end - range.start {
            return Err(invalid_range());
        }
        let channels = (0..audio.channels as usize)
            .map(|channel| {
                (range.start + params.start_frame..range.start + end)
                    .map(|frame| {
                        audio.samples[frame as usize * audio.channels as usize + channel] as f64
                    })
                    .collect()
            })
            .collect();
        Ok(CommandResult::Samples(SamplesData {
            document_id: clip.id,
            revision_id: clip.edit_revision_id,
            start_frame: params.start_frame,
            frames: params.frames,
            sample_rate: audio.rate,
            channels,
        }))
    }
    pub(super) fn trim_clip(
        &mut self,
        params: TrimClipParams,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        let (_, old) = self
            .arrangement
            .clip(&params.clip_id)
            .ok_or_else(|| not_found("clip"))?;
        if old.edit_revision_id != params.edit_revision_id {
            return Err(CommandError::new(ErrorCode::StaleRevision, "Clip changed"));
        }
        let clip = old.trimmed(
            params.source_start,
            params.source_end,
            self.arrangement.sample_rate,
        )?;
        let mut test = self.arrangement.clone();
        *find_clip(&mut test, &clip.id).unwrap() = clip.clone();
        test.validate()?;
        if clip.effects.is_empty() {
            let mut clip = clip;
            if clip.linked {
                clip.bounce = None;
                clip.tail_frames = 0;
                clip.tail_capped = false;
            }
            let before = self.arrangement.clone();
            if clip != *old {
                advance(revision)?;
                clip.edit_revision_id = uuid::Uuid::new_v4().to_string();
                *find_clip(&mut self.arrangement, &clip.id).unwrap() = clip.clone();
                self.arrangement.revision_id = uuid::Uuid::new_v4().to_string();
                self.arrangement.dirty = true;
                self.history
                    .push_arrangement(before, self.arrangement.clone(), "Trim clip");
            }
            let job = JobInfo {
                id: uuid::Uuid::new_v4().to_string(),
                kind: "Trim clip".into(),
                path: clip.name.clone(),
                status: JobStatus::Succeeded,
                progress: 100,
                result: None,
                error: None,
                output: Some(JobOutput::ClipEdit(Box::new(clip))),
            };
            self.jobs.insert(
                job.id.clone(),
                (job.clone(), Arc::new(AtomicBool::new(false))),
            );
            return Ok(CommandResult::Job(job.into()));
        }
        self.start_clip(clip, "Trim clip", None)
    }
    pub(super) fn process_clip(
        &mut self,
        params: ClipEffectParams,
    ) -> Result<CommandResult, CommandError> {
        params.effect.validate()?;
        let (_, old) = self
            .arrangement
            .clip(&params.clip_id)
            .ok_or_else(|| not_found("clip"))?;
        if old.edit_revision_id != params.edit_revision_id {
            return Err(CommandError::new(ErrorCode::StaleRevision, "Clip changed"));
        }
        if old.effects.len() >= 32 {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "A clip supports up to 32 processing steps",
            ));
        }
        let mut clip = old.clone();
        clip.effects.push(params.effect.clone());
        let label = format!("Clip {}", params.effect.label());
        self.start_clip(clip, &label, Some(params.effect))
    }
    fn start_clip(
        &mut self,
        clip: Clip,
        label: &str,
        append: Option<crate::effects::Effect>,
    ) -> Result<CommandResult, CommandError> {
        if self
            .jobs
            .values()
            .filter(|(job, _)| job.status == JobStatus::Running)
            .count()
            >= 2
        {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Two audio jobs are already running",
            ));
        }
        let (old, audio, range) = if append.is_some() {
            self.clip_audio(&clip.id)?
        } else {
            let (_, old) = self.arrangement.clip(&clip.id).unwrap();
            (
                old.clone(),
                self.revisions[&clip.revision_id].clone(),
                FrameRange {
                    start: clip.source_start,
                    end: clip.source_end,
                },
            )
        };
        if self.retained_bytes() + (range.end - range.start) * audio.channels as u64 * 8
            > 2 * 1024 * 1024 * 1024
        {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Clip processing would exceed the retained audio limit",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = JobInfo {
            id: id.clone(),
            kind: label.into(),
            path: clip.name.clone(),
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
            output: None,
        };
        self.pending_clips.insert(
            id.clone(),
            PendingClip {
                base: old.edit_revision_id,
                clip: clip.clone(),
                label: label.into(),
            },
        );
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(255);
            let progress = |percent| {
                if last.replace(percent) == percent {
                    return;
                }
                let _ = sender.send(Event::Progress(id.clone(), percent));
                if let Some(wake) = &wake {
                    wake();
                }
            };
            let result = (|| {
                let values = audio::resample::convert(
                    &audio,
                    &range,
                    audio.rate,
                    audio.channels,
                    &cancel,
                    |p| progress(p / 4),
                )?;
                let mut processed = audio::prepare(
                    audio.rate,
                    audio.channels,
                    "float32".into(),
                    Default::default(),
                    values,
                    &cancel,
                    |_| {},
                )?;
                let effects = append.map_or(clip.effects, |effect| vec![effect]);
                for (index, effect) in effects.iter().enumerate() {
                    let frames = processed.frames();
                    processed = crate::effects::process(
                        &processed,
                        FrameRange {
                            start: 0,
                            end: frames,
                        },
                        effect,
                        &cancel,
                        |p| {
                            progress(
                                25 + ((index * 75 + p as usize * 75 / 100) / effects.len()) as u8,
                            )
                        },
                    )?;
                }
                Ok(processed)
            })();
            let _ = sender.send(Event::ClipProcessed(id, result));
            if let Some(wake) = wake {
                wake();
            }
        });
        Ok(CommandResult::Job(job.into()))
    }
    pub(super) fn finish_clip(
        &mut self,
        id: String,
        result: Result<audio::Audio, CommandError>,
        revision: &mut u64,
    ) {
        let Some(pending) = self.pending_clips.remove(&id) else {
            return;
        };
        if self
            .jobs
            .get(&id)
            .is_none_or(|(job, _)| job.status != JobStatus::Running)
        {
            return;
        }
        let result = (|| {
            let (old, old_audio, old_range) = self.clip_audio(&pending.clip.id)?;
            if old.edit_revision_id != pending.base {
                return Err(CommandError::new(
                    ErrorCode::StaleRevision,
                    "Clip changed while processing",
                ));
            }
            let audio = result?;
            let mut clip = pending.clip;
            let same = old.source_start == clip.source_start
                && old.source_end == clip.source_end
                && old_audio.samples[old_range.start as usize * old_audio.channels as usize
                    ..old_range.end as usize * old_audio.channels as usize]
                    == *audio.samples;
            if same {
                return Ok(old);
            }
            if self.retained_bytes() + audio.samples.len() as u64 * 4 > 2 * 1024 * 1024 * 1024 {
                return Err(CommandError::new(
                    ErrorCode::ResourceLimit,
                    "Clip result exceeds retained audio limit",
                ));
            }
            clip.edit_revision_id = uuid::Uuid::new_v4().to_string();
            let audio_id = uuid::Uuid::new_v4().to_string();
            clip.processed_revision_id = Some(audio_id.clone());
            clip.bounce = None;
            clip.linked = false;
            clip.tail_frames = 0;
            clip.tail_capped = false;
            let mut next = self.arrangement.clone();
            *find_clip(&mut next, &clip.id).unwrap() = clip.clone();
            next.validate()?;
            advance(revision)?;
            next.revision_id = uuid::Uuid::new_v4().to_string();
            next.dirty = true;
            self.revisions.insert(audio_id, Arc::new(audio));
            self.history
                .push_arrangement(self.arrangement.clone(), next.clone(), &pending.label);
            self.arrangement = next;
            self.collect_revisions();
            Ok(clip)
        })();
        let job = &mut self.jobs.get_mut(&id).unwrap().0;
        match result {
            Ok(clip) => {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.output = Some(JobOutput::ClipEdit(Box::new(clip)));
            }
            Err(error) => {
                job.status = if error.code == ErrorCode::Cancelled {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Failed
                };
                job.error = Some(error);
            }
        }
    }
}
fn find_clip<'a>(arrangement: &'a mut Arrangement, id: &str) -> Option<&'a mut Clip> {
    arrangement
        .tracks
        .iter_mut()
        .flat_map(|track| &mut track.clips)
        .find(|clip| clip.id == id)
}
