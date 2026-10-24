use super::*;
use wavebob::{
    edits::{EditParams, PasteParams},
    effects::EditTarget,
};
#[derive(Clone, Copy)]
pub(super) enum Action {
    Copy,
    Cut,
    Paste,
    Delete,
    Trim,
}
impl WaveBob {
    fn edit_enabled(&self, action: Action) -> bool {
        self.host.presentation().mode == ViewMode::Waveform
            && self.gesture.is_none()
            && self.active_document().is_some_and(|doc| match action {
                Action::Paste => self.host.clipboard().frames > 0,
                _ => doc.selection.is_some(),
            })
    }
    pub(super) fn edit_action(&mut self, action: Action) {
        if self.host.frozen() || !self.edit_enabled(action) {
            return;
        }
        let doc = self.active_document().unwrap();
        if matches!(action, Action::Paste) {
            let range = doc.selection.unwrap_or(FrameRange {
                start: doc.cursor,
                end: doc.cursor,
            });
            self.dispatch(Command::PasteAudio(PasteParams {
                document_id: doc.id,
                revision_id: doc.revision_id,
                start_frame: range.start,
                end_frame: range.end,
            }));
        } else if let Some(range) = doc.selection {
            let params = EditParams {
                target: EditTarget::Range {
                    document_id: doc.id,
                    revision_id: doc.revision_id,
                    start_frame: range.start,
                    end_frame: range.end,
                },
            };
            self.dispatch(match action {
                Action::Copy => Command::CopyAudio(params),
                Action::Cut => Command::CutAudio(params),
                Action::Delete => Command::DeleteAudio(params),
                Action::Trim => Command::TrimAudio(params),
                Action::Paste => unreachable!(),
            });
        }
    }
    pub(super) fn edit_menu(&mut self, ui: &mut egui::Ui, help: &mut Help) {
        for (action, label, description) in [
            (
                Action::Cut,
                "Cut    Ctrl/Cmd+X",
                "Copy the selection to the internal audio clipboard and delete it as one edit.",
            ),
            (
                Action::Copy,
                "Copy    Ctrl/Cmd+C",
                "Copy the selection without changing audio or undo history.",
            ),
            (
                Action::Paste,
                "Paste    Ctrl/Cmd+V",
                "Insert audio at the cursor or replace the selection, converting rate and channels.",
            ),
            (
                Action::Delete,
                "Delete    Del/Backspace",
                "Delete the selection as one undoable edit.",
            ),
            (
                Action::Trim,
                "Trim to selection    Ctrl/Cmd+T",
                "Keep only the selected frames as one undoable edit.",
            ),
        ] {
            let enabled = self.edit_enabled(action);
            let response = ui.add_enabled(enabled, egui::Button::new(label));
            help.observe(
                &response,
                if enabled {
                    description
                } else if matches!(action, Action::Paste) {
                    "Open a waveform document and copy some audio to enable Paste."
                } else {
                    "Select a nonempty interval in a waveform document to enable this edit."
                },
            );
            if response.clicked() {
                self.edit_action(action);
                ui.close();
            }
        }
    }
    pub(super) fn text_clipboard_input(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if !ctx.text_edit_focused() {
            return;
        }
        // Native clipboard shortcut translation is disabled so an empty OS clipboard
        // cannot swallow the internal audio Paste key. Text widgets explicitly
        // delegate their clipboard requests back to the egui platform integration.
        input.events.retain_mut(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            if !(modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
                || modifiers.alt
                || modifiers.shift
            {
                return true;
            }
            match key {
                egui::Key::C => *event = egui::Event::Copy,
                egui::Key::X => *event = egui::Event::Cut,
                egui::Key::V => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    return false;
                }
                _ => {}
            }
            true
        });
    }
}
