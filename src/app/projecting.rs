use super::*;
use wavebob::projects::SaveProjectParams;

pub(super) struct ProjectPicker {
    pub(super) receiver: mpsc::Receiver<Option<std::path::PathBuf>>,
    pub(super) save: bool,
    pub(super) close_after: bool,
    pub(super) revision: String,
}
impl WaveBob {
    pub(super) fn project_save(&mut self, ctx: &egui::Context, save_as: bool, close_after: bool) {
        if self.host.frozen() {
            return;
        }
        let project = self.host.snapshot().project;
        if !save_as && let Some(path) = project.path {
            self.submit_output(
                Command::SaveProject(SaveProjectParams {
                    path,
                    overwrite: true,
                    expected_project_revision: Some(project.revision_id),
                }),
                close_after,
            );
        } else {
            self.choose_project(ctx, true, close_after);
        }
    }
    pub(super) fn choose_project(&mut self, ctx: &egui::Context, save: bool, close_after: bool) {
        if self.host.frozen() || self.project_picker.is_some() {
            return;
        }
        let project = self.host.snapshot().project;
        let (sender, receiver) = mpsc::channel();
        self.project_picker = Some(ProjectPicker {
            receiver,
            save,
            close_after,
            revision: project.revision_id,
        });
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut picker = rfd::FileDialog::new().add_filter("WaveBob project", &["wavebob"]);
            if let Some(path) = project.path {
                let path = std::path::PathBuf::from(path);
                if let Some(parent) = path.parent() {
                    picker = picker.set_directory(parent);
                }
                if let Some(name) = path.file_name() {
                    picker = picker.set_file_name(name.to_string_lossy());
                }
            } else {
                picker = picker.set_file_name("Untitled.wavebob");
            }
            let path = if save {
                picker.set_title("Save project").save_file()
            } else {
                picker.set_title("Open project").pick_file()
            };
            let _ = sender.send(path);
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        });
    }
    pub(super) fn poll_project_picker(&mut self) {
        let Some(picker) = self.project_picker.take() else {
            return;
        };
        let path = match picker.receiver.try_recv() {
            Ok(path) => path,
            Err(mpsc::TryRecvError::Empty) => {
                self.project_picker = Some(picker);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.error = Some("The project dialog did not complete.".into());
                None
            }
        };
        let close_after = picker.close_after && self.close_flow.is_some();
        if let Some(path) = path {
            if let Some(path) = path.to_str() {
                if picker.save {
                    if self.submit_output(
                        Command::SaveProject(SaveProjectParams {
                            path: path.into(),
                            overwrite: true,
                            expected_project_revision: Some(picker.revision),
                        }),
                        close_after,
                    ) {
                        return;
                    }
                } else {
                    self.begin_close(saving::CloseTarget::OpenProject(path.into()));
                    return;
                }
            } else {
                self.error = Some("The selected project path is not valid Unicode.".into());
            }
        }
        if close_after && let Some(flow) = &mut self.close_flow {
            flow.saving = false;
        }
    }
    pub(super) fn document_unsaved(&self, doc: &DocumentInfo) -> bool {
        doc.dirty
            && self
                .host
                .snapshot()
                .project
                .saved_document_revisions
                .get(&doc.id)
                != Some(&doc.revision_id)
    }
}
