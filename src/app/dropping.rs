use super::*;
use std::{collections::VecDeque, path::PathBuf};

#[derive(Default)]
pub(super) struct FileImports {
    pub(super) paths: VecDeque<PathBuf>,
    activate_waveform: bool,
}
impl FileImports {
    pub(super) fn clear(&mut self) {
        self.paths.clear();
        self.activate_waveform = false;
    }
}
fn supported(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("wav") || extension.eq_ignore_ascii_case("mp3")
        })
}
impl WaveBob {
    fn import_dialog_blocked(&self, ctx: &egui::Context) -> bool {
        self.dialog.is_some()
            || self.open_requested
            || self.output_dialog.is_some()
            || self.save_picker.is_some()
            || self.project_picker.is_some()
            || self.close_flow.is_some()
            || ctx.memory(|memory| memory.top_modal_layer().is_some())
            || self
                .host
                .snapshot()
                .jobs
                .iter()
                .any(|job| job.status == JobStatus::Running && job.kind == "Open project")
    }
    pub(super) fn file_drops(&mut self, ctx: &egui::Context) {
        let files = ctx.input_mut(|input| std::mem::take(&mut input.raw.dropped_files));
        if self.host.presentation().close_requested {
            self.file_imports.clear();
            return;
        }
        if !files.is_empty() {
            if self.file_imports.paths.is_empty() {
                self.file_imports.activate_waveform = true;
            }
            self.file_imports
                .paths
                .extend(files.iter().map(|file| file.path().to_path_buf()));
        }
        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        if !self.host.frozen() && (hovering || !files.is_empty()) {
            self.gesture = None;
            self.arrangement_gesture = None;
        }
        if self.host.frozen() || self.import_dialog_blocked(ctx) {
            return;
        }
        // Consume native events once, retain only paths, and let the existing
        // command layer own decoding, budgets, progress and publication. Limit
        // UI work per frame as well as respecting the core's worker limit.
        for _ in 0..2 {
            let Some(path) = self.file_imports.paths.front() else {
                break;
            };
            if !supported(path) {
                self.error = Some(format!(
                    "Cannot open {}: drop WAV or MP3 audio files.",
                    path.display()
                ));
                self.file_imports.paths.pop_front();
                continue;
            }
            let Some(path) = path.to_str() else {
                self.error = Some("A dropped audio path is not valid Unicode.".into());
                self.file_imports.paths.pop_front();
                continue;
            };
            match self
                .host
                .execute(Request::new(
                    "gui-file-drop",
                    Command::OpenDocument(OpenParams { path: path.into() }),
                ))
                .outcome
            {
                Outcome::Ok {
                    result: CommandResult::Job(_),
                } => {
                    self.file_imports.paths.pop_front();
                    if self.file_imports.activate_waveform {
                        self.file_imports.activate_waveform = false;
                        let response = self.host.execute(Request::new(
                            "gui-file-drop-mode",
                            Command::SetMode(SetModeParams {
                                mode: ViewMode::Waveform,
                                expected_presentation_revision: None,
                            }),
                        ));
                        if let Outcome::Error { error } = response.outcome {
                            self.error = Some(error.message);
                        }
                    }
                }
                Outcome::Error { error } if error.code == wavebob::ErrorCode::Busy => break,
                Outcome::Error { error } => {
                    self.error = Some(error.message);
                    self.file_imports.paths.pop_front();
                }
                _ => {
                    self.file_imports.paths.pop_front();
                }
            }
        }
    }
    pub(super) fn file_drop_feedback(&self, ctx: &egui::Context, help: &mut Help) {
        let hovered = ctx.input(|input| input.raw.hovered_files.clone());
        if hovered.is_empty() {
            if !self.file_imports.paths.is_empty() && help.text().is_empty() {
                help.focus = Some(format!(
                    "{} files queued to open. Cancel queued files in the open-file panel.",
                    self.file_imports.paths.len()
                ));
            }
            return;
        }
        let supported_count = hovered
            .iter()
            .filter(|file| file.path.as_ref().is_none_or(|path| supported(path)))
            .count();
        let title = if supported_count == 0 {
            "Drop WAV or MP3 audio files"
        } else {
            "Drop audio to open"
        };
        let detail = if self.import_dialog_blocked(ctx) {
            "Files will open after the current dialog or operation finishes.".into()
        } else if supported_count == 0 {
            "These files are not supported.".into()
        } else {
            format!(
                "{supported_count} audio file{} · WAV & MP3",
                if supported_count == 1 { "" } else { "s" }
            )
        };
        help.hover=Some("Drop WAV or MP3 files to open documents. Drag open documents onto tracks to arrange them.".into());
        let rect = ctx.content_rect().shrink2(Vec2::new(12.0, 40.0));
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("external-file-drop"),
        ));
        painter.rect_filled(rect, 8.0, Color32::from_rgba_unmultiplied(14, 22, 31, 230));
        painter.rect_stroke(rect, 8.0, Stroke::new(2.0, BLUE), egui::StrokeKind::Inside);
        painter.text(
            rect.center() - Vec2::new(0.0, 14.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(22.0),
            Color32::WHITE,
        );
        painter.text(
            rect.center() + Vec2::new(0.0, 18.0),
            egui::Align2::CENTER_CENTER,
            detail,
            egui::FontId::proportional(13.0),
            MUTED,
        );
    }
}
