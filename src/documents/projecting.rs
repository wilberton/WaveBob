use super::*;
use crate::projects::{self, ProjectInfo, StateKey, storage};
#[derive(Default)]
pub(super) struct ProjectState {
    path: Option<String>,
    saved_key: Option<StateKey>,
    saved_revision: Option<String>,
    protected_roots: std::collections::BTreeSet<String>,
}
enum Operation {
    Save { key: StateKey },
    Open { base: String },
}
pub(super) struct PendingProject {
    operation: Operation,
    pub committing: bool,
    error: Option<CommandError>,
    pub worker: Option<std::thread::JoinHandle<()>>,
}
impl Editor {
    pub(super) fn guarded_asset_roots(&self) -> Vec<String> {
        self.project.protected_roots.iter().cloned().collect()
    }
    fn project_key(&self) -> StateKey {
        StateKey::new(&self.documents, &self.arrangement)
    }
    pub fn project_info(&self) -> ProjectInfo {
        let revision_id = self.project_key().revision();
        ProjectInfo {
            path: self.project.path.clone(),
            dirty: if self.project.path.is_some() {
                self.project.saved_revision.as_ref() != Some(&revision_id)
            } else {
                self.arrangement.dirty || self.documents.iter().any(|doc| doc.dirty)
            },
            revision_id,
            saved_revision_id: self.project.saved_revision.clone(),
            saved_document_revisions: self
                .project
                .saved_key
                .iter()
                .flat_map(|key| &key.documents)
                .map(|doc| (doc.id.clone(), doc.revision_id.clone()))
                .collect(),
        }
    }
    pub(super) fn document_in_project(&self, document: &DocumentInfo) -> bool {
        self.project.saved_key.as_ref().is_some_and(|key| {
            key.documents.iter().any(|doc| {
                doc.id == document.id
                    && doc.revision_id == document.revision_id
                    && doc.processing.same_sound(&document.processing)
            })
        })
    }
    pub fn take_project_loop(&mut self) -> Option<bool> {
        self.loaded_loop.take()
    }
    fn start_project_job(
        &mut self,
        path: String,
        label: &str,
        operation: Operation,
    ) -> Result<(JobInfo, Arc<AtomicBool>), CommandError> {
        if path.trim().is_empty() {
            return Err(CommandError::new(
                ErrorCode::InvalidParams,
                "Choose a project path",
            ));
        }
        if self
            .jobs
            .values()
            .filter(|(job, _)| job.status == JobStatus::Running)
            .count()
            >= 2
            || self
                .pending_projects
                .values()
                .any(|pending| matches!(pending.operation, Operation::Save { .. }))
        {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Audio jobs or a project save are already running",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = JobInfo {
            id: id.clone(),
            kind: label.into(),
            path,
            status: JobStatus::Running,
            progress: 0,
            result: None,
            error: None,
            output: None,
        };
        self.pending_projects.insert(
            id.clone(),
            PendingProject {
                operation,
                committing: false,
                error: None,
                worker: None,
            },
        );
        self.jobs.insert(id, (job.clone(), cancel.clone()));
        Ok((job, cancel))
    }
    pub fn save_project(
        &mut self,
        params: projects::SaveProjectParams,
        loop_enabled: bool,
    ) -> Result<CommandResult, CommandError> {
        if self.has_active_preview() {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Commit or cancel processing previews before saving a project",
            ));
        }
        let key = self.project_key();
        let revision_id = key.revision();
        if params
            .expected_project_revision
            .as_ref()
            .is_some_and(|expected| expected != &revision_id)
        {
            return Err(CommandError::new(
                ErrorCode::StaleRevision,
                "Project changed since the expected revision",
            ));
        }
        let required = self
            .documents
            .iter()
            .map(|doc| doc.revision_id.clone())
            .chain(self.arrangement.retained_revisions().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        let revisions = required
            .into_iter()
            .map(|id| {
                self.revisions
                    .get(&id)
                    .cloned()
                    .map(|audio| (id, audio))
                    .ok_or_else(|| not_found("audio revision"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let snapshot = storage::SaveSnapshot {
            payload: projects::Payload {
                loop_enabled,
                documents: self.documents.clone(),
                active_document_id: self.active.clone(),
                arrangement: self.arrangement.clone(),
                assets: Vec::new(),
            },
            revisions,
            revision_id,
        };
        let (job, cancel) =
            self.start_project_job(params.path.clone(), "Save project", Operation::Save { key })?;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        let id = job.id.clone();
        let worker_id = id.clone();
        let worker = std::thread::spawn(move || {
            let notify = || {
                if let Some(wake) = &wake {
                    wake();
                }
            };
            let last = std::cell::Cell::new(255);
            let result = storage::prepare_save(
                snapshot,
                &params.path,
                params.overwrite,
                &cancel,
                |percent| {
                    if last.replace(percent) != percent {
                        let _ = sender.send(Event::Progress(id.clone(), percent));
                        notify();
                    }
                },
            )
            .and_then(|prepared| {
                let (permit, approved) = mpsc::sync_channel(1);
                sender
                    .send(Event::ProjectSavePrepared(id.clone(), permit))
                    .map_err(|_| {
                        CommandError::new(ErrorCode::Cancelled, "Project session closed")
                    })?;
                notify();
                if !approved.recv().unwrap_or(false) {
                    return Err(CommandError::new(
                        ErrorCode::Cancelled,
                        "Project save publication cancelled",
                    ));
                }
                prepared.publish(params.overwrite)
            });
            let _ = sender.send(Event::ProjectSaved(id, result));
            notify();
        });
        self.pending_projects.get_mut(&worker_id).unwrap().worker = Some(worker);
        Ok(CommandResult::Job(job.into()))
    }
    pub fn open_project(
        &mut self,
        params: projects::OpenProjectParams,
    ) -> Result<CommandResult, CommandError> {
        if !params.discard_changes && self.project_info().dirty {
            return Err(CommandError::new(
                ErrorCode::DirtyDocument,
                "Save the project or explicitly discard current work before opening another project",
            ));
        }
        if !self.pending_outputs.is_empty()
            || !self.pending_projects.is_empty()
            || self
                .jobs
                .values()
                .any(|(job, _)| job.status == JobStatus::Running)
        {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Wait for existing jobs and output publication before opening a project",
            ));
        }
        let available = (2 * 1024 * 1024 * 1024u64).saturating_sub(self.retained_bytes());
        let (job, cancel) = self.start_project_job(
            params.path.clone(),
            "Open project",
            Operation::Open {
                base: self.project_key().revision(),
            },
        )?;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        let id = job.id.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(255);
            let result = storage::load(&params.path, available, &cancel, |percent| {
                if last.replace(percent) != percent {
                    let _ = sender.send(Event::Progress(id.clone(), percent));
                    if let Some(wake) = &wake {
                        wake();
                    }
                }
            });
            let _ = sender.send(Event::ProjectLoaded(id, result.map(Box::new)));
            if let Some(wake) = wake {
                wake();
            }
        });
        Ok(CommandResult::Job(job.into()))
    }
    pub(super) fn approve_project_save(
        &mut self,
        id: String,
        permit: mpsc::SyncSender<bool>,
        revision: u64,
    ) {
        let running = self
            .jobs
            .get(&id)
            .is_some_and(|(job, _)| job.status == JobStatus::Running);
        let Some(pending) = self.pending_projects.get_mut(&id) else {
            return;
        };
        pending.error = if !running {
            Some(CommandError::new(
                ErrorCode::Cancelled,
                "Project save cancelled before publication",
            ))
        } else if revision == u64::MAX {
            Some(CommandError::new(
                ErrorCode::RevisionExhausted,
                "Session revision exhausted before project save publication",
            ))
        } else {
            None
        };
        pending.committing = pending.error.is_none();
        let _ = permit.send(pending.committing);
    }
    pub(super) fn finish_project_save(
        &mut self,
        id: String,
        result: Result<projects::ProjectSummary, CommandError>,
        revision: &mut u64,
    ) {
        let Some(pending) = self.pending_projects.remove(&id) else {
            return;
        };
        if self
            .jobs
            .get(&id)
            .is_none_or(|(job, _)| job.status != JobStatus::Running)
        {
            return;
        }
        let Operation::Save { key } = pending.operation else {
            return;
        };
        let result = pending.error.map_or(result, Err).and_then(|summary| {
            if self.project.path.as_ref()!=Some(&summary.path)||self.project.saved_revision.as_ref()!=Some(&summary.revision_id)||self.arrangement.saved_revision_id.as_ref()!=Some(&key.arrangement_revision_id) {advance(revision).map_err(|mut error|{error.message="Project was written, but session revision exhausted; saved state was not changed".into();error.details=Some(serde_json::json!({"path":summary.path,"written":true}));error})?;}
            let mut protected_roots = std::mem::take(&mut self.project.protected_roots);
            protected_roots.insert(summary.managed_assets_path.clone());
            self.project = ProjectState {
                path: Some(summary.path.clone()),
                saved_revision: Some(summary.revision_id.clone()),
                saved_key: Some(key.clone()),
                protected_roots,
            };
            self.arrangement.saved_revision_id = Some(key.arrangement_revision_id);
            self.arrangement.dirty =
                self.arrangement.saved_revision_id.as_ref() != Some(&self.arrangement.revision_id);
            Ok(summary)
        });
        self.finish_project_job(id, result, false);
    }
    pub(super) fn finish_project_load(
        &mut self,
        id: String,
        result: Result<Box<storage::LoadedProject>, CommandError>,
        revision: &mut u64,
    ) {
        let Some(pending) = self.pending_projects.remove(&id) else {
            return;
        };
        if self
            .jobs
            .get(&id)
            .is_none_or(|(job, _)| job.status != JobStatus::Running)
        {
            return;
        }
        let Operation::Open { base } = pending.operation else {
            return;
        };
        let result = result.and_then(|loaded| {
            if base != self.project_key().revision() {
                return Err(CommandError::new(
                    ErrorCode::StaleRevision,
                    "Workspace changed while loading the project",
                ));
            }
            if !self.pending_outputs.is_empty()
                || !self.pending_projects.is_empty()
                || self
                    .jobs
                    .iter()
                    .any(|(other, (job, _))| other != &id && job.status == JobStatus::Running)
            {
                return Err(CommandError::new(
                    ErrorCode::Busy,
                    "Other work started during project loading; retry after it finishes",
                ));
            }
            advance(revision)?;
            let old = (
                std::mem::replace(&mut self.documents, loaded.payload.documents),
                std::mem::replace(&mut self.revisions, loaded.revisions),
                std::mem::take(&mut self.history),
                self.clipboard.take(),
                self.mix.take(),
            );
            std::thread::spawn(move || drop(old));
            self.active = loaded.payload.active_document_id;
            self.arrangement = loaded.payload.arrangement;
            self.loaded_loop = Some(loaded.payload.loop_enabled);
            self.pending_edits.clear();
            self.pending_clips.clear();
            self.mix_attempt = None;
            self.mix_bases.clear();
            let key = self.project_key();
            let mut protected_roots = std::mem::take(&mut self.project.protected_roots);
            protected_roots.extend(loaded.asset_roots);
            self.project = ProjectState {
                path: Some(loaded.summary.path.clone()),
                saved_revision: Some(key.revision()),
                saved_key: Some(key),
                protected_roots,
            };
            Ok(loaded.summary)
        });
        self.finish_project_job(id, result, true);
    }
    fn finish_project_job(
        &mut self,
        id: String,
        result: Result<projects::ProjectSummary, CommandError>,
        open: bool,
    ) {
        let job = &mut self.jobs.get_mut(&id).unwrap().0;
        match result {
            Ok(summary) => {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.output = Some(if open {
                    JobOutput::ProjectOpened(summary)
                } else {
                    JobOutput::ProjectSaved(summary)
                });
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
