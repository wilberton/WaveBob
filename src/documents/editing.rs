//! Atomic edit publication and chronological history, shared by all adapters.
use super::*;
use crate::effects::{self, ApplyEffectParams, HistoryInfo};

pub(super) struct PendingEdit {
    document_id: String,
    base_revision: String,
    label: String,
    navigation: Navigation,
    clipboard: Option<Clipboard>,
}
#[derive(Clone)]
pub(super) struct Clipboard {
    audio: Arc<audio::Audio>,
    range: FrameRange,
    revision_id: String,
}
enum Navigation {
    Keep,
    Delete(FrameRange),
    Trim(FrameRange),
    Paste { start: u64, frames: u64 },
}
impl Navigation {
    fn apply(&self, doc: &mut DocumentInfo) {
        match self {
            Self::Keep => {}
            Self::Delete(range) => {
                let map = |frame: u64| {
                    if frame < range.start {
                        frame
                    } else if frame >= range.end {
                        frame - (range.end - range.start)
                    } else {
                        range.start
                    }
                };
                doc.cursor = map(doc.cursor);
                doc.selection = doc.selection.as_ref().and_then(|selection| {
                    let start = map(selection.start);
                    let end = map(selection.end);
                    (start < end).then_some(FrameRange { start, end })
                });
            }
            Self::Trim(range) => {
                let map = |frame: u64| frame.clamp(range.start, range.end) - range.start;
                doc.cursor = map(doc.cursor);
                doc.selection = doc.selection.as_ref().and_then(|selection| {
                    let start = map(selection.start);
                    let end = map(selection.end);
                    (start < end).then_some(FrameRange { start, end })
                });
            }
            Self::Paste { start, frames } => {
                doc.cursor = start + frames;
                doc.selection = Some(FrameRange {
                    start: *start,
                    end: start + frames,
                });
            }
        }
        doc.cursor = doc.cursor.min(doc.frames);
    }
}
#[derive(Clone)]
struct Change {
    before: DocumentInfo,
    after: DocumentInfo,
    label: String,
}
#[derive(Default)]
pub(super) struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
}
impl History {
    pub fn snapshot(&self) -> HistoryInfo {
        HistoryInfo {
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            undo_label: self.undo.last().map(|change| change.label.clone()),
            redo_label: self.redo.last().map(|change| change.label.clone()),
            undo_count: self.undo.len(),
            redo_count: self.redo.len(),
        }
    }
    pub fn remove_document(&mut self, id: &str) {
        self.undo.retain(|change| change.before.id != id);
        self.redo.retain(|change| change.before.id != id);
    }
}
impl Editor {
    pub(super) fn retained_bytes(&self) -> u64 {
        let mut seen = std::collections::BTreeSet::new();
        self.revisions
            .values()
            .chain(self.clipboard.iter().map(|clipboard| &clipboard.audio))
            .filter(|audio| seen.insert(Arc::as_ptr(audio)))
            .map(|audio| audio.samples.len() as u64 * 4)
            .sum()
    }
    pub fn history_snapshot(&self) -> HistoryInfo {
        self.history.snapshot()
    }
    pub(super) fn collect_revisions(&mut self) {
        let mut retained = std::collections::BTreeSet::new();
        for doc in &self.documents {
            retained.insert(doc.revision_id.clone());
        }
        for change in self.history.undo.iter().chain(&self.history.redo) {
            retained.insert(change.before.revision_id.clone());
            retained.insert(change.after.revision_id.clone());
        }
        self.revisions.retain(|id, _| retained.contains(id));
    }
    pub(super) fn apply_effect(
        &mut self,
        params: ApplyEffectParams,
    ) -> Result<CommandResult, CommandError> {
        params.effect.validate()?;
        let (id, base) = params.target.ids();
        let (document, audio) = self.playback_audio(id)?;
        if document.revision_id != base {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Audio revision changed",
            ));
        }
        let range = params.target.range(audio.frames())?;
        let label = params.effect.label().to_string();
        self.start_edit(
            document,
            audio,
            label,
            Navigation::Keep,
            None,
            move |audio, cancel, progress| {
                effects::process(audio, range, &params.effect, cancel, progress)
            },
        )
    }
    fn start_edit(
        &mut self,
        document: DocumentInfo,
        audio: Arc<audio::Audio>,
        label: String,
        navigation: Navigation,
        clipboard: Option<Clipboard>,
        process: impl FnOnce(
            &audio::Audio,
            &AtomicBool,
            &dyn Fn(u8),
        ) -> Result<audio::Audio, CommandError>
        + Send
        + 'static,
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
                "Two audio jobs are already running; wait or cancel one",
            ));
        }
        let allocated = self.retained_bytes();
        if allocated + audio.samples.len() as u64 * 4 > 2 * 1024 * 1024 * 1024 {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Edit would exceed the current 2 GiB retained audio limit",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let job = JobInfo {
            id: id.clone(),
            kind: label.clone(),
            path: document.path.clone(),
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
            output: None,
        };
        let pending = PendingEdit {
            document_id: document.id,
            base_revision: document.revision_id,
            label,
            navigation,
            clipboard,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending_edits.insert(id.clone(), pending);
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(255);
            let result = process(&audio, &cancel, &|progress| {
                if last.replace(progress) != progress {
                    let _ = sender.send(Event::Progress(id.clone(), progress));
                    if let Some(wake) = &wake {
                        wake();
                    }
                }
            })
            .map(|result| {
                if result.samples == audio.samples {
                    None
                } else {
                    Some(result)
                }
            });
            let _ = sender.send(Event::Edited(id, result));
            if let Some(wake) = wake {
                wake();
            }
        });
        Ok(CommandResult::Job(job.into()))
    }
    pub fn clipboard_info(&self) -> crate::edits::ClipboardInfo {
        self.clipboard
            .as_ref()
            .map_or_else(Default::default, |clipboard| crate::edits::ClipboardInfo {
                frames: clipboard.range.end - clipboard.range.start,
                sample_rate: Some(clipboard.audio.rate),
                channels: Some(clipboard.audio.channels),
                source_revision_id: Some(clipboard.revision_id.clone()),
            })
    }
    pub(super) fn copy_audio(
        &mut self,
        params: crate::edits::EditParams,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        let (id, base) = params.target.ids();
        let (document, audio) = self.playback_audio(id)?;
        if document.revision_id != base {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Audio revision changed",
            ));
        }
        let range = params.target.range(audio.frames())?;
        if self
            .clipboard
            .as_ref()
            .is_none_or(|old| old.revision_id != base || old.range != range)
        {
            advance(revision)?;
            self.clipboard = Some(Clipboard {
                audio,
                range,
                revision_id: base.into(),
            });
        }
        Ok(CommandResult::Clipboard(self.clipboard_info()))
    }
    pub(super) fn structural_edit(
        &mut self,
        params: crate::edits::EditParams,
        label: &'static str,
    ) -> Result<CommandResult, CommandError> {
        let (id, base) = params.target.ids();
        let (document, audio) = self.playback_audio(id)?;
        if document.revision_id != base {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Audio revision changed",
            ));
        }
        let range = params.target.range(audio.frames())?;
        let clipboard = (label == "Cut").then(|| Clipboard {
            audio: audio.clone(),
            range: range.clone(),
            revision_id: base.into(),
        });
        let navigation = if label == "Trim" {
            Navigation::Trim(range.clone())
        } else {
            Navigation::Delete(range.clone())
        };
        self.start_edit(
            document,
            audio,
            label.into(),
            navigation,
            clipboard,
            move |audio, cancel, progress| {
                let channels = audio.channels as usize;
                let mut samples = Vec::new();
                let intervals = if label == "Trim" {
                    [
                        range.start as usize * channels..range.end as usize * channels,
                        0..0,
                    ]
                } else {
                    [
                        0..range.start as usize * channels,
                        range.end as usize * channels..audio.samples.len(),
                    ]
                };
                let length = intervals.iter().map(|range| range.len()).sum();
                reserve(&mut samples, length)?;
                for range in intervals {
                    append(&mut samples, &audio.samples[range], cancel)?;
                }
                audio::prepare(
                    audio.rate,
                    audio.channels,
                    audio.encoding.clone(),
                    audio.source.clone(),
                    samples,
                    cancel,
                    progress,
                )
            },
        )
    }
    pub(super) fn paste_audio(
        &mut self,
        params: crate::edits::PasteParams,
    ) -> Result<CommandResult, CommandError> {
        let (document, audio) = self.playback_audio(&params.document_id)?;
        if document.revision_id != params.revision_id {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Audio revision changed",
            ));
        }
        if params.start_frame > params.end_frame || params.end_frame > audio.frames() {
            return Err(invalid_range());
        }
        let clipboard = self.clipboard.clone().ok_or_else(|| {
            CommandError::new(
                ErrorCode::InvalidParams,
                "The internal audio clipboard is empty",
            )
        })?;
        let frames = audio::resample::frame_count(
            clipboard.range.end - clipboard.range.start,
            clipboard.audio.rate,
            audio.rate,
        );
        self.start_edit(
            document,
            audio,
            "Paste".into(),
            Navigation::Paste {
                start: params.start_frame,
                frames,
            },
            None,
            move |audio, cancel, progress| {
                let converted = audio::resample::convert(
                    &clipboard.audio,
                    &clipboard.range,
                    audio.rate,
                    audio.channels,
                    cancel,
                    progress,
                )?;
                let channels = audio.channels as usize;
                let start = params.start_frame as usize * channels;
                let end = params.end_frame as usize * channels;
                let length = audio.samples.len() - (end - start) + converted.len();
                let mut samples = Vec::new();
                reserve(&mut samples, length)?;
                append(&mut samples, &audio.samples[..start], cancel)?;
                append(&mut samples, &converted, cancel)?;
                append(&mut samples, &audio.samples[end..], cancel)?;
                audio::prepare(
                    audio.rate,
                    audio.channels,
                    audio.encoding.clone(),
                    audio.source.clone(),
                    samples,
                    cancel,
                    progress,
                )
            },
        )
    }
    pub(super) fn finish_edit(
        &mut self,
        id: String,
        result: Result<Option<audio::Audio>, CommandError>,
        revision: &mut u64,
    ) {
        let Some(pending) = self.pending_edits.remove(&id) else {
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
            let before = self.document(&pending.document_id)?.clone();
            if before.revision_id != pending.base_revision {
                return Err(CommandError::new(
                    ErrorCode::StaleRevision,
                    "Audio changed while the edit was running",
                ));
            }
            let Some(audio) = result? else {
                let mut after = before.clone();
                pending.navigation.apply(&mut after);
                if after != before {
                    advance(revision)?;
                    *self
                        .documents
                        .iter_mut()
                        .find(|doc| doc.id == after.id)
                        .unwrap() = after.clone();
                }
                return Ok(after);
            };
            let allocated = self.retained_bytes();
            if allocated + audio.samples.len() as u64 * 4 > 2 * 1024 * 1024 * 1024 {
                return Err(CommandError::new(
                    ErrorCode::ResourceLimit,
                    "Edit result exceeds the current 2 GiB retained audio limit",
                ));
            }
            let mut after = before.clone();
            after.revision_id = uuid::Uuid::new_v4().to_string();
            after.dirty = true;
            after.frames = audio.frames();
            after.peak_level = audio.peak_level;
            after.overloaded_samples = audio.overloaded_samples;
            pending.navigation.apply(&mut after);
            advance(revision)?;
            self.revisions
                .insert(after.revision_id.clone(), Arc::new(audio));
            *self
                .documents
                .iter_mut()
                .find(|doc| doc.id == after.id)
                .unwrap() = after.clone();
            self.history.undo.push(Change {
                before,
                after: after.clone(),
                label: pending.label,
            });
            self.history.redo.clear();
            if let Some(clipboard) = pending.clipboard {
                self.clipboard = Some(clipboard);
            }
            self.collect_revisions();
            Ok(after)
        })();
        let job = &mut self.jobs.get_mut(&id).unwrap().0;
        match result {
            Ok(document) => {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.result = Some(document);
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
    pub(super) fn restore_history(
        &mut self,
        redo: bool,
        revision: &mut u64,
    ) -> Result<CommandResult, CommandError> {
        let stack = if redo {
            &mut self.history.redo
        } else {
            &mut self.history.undo
        };
        let change = stack
            .last()
            .ok_or_else(|| {
                CommandError::new(
                    ErrorCode::InvalidParams,
                    if redo {
                        "Nothing to redo"
                    } else {
                        "Nothing to undo"
                    },
                )
            })?
            .clone();
        advance(revision)?;
        stack.pop();
        let mut restored = if redo {
            change.after.clone()
        } else {
            change.before.clone()
        };
        let current = self.document(&restored.id)?;
        restored.path = current.path.clone();
        restored.name = current.name.clone();
        restored.saved_revision_id = current.saved_revision_id.clone();
        restored.save_format = current.save_format.clone();
        restored.dirty = restored.saved_revision_id.as_ref() != Some(&restored.revision_id);
        *self
            .documents
            .iter_mut()
            .find(|doc| doc.id == restored.id)
            .expect("closed document history is removed") = restored.clone();
        self.active = Some(restored.id.clone());
        if redo {
            self.history.undo.push(change);
        } else {
            self.history.redo.push(change);
        }
        Ok(CommandResult::History(self.history.snapshot()))
    }
}

fn reserve(samples: &mut Vec<f32>, length: usize) -> Result<(), CommandError> {
    if length > 128 * 1024 * 1024 {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Edited audio exceeds the current 512 MiB PCM limit",
        ));
    }
    samples
        .try_reserve_exact(length)
        .map_err(|_| CommandError::new(ErrorCode::ResourceLimit, "Unable to allocate edited audio"))
}
fn append(output: &mut Vec<f32>, source: &[f32], cancel: &AtomicBool) -> Result<(), CommandError> {
    for chunk in source.chunks(16384) {
        if cancel.load(Ordering::Acquire) {
            return Err(CommandError::new(ErrorCode::Cancelled, "Edit cancelled"));
        }
        output.extend_from_slice(chunk);
    }
    Ok(())
}
