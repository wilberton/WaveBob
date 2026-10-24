//! Atomic edit publication and chronological history, shared by all adapters.
use super::*;
use crate::effects::{self, ApplyEffectParams, HistoryInfo};

pub(super) struct PendingEdit {
    document_id: String,
    base_revision: String,
    label: String,
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
        let (document, audio) = self.audio(id, Some(base))?;
        let range = params.target.range(audio.frames())?;
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
        let allocated = self
            .revisions
            .values()
            .map(|audio| audio.samples.len() as u64 * 4)
            .sum::<u64>();
        if allocated + audio.samples.len() as u64 * 4 > 2 * 1024 * 1024 * 1024 {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Edit would exceed the current 2 GiB retained audio limit",
            ));
        }
        let audio = self.revisions[&document.revision_id].clone();
        let id = uuid::Uuid::new_v4().to_string();
        let job = JobInfo {
            id: id.clone(),
            kind: params.effect.label().into(),
            path: document.path.clone(),
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
        };
        let pending = PendingEdit {
            document_id: document.id.clone(),
            base_revision: document.revision_id.clone(),
            label: params.effect.label().into(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending_edits.insert(id.clone(), pending);
        self.jobs.insert(id.clone(), (job.clone(), cancel.clone()));
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(255);
            let result = effects::process(&audio, range, &params.effect, &cancel, |progress| {
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
                return Ok(before);
            };
            let allocated = self
                .revisions
                .values()
                .map(|audio| audio.samples.len() as u64 * 4)
                .sum::<u64>();
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
        let restored = if redo { &change.after } else { &change.before };
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
