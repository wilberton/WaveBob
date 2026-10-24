use super::*;
use wavebob::effects::{ApplyEffectParams, EditTarget, Effect};

impl WaveBob {
    fn effect_target(&self) -> Option<EditTarget> {
        if self.host.presentation().mode != ViewMode::Waveform {
            return None;
        }
        let document = self.active_document()?;
        if document.frames == 0 {
            return None;
        }
        Some(if let Some(range) = document.selection {
            EditTarget::Range {
                document_id: document.id,
                revision_id: document.revision_id,
                start_frame: range.start,
                end_frame: range.end,
            }
        } else {
            EditTarget::Whole {
                document_id: document.id,
                revision_id: document.revision_id,
            }
        })
    }
    pub(super) fn effects(&mut self, ui: &mut egui::Ui, area: Rect, help: &mut Help) {
        let mut content = child(ui, area);
        content.horizontal(|ui| {
            let response = ui.add(egui::Button::new("›").frame(false));
            help.observe(
                &response,
                "Hide the effects panel. Backtick toggles both panels.",
            );
            if response.clicked() {
                self.layout(SetLayoutParams {
                    right_visible: Some(false),
                    ..Default::default()
                });
            }
            section(ui, "EDIT & EFFECTS");
        });
        content.add_space(18.0);
        content.label("Target");
        let target = self.effect_target();
        let enabled = target.is_some() && self.gesture.is_none();
        let label = match &target {
            Some(EditTarget::Range { .. }) => "Selection",
            Some(EditTarget::Whole { .. }) => "Whole sample",
            None => "No editing target",
        };
        help.observe(&content.add_enabled(false,centered_button(label).min_size(Vec2::new(area.width(),26.0))),"Effects apply to the selection, or the whole sample when no selection exists. Arrangement clip processing arrives in ticket 15.");
        content.add_space(14.0);
        content.horizontal(|ui| {
            ui.label("Gain");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.1} dB", self.gain_db))
                        .small()
                        .color(MUTED),
                );
            });
        });
        content.spacing_mut().slider_width = area.width();
        let response =
            content.add(egui::Slider::new(&mut self.gain_db, -24.0..=12.0).show_value(false));
        help.observe(
            &response,
            "Set gain in dB. Changing parameters does not edit audio; press Apply.",
        );
        if response.changed() || response.clicked() || response.drag_started() {
            self.effect = Effect::Gain { db: self.gain_db };
        }
        content.add_space(12.0);
        for pair in [
            [Some(Effect::FadeIn), Some(Effect::FadeOut)],
            [Some(Effect::Reverse), None],
        ] {
            content.horizontal(|ui|{for effect in pair{
                let label=effect.as_ref().map_or("Reverb",Effect::label);
                let selected=effect.as_ref().is_some_and(|effect|effect==&self.effect);
                let response=ui.add_enabled(effect.is_some(),centered_button(label).selected(selected).min_size(Vec2::new((area.width()-7.0)/2.0,28.0)));
                help.observe(&response,if effect.is_some(){"Choose this effect, then press Apply. Only the displayed target will change."}else{"Reverb becomes available in ticket 12."});
                if response.clicked()&&let Some(effect)=effect{self.effect=effect;}
            }});
            content.add_space(4.0);
        }
        content.add_space(5.0);
        let response = content.add_enabled(
            enabled,
            centered_button("Apply")
                .fill(BLUE)
                .min_size(Vec2::new(area.width(), 30.0)),
        );
        help.observe(
            &response,
            if enabled {
                "Apply the selected effect as one undoable background edit."
            } else {
                "Open a nonempty sample and finish any selection gesture to enable Apply."
            },
        );
        if response.clicked()
            && let Some(target) = target
        {
            self.dispatch(Command::ApplyEffect(ApplyEffectParams {
                target,
                effect: self.effect.clone(),
            }));
        }
        content.add_space(12.0);
        let history = self.host.history();
        content.horizontal(|ui| {
            for (label, available, redo) in [
                ("Undo", history.can_undo, false),
                ("Redo", history.can_redo, true),
            ] {
                let response = ui.add_enabled(
                    available,
                    centered_button(label).min_size(Vec2::new((area.width() - 7.0) / 2.0, 26.0)),
                );
                help.observe(
                    &response,
                    if available {
                        if redo {
                            "Redo the last undone edit. Ctrl/Cmd+Shift+Z or Ctrl+Y."
                        } else {
                            "Undo the last committed edit. Ctrl/Cmd+Z."
                        }
                    } else {
                        if redo {
                            "There is no undone edit to redo."
                        } else {
                            "No audio edits have been committed yet."
                        }
                    },
                );
                if response.clicked() {
                    self.dispatch(if redo {
                        Command::Redo(EmptyParams {})
                    } else {
                        Command::Undo(EmptyParams {})
                    });
                }
            }
        });
        if let Some(document) = self.active_document()
            && document.overloaded_samples > 0
        {
            content.add_space(8.0);
            content.label(
                RichText::new(format!(
                    "Peak {:+.1} dB · {} overloaded samples",
                    20.0 * document.peak_level.log10(),
                    document.overloaded_samples
                ))
                .small()
                .color(Color32::from_rgb(226, 178, 89)),
            );
        }
    }
}
