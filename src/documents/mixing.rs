use super::*;
use crate::{audio::mix, mixing::*};
impl Editor {
    pub fn required_mix_identity(&self) -> String {
        self.audible_arrangement().revision_id
    }

    pub fn mix_state(&self, requested_playing: bool, peak: f64) -> MixState {
        let any_solo = self.arrangement.tracks.iter().any(|track| track.solo);
        let attempt = self
            .mix_attempt
            .as_ref()
            .filter(|id| self.mix_bases.get(*id) == Some(&self.audible_arrangement().revision_id))
            .and_then(|id| self.jobs.get(id).map(|(job, _)| job));
        MixState {
            required_render_identity: self.audible_arrangement().revision_id,
            prepared_render_identity: self.mix.as_ref().map(|g| g.revision_id.clone()),
            bounce_cache_bytes: self
                .mix
                .as_ref()
                .map_or(0, |g| crate::bounces::bytes(&g.bounces)),
            bounce_cache_entries: self.mix.as_ref().map_or(0, |g| g.bounces.len()),
            arrangement_id: self.arrangement.id.clone(),
            revision_id: self.arrangement.revision_id.clone(),
            prepared_revision_id: self
                .prepared_mix()
                .map(|_| self.arrangement.revision_id.clone()),
            pending_job_id: attempt
                .filter(|job| job.status == JobStatus::Running)
                .map(|job| job.id.clone()),
            preparation_error: attempt.and_then(|job| job.error.clone()),
            requested_playing,
            peak_level: peak,
            overloaded: peak > 1.0,
            master_gain_db: self.arrangement.master_gain_db,
            tracks: self
                .arrangement
                .tracks
                .iter()
                .map(|track| EffectiveTrack {
                    track_id: track.id.clone(),
                    audible: !track.mute && (!any_solo || track.solo),
                    gain_db: track.gain_db,
                    pan: track.pan,
                })
                .collect(),
        }
    }
    pub fn prepared_mix(&self) -> Option<Arc<mix::Graph>> {
        self.mix
            .as_ref()
            .filter(|graph| graph.revision_id == self.audible_arrangement().revision_id)
            .cloned()
    }
    pub fn ensure_mix(&mut self, explicit: bool) -> Result<JobInfo, CommandError> {
        if let Some(id) = &self.mix_attempt
            && let Some(base) = self.mix_bases.get(id)
            && base == &self.audible_arrangement().revision_id
        {
            let job = &self.jobs[id].0;
            if job.status == JobStatus::Running
                || (job.status == JobStatus::Succeeded && self.prepared_mix().is_some())
            {
                return Ok(job.clone());
            }
            if !explicit {
                return Err(job.error.clone().unwrap_or_else(|| {
                    CommandError::new(
                        ErrorCode::Cancelled,
                        "Mix preparation was cancelled; prepare again to retry",
                    )
                }));
            }
        }
        if let Some(id) = &self.mix_attempt
            && self.mix_bases.get(id) != Some(&self.audible_arrangement().revision_id)
            && let Some((_, cancel)) = self.jobs.get(id)
        {
            cancel.store(true, Ordering::Release);
        }
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
        let inputs = self.mix_inputs()?;
        let extra = inputs
            .iter()
            .filter(|input| input.audio.rate != self.arrangement.sample_rate)
            .map(|input| {
                input
                    .clip
                    .frames
                    .saturating_mul(input.audio.channels as u64)
                    .saturating_mul(4)
            })
            .sum::<u64>();
        if self.retained_bytes().saturating_add(extra) > 2 * 1024 * 1024 * 1024 {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Mix preparation would exceed retained audio limit",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = JobInfo {
            id: id.clone(),
            kind: "Prepare mix".into(),
            path: "Arrangement".into(),
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
            output: None,
        };
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        self.mix_attempt = Some(id.clone());
        self.mix_bases
            .insert(id.clone(), self.audible_arrangement().revision_id.clone());
        let arrangement = self.audible_arrangement();
        let previous = self.mix.clone();
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(255);
            let result = mix::prepare(&arrangement, inputs, previous, &cancel, |percent| {
                if last.replace(percent) != percent {
                    let _ = sender.send(Event::Progress(id.clone(), percent));
                    if let Some(wake) = &wake {
                        wake();
                    }
                }
            });
            let _ = sender.send(Event::MixPrepared(id, result));
            if let Some(wake) = wake {
                wake();
            }
        });
        Ok(job)
    }
    pub(super) fn mix_inputs(&self) -> Result<Vec<mix::Input>, CommandError> {
        self.audible_arrangement()
            .tracks
            .into_iter()
            .flat_map(|track| track.clips)
            .map(|clip| {
                if clip.linked {
                    let (doc, source) = self.playback_audio(&clip.document_id)?;
                    let audio = clip
                        .bounce
                        .as_ref()
                        .and_then(|b| self.revisions.get(&b.audio_revision_id))
                        .cloned()
                        .unwrap_or_else(|| source.clone());
                    let range = FrameRange {
                        start: 0,
                        end: audio.frames(),
                    };
                    Ok(mix::Input {
                        document: Some(crate::bounces::DocumentInput {
                            revision_id: doc.revision_id,
                            audio: source,
                            state: doc.processing,
                        }),
                        clip,
                        audio,
                        range,
                    })
                } else {
                    let (_, audio, range) = self.clip_audio(&clip.id)?;
                    Ok(mix::Input {
                        document: None,
                        clip,
                        audio,
                        range,
                    })
                }
            })
            .collect()
    }
    pub(super) fn export_mix(
        &mut self,
        params: crate::exports::MixExportParams,
    ) -> Result<CommandResult, CommandError> {
        if self.has_active_preview() {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Resolve the processing preview before reading or exporting a committed mix",
            ));
        }
        if params.revision_id != self.arrangement.revision_id {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Arrangement changed",
            ));
        }
        if params.start_frame >= params.end_frame
            || params.end_frame > crate::arrangement::MAX_TIMELINE_FRAME
        {
            return Err(invalid_range());
        }
        let rate = params
            .format
            .output_rate(self.arrangement.sample_rate, params.sample_rate)?;
        let output_frames = audio::resample::frame_count(
            params.end_frame - params.start_frame,
            self.arrangement.sample_rate,
            rate,
        );
        if output_frames > 128 * 1024 * 1024 / 2 {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Mix output exceeds the current 512 MiB PCM limit; export a shorter range",
            ));
        }
        let prepared = self.prepared_mix();
        let inputs = if prepared.is_some() {
            Vec::new()
        } else {
            self.mix_inputs()?
        };
        let extra = inputs
            .iter()
            .filter(|input| input.audio.rate != self.arrangement.sample_rate)
            .map(|input| {
                input
                    .clip
                    .frames
                    .saturating_mul(input.audio.channels as u64)
                    .saturating_mul(4)
            })
            .sum::<u64>();
        if self
            .retained_bytes()
            .saturating_add(extra)
            .saturating_add(output_frames * 8)
            > 2 * 1024 * 1024 * 1024
        {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Mix export would exceed the retained audio limit",
            ));
        }
        let arrangement = self.audible_arrangement();
        let previous = self.mix.clone();
        let overwrite = params.overwrite;
        let path = params.path.clone();
        let identity = super::saving::OutputIdentity {
            document_id: arrangement.id.clone(),
            base_revision: arrangement.revision_id.clone(),
            save: false,
            mix: true,
            render_identity: arrangement.revision_id.clone(),
            processing: crate::processing::Processing::default(),
            tempo: arrangement.effect_tempo,
        };
        let mut range = FrameRange {
            start: params.start_frame,
            end: params.end_frame,
        };
        let request = crate::exports::OutputRequest {
            path: params.path,
            revision_id: params.revision_id,
            format: params.format,
            rate,
            channels: params.channels,
            render_identity: arrangement.revision_id.clone(),
            render_metadata: None,
            overwrite,
            overload: params.overload,
            protected_source: None,
            protected_sources: self.documents.iter().map(|doc| doc.path.clone()).collect(),
            protected_directories: self.guarded_asset_roots(),
        };
        self.launch_output(
            identity,
            path,
            "Export mix",
            overwrite,
            move |cancel, progress| {
                let graph = match prepared {
                    Some(graph) => graph,
                    None => Arc::new(mix::prepare(
                        &arrangement,
                        inputs,
                        previous,
                        cancel,
                        |value| progress(value / 5),
                    )?),
                };
                if params.include_tail {
                    range.end = range.end.max(graph.frames);
                }
                crate::exports::prepare_mix(&graph, range, request, cancel, |value| {
                    progress(20 + (value as u16 * 3 / 4) as u8)
                })
            },
        )
    }
    pub(super) fn finish_mix(&mut self, id: String, result: Result<mix::Graph, CommandError>) {
        if self
            .jobs
            .get(&id)
            .is_none_or(|(job, _)| job.status != JobStatus::Running)
        {
            return;
        }
        let result = result.and_then(|graph| {
            crate::exports::check_cancel(&self.jobs[&id].1)?;
            if graph.revision_id != self.audible_arrangement().revision_id {
                return Err(CommandError::new(
                    ErrorCode::StaleRevision,
                    "Arrangement changed during mix preparation",
                ));
            }
            if self.retained_with_graph(Some(&graph)) > 2 * 1024 * 1024 * 1024 {
                return Err(crate::rendering::limit(
                    "Linked refresh exceeds the 2 GiB retained audio limit",
                ));
            }
            for asset in graph.bounces.values() {
                self.revisions.insert(
                    asset.reference.audio_revision_id.clone(),
                    asset.audio.clone(),
                );
            }
            for rendered in &graph.refreshed_clips {
                let Some(clip) = self
                    .arrangement
                    .tracks
                    .iter_mut()
                    .flat_map(|t| &mut t.clips)
                    .find(|c| c.id == rendered.id)
                else {
                    continue;
                };
                clip.source_range_intent = rendered.source_range_intent.clone();
                clip.bounce = rendered.bounce.clone();
                clip.revision_id = rendered.revision_id.clone();
                clip.edit_revision_id = rendered.edit_revision_id.clone();
                clip.source_frames = rendered.source_frames;
                clip.source_start = rendered.source_start;
                clip.source_end = rendered.source_end;
                clip.source_rate = rendered.source_rate;
                clip.channels = rendered.channels;
                clip.frames = rendered.frames;
                clip.tail_frames = rendered.tail_frames;
                clip.tail_capped = rendered.tail_capped;
                clip.clamp_source_envelope(clip.source_end - clip.source_start);
            }
            let summary = MixPrepared {
                render_identity: graph.revision_id.clone(),
                revision_id: self.arrangement.revision_id.clone(),
                frames: graph.frames,
                sample_rate: graph.rate,
            };
            self.mix = Some(Arc::new(graph));
            self.collect_revisions();
            Ok(summary)
        });
        let job = &mut self.jobs.get_mut(&id).unwrap().0;
        match result {
            Ok(summary) => {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.output = Some(JobOutput::MixPrepared(summary));
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
    pub(super) fn mix_samples(
        &self,
        params: MixSamplesParams,
    ) -> Result<CommandResult, CommandError> {
        if self.has_active_preview() {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Resolve the processing preview before reading or exporting a committed mix",
            ));
        }
        if params.revision_id != self.arrangement.revision_id {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Arrangement changed",
            ));
        }
        let graph = self.prepared_mix().ok_or_else(|| {
            CommandError::new(
                ErrorCode::Busy,
                "Prepare the current mix before inspecting audio",
            )
        })?;
        let end = params
            .start_frame
            .checked_add(params.frames as u64)
            .ok_or_else(invalid_range)?;
        if !(1..=4096).contains(&params.frames) || end > crate::arrangement::MAX_TIMELINE_FRAME {
            return Err(invalid_range());
        }
        let mut channels = vec![
            Vec::with_capacity(params.frames as usize),
            Vec::with_capacity(params.frames as usize),
        ];
        let mut peak = 0.0f64;
        let mut overloaded = 0;
        let mut stream = mix::Stream::prepare(&graph, params.start_frame, None, || false)?;
        for _ in params.start_frame..end {
            let values = stream.values[0];
            stream.advance(&graph, 1);
            for channel in 0..2 {
                let value = values[channel] as f64;
                channels[channel].push(value);
                peak = peak.max(value.abs());
                overloaded += u64::from(value.abs() > 1.0);
            }
        }
        Ok(CommandResult::MixSamples(MixSamples {
            render_identity: graph.revision_id.clone(),
            revision_id: self.arrangement.revision_id.clone(),
            sample_rate: graph.rate,
            start_frame: params.start_frame,
            frames: params.frames,
            channels,
            peak_level: peak,
            overloaded_samples: overloaded,
        }))
    }
    pub fn mix_document(&self, looped: bool) -> DocumentInfo {
        let base = if looped {
            self.arrangement.frames()
        } else {
            self.prepared_mix()
                .map_or(self.arrangement.frames(), |g| g.frames)
        };
        let frames = base.max(if looped {
            self.arrangement
                .selection
                .as_ref()
                .map_or(0, |range| range.end)
        } else {
            0
        });
        DocumentInfo {
            processing: crate::processing::Processing::default(),
            saved_processing: crate::processing::Processing::default(),
            saved_effect_tempo: 120.0,
            saved_render_identity: None,
            save_channels: None,
            tab_open: true,
            id: self.arrangement.id.clone(),
            revision_id: self.audible_arrangement().revision_id,
            path: String::new(),
            name: "Arrangement".into(),
            sample_rate: self.arrangement.sample_rate,
            channels: 2,
            frames,
            encoding: "float32".into(),
            source_details: Default::default(),
            dirty: self.arrangement.dirty,
            cursor: self.arrangement.cursor,
            selection: self.arrangement.selection.clone(),
            peak_level: 0.0,
            overloaded_samples: 0,
            saved_revision_id: self.arrangement.saved_revision_id.clone(),
            save_format: None,
            save_sample_rate: None,
        }
    }
}
