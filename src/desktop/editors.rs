//! Stable, presentation-only bindings for native effect windows.
use super::*;
use crate::processing::{Owner, Snapshot};
impl DesktopHost {
    pub fn register_editor_preview(&mut self, id: &str, preview_id: Option<String>) {
        if let Some(preview) = preview_id {
            self.editor_previews.insert(id.into(), preview);
        } else {
            self.editor_previews.remove(id);
        }
    }
    pub fn processing(&self, owner: Owner) -> Result<Snapshot, CommandError> {
        self.session.editor.processing_read(owner)
    }
    pub fn owner_name(&self, owner: &Owner) -> String {
        let snapshot = self.session.snapshot();
        match owner {
            Owner::Document(id) => snapshot
                .documents
                .iter()
                .find(|d| &d.id == id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "Document".into()),
            Owner::Clip(id) => snapshot
                .arrangement
                .clip(id)
                .map(|(_, c)| c.name.clone())
                .unwrap_or_else(|| "Clip".into()),
        }
    }
    pub(super) fn editor_command(&mut self, request: Request) -> Response {
        let mut next = self.presentation.clone();
        match &request.command {
            Command::SetEditorGeometry(params) => {
                if let Err(error) = params.geometry.validate() {
                    return self.session.failure(Some(request.id), error);
                }
                if !next.editor_windows.iter().any(|w| w.id == params.editor_id) {
                    return self.session.failure(
                        Some(request.id),
                        CommandError::new(ErrorCode::NotFound, "Editor is closed"),
                    );
                }
                if next.layout.editor_geometry.len() >= 128
                    && !next.layout.editor_geometry.contains_key(&params.editor_id)
                {
                    let key = next.layout.editor_geometry.keys().next().unwrap().clone();
                    next.layout.editor_geometry.remove(&key);
                }
                next.layout
                    .editor_geometry
                    .insert(params.editor_id.clone(), params.geometry.clone());
            }
            Command::OpenEffectEditor(params)
            | Command::FocusEffectEditor(params)
            | Command::CloseEffectEditor(params) => {
                let id = crate::projects::checksum(&(&params.owner, &params.instance_id));
                let index = next.editor_windows.iter().position(|w| w.id == id);
                if matches!(request.command, Command::CloseEffectEditor(_)) {
                    if let Some(index) = index {
                        if let Some(id) = self.editor_previews.remove(&id)
                            && self
                                .processing(params.owner.clone())
                                .is_ok_and(|state| state.preview_id.as_ref() == Some(&id))
                        {
                            self.session.execute_committed(Request::new(
                                "close-editor-preview",
                                Command::CancelProcessingPreview(
                                    crate::processing::PreviewParams { preview_id: id },
                                ),
                            ));
                        }
                        next.editor_windows.remove(index);
                    }
                } else {
                    let state = match self.processing(params.owner.clone()) {
                        Ok(state) => state,
                        Err(error) => return self.session.failure(Some(request.id), error),
                    };
                    if !state
                        .committed
                        .effects
                        .iter()
                        .any(|e| e.id == params.instance_id)
                    {
                        return self.session.failure(
                            Some(request.id),
                            CommandError::new(
                                ErrorCode::NotFound,
                                "Effect instance does not exist on that owner",
                            ),
                        );
                    }
                    if let Some(index) = index {
                        let Some(generation) =
                            next.editor_windows[index].focus_generation.checked_add(1)
                        else {
                            return self.session.failure(
                                Some(request.id),
                                CommandError::new(
                                    ErrorCode::RevisionExhausted,
                                    "Editor focus generation exhausted",
                                ),
                            );
                        };
                        next.editor_windows[index].focus_generation = generation;
                    } else if matches!(request.command, Command::FocusEffectEditor(_)) {
                        return self.session.failure(
                            Some(request.id),
                            CommandError::new(ErrorCode::NotFound, "Editor is closed"),
                        );
                    } else {
                        if next.editor_windows.len() >= 32 {
                            return self.session.failure(
                                Some(request.id),
                                CommandError::new(
                                    ErrorCode::ResourceLimit,
                                    "At most 32 native effect editors may be open",
                                ),
                            );
                        }
                        next.editor_windows.push(EditorWindow {
                            id,
                            owner: params.owner.clone(),
                            instance_id: params.instance_id.clone(),
                            focus_generation: 1,
                        });
                    }
                }
            }
            _ => unreachable!(),
        }
        self.commit_presentation(request.id, next, true)
    }
    pub(super) fn sync_editors(&mut self) {
        let mut next = self.presentation.clone();
        next.editor_windows.retain(|window| {
            self.processing(window.owner.clone()).is_ok_and(|state| {
                state
                    .committed
                    .effects
                    .iter()
                    .any(|e| e.id == window.instance_id)
            })
        });
        self.editor_previews
            .retain(|id, _| next.editor_windows.iter().any(|w| &w.id == id));
        if next.editor_windows != self.presentation.editor_windows {
            self.commit_presentation("editor-lifecycle".into(), next, false);
        }
    }
}
