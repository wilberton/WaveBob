use super::*;
use wavebob::effects::{ApplyEffectParams, EditTarget, Effect};
pub(super) struct ReverbControls {
    room_size: f64,
    decay_seconds: f64,
    wet: f64,
}
impl Default for ReverbControls {
    fn default() -> Self {
        Self {
            room_size: 0.5,
            decay_seconds: 1.5,
            wet: 0.25,
        }
    }
}
impl ReverbControls {
    fn effect(&self) -> Effect {
        Effect::Reverb {
            room_size: self.room_size,
            decay_seconds: self.decay_seconds,
            wet: self.wet,
        }
    }
}

impl WaveBob {
    pub(super) fn selected_clip(&self) -> Option<wavebob::arrangement::Clip> {
        if self.host.presentation().mode != ViewMode::Multitrack {
            return None;
        }
        let arr = self.host.snapshot().arrangement;
        if arr.selected_clip_ids.len() != 1 {
            return None;
        }
        arr.clip(&arr.selected_clip_ids[0])
            .map(|(_, clip)| clip.clone())
    }
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
        let effect = self.host.presentation().effect;
        if self.observed_effect.as_ref() != Some(&effect) {
            match &effect {
                Effect::Gain { db } => self.gain_db = *db,
                Effect::Reverb {
                    room_size,
                    decay_seconds,
                    wet,
                } => {
                    self.reverb = ReverbControls {
                        room_size: *room_size,
                        decay_seconds: *decay_seconds,
                        wet: *wet,
                    }
                }
                _ => {}
            }
            self.effect = effect.clone();
            self.observed_effect = Some(effect);
        }
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
        egui::ScrollArea::vertical()
            .id_salt("effects-scroll")
            .auto_shrink([false, false])
            .show(&mut content, |ui| self.effect_controls(ui, help));
    }
    fn effect_controls(&mut self, content: &mut egui::Ui, help: &mut Help) {
        let width = content.available_width();
        content.label("Target");
        let target = self.effect_target();
        let clip = self.selected_clip();
        let enabled = (target.is_some() || clip.is_some())
            && self.gesture.is_none()
            && self.arrangement_gesture.is_none();
        let label = match &target {
            Some(EditTarget::Range { .. }) => "Selection",
            Some(EditTarget::Whole { .. }) => "Whole sample",
            None if clip.is_some() => "Selected clip",
            None => "No editing target",
        };
        help.observe(&content.add_enabled(false,centered_button(label).min_size(Vec2::new(width,26.0))),"Waveform effects target the selection or whole sample. Arrangement effects require exactly one selected clip and affect only that instance.");
        content.add_space(14.0);
        let (gain_row, _) = content.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
        content.painter().text(
            gain_row.left_center(),
            egui::Align2::LEFT_CENTER,
            "Gain",
            egui::FontId::proportional(13.0),
            if matches!(self.effect, Effect::Gain { .. }) {
                Color32::from_rgb(217, 226, 240)
            } else {
                MUTED
            },
        );
        content.painter().text(
            gain_row.right_center(),
            egui::Align2::RIGHT_CENTER,
            format!("{:.1} dB", self.gain_db),
            egui::FontId::proportional(11.0),
            MUTED,
        );
        content.spacing_mut().slider_width = width;
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
            [Effect::FadeIn, Effect::FadeOut],
            [Effect::Reverse, self.reverb.effect()],
        ] {
            content.horizontal(|ui|{for effect in pair{
                let selected=effect==self.effect||(matches!(effect,Effect::Reverb{..})&&matches!(self.effect,Effect::Reverb{..}));
                let response=ui.add(centered_button(effect.label()).selected(selected).min_size(Vec2::new((width-7.0)/2.0,28.0)));
                help.observe(&response,"Choose this effect, then press Apply. Only the displayed target will change.");
                if response.clicked(){self.effect=effect;}
            }});
            content.add_space(4.0);
        }
        content.add_space(5.0);
        if matches!(self.effect, Effect::Reverb { .. }) {
            content.label(RichText::new("Reverb").strong().small());
            content.spacing_mut().slider_width = (width - 110.0).max(30.0);
            let room =
                content.add(egui::Slider::new(&mut self.reverb.room_size, 0.0..=1.0).text("Room"));
            help.observe(
                &room,
                "Room size scales the reverb's reflection delays. Press Apply to process.",
            );
            let decay = content.add(
                egui::Slider::new(&mut self.reverb.decay_seconds, 0.1..=10.0)
                    .logarithmic(true)
                    .text("Decay s"),
            );
            help.observe(&decay, "Reverb decay in seconds, bounded from 0.1 to 10.");
            let wet = content.add(egui::Slider::new(&mut self.reverb.wet, 0.0..=1.0).text("Wet"));
            help.observe(
                &wet,
                "Wet/dry mix: zero is unchanged dry audio; one is only the reverberated signal.",
            );
            self.effect = self.reverb.effect();
            content.label(
                RichText::new("Tail is truncated at target end.")
                    .small()
                    .color(MUTED),
            );
            content.add_space(6.0);
        }
        let response = content.add_enabled(
            enabled,
            centered_button("Apply")
                .fill(BLUE)
                .min_size(Vec2::new(width, 30.0)),
        );
        help.observe(
            &response,
            if enabled {
                "Apply the selected effect as one undoable background edit."
            } else {
                "Select exactly one arrangement clip, or open a nonempty waveform sample. Finish the current gesture to enable Apply."
            },
        );
        if response.clicked()
            && let Some(clip) = &clip
        {
            self.dispatch(Command::ApplyClipEffect(
                wavebob::arrangement::ClipEffectParams {
                    clip_id: clip.id.clone(),
                    edit_revision_id: clip.edit_revision_id.clone(),
                    effect: self.effect.clone(),
                },
            ));
        } else if response.clicked()
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
                    centered_button(label).min_size(Vec2::new((width - 7.0) / 2.0, 26.0)),
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
        if let Some(clip) = clip {
            content.add_space(16.0);
            content.separator();
            content.label(RichText::new("Clip source interval").strong().small());
            content
                .add(egui::Label::new(RichText::new(&clip.name).small().color(MUTED)).truncate());
            let draft =
                self.clip_trim_draft
                    .get_or_insert_with(|| super::arranging::ClipTrimDraft {
                        revision: clip.edit_revision_id.clone(),
                        start: clip.source_start,
                        end: clip.source_end,
                    });
            if draft.revision != clip.edit_revision_id {
                draft.revision = clip.edit_revision_id.clone();
                draft.start = clip.source_start;
                draft.end = clip.source_end;
            }
            content.horizontal(|ui| {
                ui.label("Start");
                let response =
                    ui.add(egui::DragValue::new(&mut draft.start).range(0..=clip.source_frames));
                help.observe(
                    &response,
                    "Exact source frame at the clip's left edge. Press Trim to apply.",
                );
            });
            content.horizontal(|ui| {
                ui.label("End");
                let response =
                    ui.add(egui::DragValue::new(&mut draft.end).range(0..=clip.source_frames));
                help.observe(
                    &response,
                    "Exclusive source frame at the clip's right edge. Press Trim to apply.",
                );
            });
            let interval = (draft.start, draft.end);
            let response = content.add_enabled(
                self.arrangement_gesture.is_none(),
                centered_button("Trim clip").min_size(Vec2::new(width, 26.0)),
            );
            help.observe(&response,"Apply exact source bounds and rebuild this clip's effects. Other instances and the original waveform remain unchanged.");
            if response.clicked() {
                self.dispatch(Command::TrimClip(wavebob::arrangement::TrimClipParams {
                    clip_id: clip.id,
                    edit_revision_id: clip.edit_revision_id,
                    source_start: interval.0,
                    source_end: interval.1,
                }));
            }
            content.label(
                RichText::new(format!(
                    "Processing: {} {}",
                    clip.effects.len(),
                    if clip.effects.len() == 1 {
                        "step"
                    } else {
                        "steps"
                    }
                ))
                .small()
                .color(MUTED),
            );
        }
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
        if !self.host.frozen() && self.effect != self.host.presentation().effect {
            self.dispatch(Command::SetEffect(SetEffectParams {
                effect: self.effect.clone(),
                expected_presentation_revision: Some(self.host.presentation().revision),
            }));
        }
    }
}
