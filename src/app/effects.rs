use super::*;
use wavebob::processing::{self, Action, Owner, ParameterValue};
fn default_geometry(kind: processing::EffectKind) -> WindowGeometry {
    if kind == processing::EffectKind::Reverb {
        WindowGeometry {
            width: 1050.0,
            height: 610.0,
            ..Default::default()
        }
    } else {
        WindowGeometry::default()
    }
}
#[derive(Default)]
pub(super) struct NativeEditor {
    focus: u64,
    close_requested: bool,
    preview: Option<String>,
    control: Option<String>,
    pub(super) help: Option<String>,
}
impl WaveBob {
    pub(super) fn selected_clip(&self) -> Option<wavebob::arrangement::Clip> {
        if self.host.presentation().mode != ViewMode::Multitrack {
            return None;
        }
        let arrangement = self.host.snapshot().arrangement;
        if arrangement.selected_clip_ids.len() != 1 {
            return None;
        }
        arrangement
            .clip(&arrangement.selected_clip_ids[0])
            .map(|(_, clip)| clip.clone())
    }
    pub(super) fn stack_owner(&self) -> Option<Owner> {
        match self.host.presentation().mode {
            ViewMode::Waveform => self.active_document().map(|d| Owner::Document(d.id)),
            ViewMode::Multitrack => self.selected_clip().map(|c| Owner::Clip(c.id)),
        }
    }
    pub(super) fn processing_action(&mut self, owner: &Owner, change: Action) {
        if let Ok(state) = self.host.processing(owner.clone()) {
            self.dispatch(Command::EditProcessing(processing::EditParams {
                owner: owner.clone(),
                expected_edit_revision: state.committed.edit_revision_id,
                change,
            }));
        }
    }
    pub(super) fn effects(&mut self, ui: &mut egui::Ui, area: Rect, help: &mut Help) {
        let mut content = child(ui, area);
        content.horizontal(|ui| {
            let hide = ui.small_button("›");
            help.observe(
                &hide,
                "Hide the effects panel. Backtick toggles both panels.",
            );
            if hide.clicked() {
                self.layout(SetLayoutParams {
                    right_visible: Some(false),
                    ..Default::default()
                });
            }
            section(ui, "EFFECTS");
        });
        content.add_space(12.0);
        let Some(owner) = self.stack_owner() else {
            content.label(
                RichText::new(if self.host.presentation().mode == ViewMode::Multitrack {
                    "Select exactly one clip to edit its effect stack."
                } else {
                    "Open a waveform document to add effects."
                })
                .color(MUTED),
            );
            return;
        };
        let Ok(snapshot) = self.host.processing(owner.clone()) else {
            return;
        };
        let state = snapshot.preview.as_ref().unwrap_or(&snapshot.committed);
        content.label(RichText::new(self.host.owner_name(&owner)).strong());
        content.label(
            RichText::new(match owner {
                Owner::Document(_) => "Whole document",
                Owner::Clip(_) => "Selected clip",
            })
            .small()
            .color(MUTED),
        );
        let exact = content.small_button("Gain and fades…");
        help.observe(&exact,"Edit exact gain, mute and source-frame fades for this owner, including offscreen or tiny bodies.");
        if exact.clicked() {
            self.open_envelope_entry(owner.clone());
        }
        content.add_space(12.0);
        let footer = 88.0;
        egui::ScrollArea::vertical()
            .id_salt("effect-stack")
            .max_height((content.available_height() - footer).max(80.0))
            .show(&mut content, |ui| {
                for event in stack_rows(ui, state, &mut self.stack_drag, help) {
                    match event {
                        StackEvent::Edit(change) => self.processing_action(&owner, change),
                        StackEvent::Open(id) => {
                            self.dispatch(Command::OpenEffectEditor(EditorParams {
                                owner: owner.clone(),
                                instance_id: id,
                                expected_presentation_revision: None,
                            }))
                        }
                    }
                }
                ui.menu_button("＋ Add effect", |ui| {
                    let catalogue = processing::catalogue();
                    if !catalogue.iter().any(|e| e.available) {
                        ui.label("No effects available.");
                    }
                    for info in catalogue.into_iter().filter(|e| e.available) {
                        if ui
                            .add_enabled(
                                state.effects.len() < processing::MAX_EFFECTS,
                                egui::Button::new(info.kind.label()),
                            )
                            .clicked()
                        {
                            self.processing_action(&owner, Action::Add { kind: info.kind });
                            ui.close();
                        }
                    }
                });
            });
        content.add_space(12.0);
        let mut bypass = state.bypassed;
        let response = content.checkbox(&mut bypass, "Bypass all");
        help.observe(
            &response,
            "Bypass this owner's entire stack; the envelope remains active.",
        );
        if response.changed() {
            self.processing_action(&owner, Action::BypassAll { bypassed: bypass });
        }
        let transport = self.host.transport();
        let peak = transport.peak_level;
        content.label(RichText::new("Processed output").small().color(MUTED));
        let db = if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            -120.0
        };
        content.add(
            egui::ProgressBar::new(((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32)
                .fill(if peak > 1.0 {
                    Color32::RED
                } else {
                    Color32::from_rgb(69, 200, 110)
                })
                .text(format!("{db:.1} dBFS")),
        );
        if transport.processing.pending {
            content.label(RichText::new("Preparing sound…").small().color(MUTED));
        }
        if let Some(error) = transport.processing.preparation_error {
            content.colored_label(Color32::LIGHT_RED, error.message);
        }
    }
    pub(super) fn effect_windows(&mut self, ctx: &egui::Context) {
        let presentation = self.host.presentation();
        self.native_editors
            .retain(|id, _| presentation.editor_windows.iter().any(|w| &w.id == id));
        for window in presentation.editor_windows {
            let Ok(snapshot) = self.host.processing(window.owner.clone()) else {
                continue;
            };
            let state = snapshot.preview.as_ref().unwrap_or(&snapshot.committed);
            let Some(instance) = state
                .effects
                .iter()
                .find(|e| e.id == window.instance_id)
                .cloned()
            else {
                continue;
            };
            let geometry = presentation
                .layout
                .editor_geometry
                .get(&window.id)
                .cloned()
                .unwrap_or_else(|| default_geometry(instance.kind));
            let title = format!(
                "{} · {} · {}",
                instance.kind.label(),
                self.host.owner_name(&window.owner),
                &instance.id[..8]
            );
            let monitor = ctx
                .input(|i| i.viewport().monitor_size)
                .unwrap_or(Vec2::new(1920.0, 1080.0));
            let mut builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([
                    geometry.width.min(monitor.x),
                    geometry.height.min(monitor.y),
                ])
                .with_min_inner_size([320.0, 240.0])
                .with_clamp_size_to_monitor_size(true);
            if let (Some(x), Some(y)) = (geometry.x, geometry.y) {
                builder = builder.with_position([
                    x.clamp(0.0, (monitor.x - geometry.width).max(0.0)),
                    y.clamp(0.0, (monitor.y - geometry.height).max(0.0)),
                ]);
            }
            let shared = self.shared_self.clone();
            let id = window.id.clone();
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of(&id),
                builder,
                move |ui, class| {
                    if class == egui::ViewportClass::EmbeddedWindow {
                        ui.label("Native effect windows require a desktop renderer.");
                        return;
                    }
                    if let Some(shared) = shared.upgrade() {
                        shared.lock().unwrap().native_effect_frame(ui, &id);
                    }
                },
            );
        }
    }
    fn native_effect_frame(&mut self, ui: &mut egui::Ui, id: &str) {
        self.host.begin_frame(ui.ctx());
        let presentation = self.host.presentation();
        let Some(window) = presentation
            .editor_windows
            .iter()
            .find(|window| window.id == id)
            .cloned()
        else {
            return;
        };
        let Ok(snapshot) = self.host.processing(window.owner.clone()) else {
            return;
        };
        let state = snapshot.preview.as_ref().unwrap_or(&snapshot.committed);
        let Some(instance) = state
            .effects
            .iter()
            .find(|effect| effect.id == window.instance_id)
            .cloned()
        else {
            return;
        };
        let geometry = presentation
            .layout
            .editor_geometry
            .get(id)
            .cloned()
            .unwrap_or_else(|| default_geometry(instance.kind));
        let mut native = self.native_editors.remove(id).unwrap_or_default();
        if native.focus != window.focus_generation {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            native.focus = window.focus_generation;
        }
        native.close_requested |= ui.input(|input| input.viewport().close_requested());
        if native.close_requested {
            if self.host.frozen() {
                self.native_editors.insert(window.id, native);
                ui.ctx().request_repaint();
                return;
            }
            self.dispatch(Command::CloseEffectEditor(EditorParams {
                owner: window.owner.clone(),
                instance_id: window.instance_id.clone(),
                expected_presentation_revision: None,
            }));
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
            return;
        }
        self.shortcuts(ui.ctx());
        let capture = self.host.editor_capture_requested(&window.id);

        if ui.input(|i| i.viewport().visible() == Some(false)) && !capture {
            self.native_editors.insert(window.id, native);
            return;
        }
        if !self.host.frozen() {
            let size = ui.ctx().viewport_rect().size();
            let position = ui.input(|input| input.viewport().inner_rect.map(|rect| rect.min));
            let next = WindowGeometry {
                width: size.x.clamp(320.0, 4096.0),
                height: size.y.clamp(240.0, 2160.0),
                x: position.map(|point| point.x),
                y: position.map(|point| point.y),
            };
            if next != geometry {
                self.dispatch(Command::SetEditorGeometry(EditorGeometryParams {
                    editor_id: window.id.clone(),
                    geometry: next,
                    expected_presentation_revision: None,
                }));
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if self.host.frozen() {
                ui.visuals_mut().disabled_alpha = 1.0;
                ui.disable();
            }
            ui.heading(instance.kind.label());
            ui.label(RichText::new(self.host.owner_name(&window.owner)).color(MUTED));
            ui.separator();
            native.help = None;
            self.native_parameters(ui, &window, &instance, &mut native);
            let transport = self.host.transport();
            ui.label(
                RichText::new(format!(
                    "Transport output · {:.1} dBFS",
                    if transport.peak_level > 0.0 {
                        20.0 * transport.peak_level.log10()
                    } else {
                        -120.0
                    }
                ))
                .small()
                .color(MUTED),
            );
            let values: Vec<_> = transport
                .analyzer
                .iter()
                .take(128)
                .map(|sample| (sample[0] * 0.5 + 0.5).clamp(0.0, 1.0))
                .collect();
            effect_controls::graph(ui, &values, 64.0);
            ui.separator();
            ui.label(
                RichText::new(
                    native
                        .help
                        .as_deref()
                        .unwrap_or("Changes are heard live · double-click a control to reset"),
                )
                .small()
                .color(MUTED),
            );
            if let Some(error) = &self.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
        });
        self.host.end_editor_frame(ui.ctx(), &window.id);
        self.native_editors.insert(window.id, native);
        if ui.input(|input| !input.events.is_empty()) || self.host.frozen() {
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        }
        if self.host.transport().applied.playing || self.host.transport().processing.pending {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn native_parameters(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &processing::Instance,
        native: &mut NativeEditor,
    ) {
        if ui.input(|i| i.key_pressed(egui::Key::Escape))
            && let Some(id) = native.preview.take()
        {
            self.dispatch(Command::CancelProcessingPreview(
                processing::PreviewParams { preview_id: id },
            ));
            return;
        }
        if instance.kind == processing::EffectKind::SoftClipper {
            self.soft_clipper_controls(ui, window, instance, native);
            return;
        }
        if instance.kind == processing::EffectKind::Reverb {
            self.reverb_controls(ui, window, instance, native);
            return;
        }
        let Some(info) = processing::catalogue()
            .into_iter()
            .find(|e| e.kind == instance.kind)
        else {
            return;
        };
        if !info.parameters.is_empty() {
            ui.label(
                RichText::new("Parameter map · drag to adjust")
                    .small()
                    .color(MUTED),
            );
            let values: Vec<_> = info
                .parameters
                .iter()
                .take(128)
                .map(|parameter| {
                    let value = match instance
                        .parameters
                        .get(&parameter.id)
                        .unwrap_or(&parameter.default)
                    {
                        ParameterValue::Boolean(value) => f64::from(u8::from(*value)),
                        ParameterValue::Integer(value) => *value as f64,
                        ParameterValue::Number(value) => *value,
                    };
                    ((value - parameter.minimum) / (parameter.maximum - parameter.minimum)) as f32
                })
                .collect();
            let (plot, mut response) = effect_controls::graph(ui, &values, 92.0);
            if (response.clicked() || response.dragged())
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let fraction = ((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
                let index = (fraction * (values.len() - 1) as f32).round() as usize;
                let parameter = &info.parameters[index];
                let normalized =
                    ((plot.bottom() - pointer.y) / plot.height()).clamp(0.0, 1.0) as f64;
                let number =
                    parameter.minimum + (parameter.maximum - parameter.minimum) * normalized;
                let value = match parameter.default {
                    ParameterValue::Boolean(_) => ParameterValue::Boolean(number >= 0.5),
                    ParameterValue::Integer(_) => ParameterValue::Integer(number.round() as i32),
                    ParameterValue::Number(_) => ParameterValue::Number(number),
                };
                response.mark_changed();
                self.edit_native_control(
                    window,
                    &instance.id,
                    native,
                    (parameter.id.clone(), value, true),
                    &response,
                    ui.ctx(),
                );
            } else if response.drag_stopped()
                && let Some(id) = native.preview.take()
            {
                self.dispatch(Command::CommitProcessingPreview(
                    processing::PreviewParams { preview_id: id },
                ));
            }
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for parameter in info.parameters {
                let Some(mut value) = instance.parameters.get(&parameter.id).cloned() else {
                    continue;
                };
                let response = ui
                    .horizontal(|ui| {
                        ui.label(&parameter.name);
                        let mut response = match &mut value {
                            ParameterValue::Boolean(value) => ui.checkbox(value, ""),
                            ParameterValue::Integer(value) => {
                                let mut number = *value as f64;
                                let response = effect_controls::number(
                                    ui,
                                    &mut number,
                                    parameter.minimum,
                                    parameter.maximum,
                                    &parameter.unit,
                                    true,
                                    matches!(
                                        parameter.id.as_str(),
                                        "dry" | "wet" | "early" | "volume"
                                    ),
                                );
                                *value = number.round() as i32;
                                response
                            }
                            ParameterValue::Number(value) => effect_controls::number(
                                ui,
                                value,
                                parameter.minimum,
                                parameter.maximum,
                                &parameter.unit,
                                false,
                                matches!(parameter.id.as_str(), "dry" | "wet" | "early" | "volume"),
                            ),
                        };
                        if response.double_clicked() {
                            value = parameter.default.clone();
                            response.mark_changed();
                        }
                        response
                    })
                    .inner;
                if response.hovered() || response.has_focus() {
                    let formatted = match &value {
                        ParameterValue::Boolean(value) => {
                            if *value {
                                "On".into()
                            } else {
                                "Off".into()
                            }
                        }
                        ParameterValue::Integer(value) => value.to_string(),
                        ParameterValue::Number(value) => format!("{value:.4}"),
                    };
                    native.help = Some(format!(
                        "{}: {} {} · Shift for fine adjustment",
                        parameter.name, formatted, parameter.unit
                    ));
                }
                self.edit_native_control(
                    window,
                    &instance.id,
                    native,
                    (parameter.id, value, false),
                    &response,
                    ui.ctx(),
                );
            }
        });
    }
    pub(super) fn edit_native_control(
        &mut self,
        window: &EditorWindow,
        instance_id: &str,
        native: &mut NativeEditor,
        change: (String, ParameterValue, bool),
        response: &egui::Response,
        ctx: &egui::Context,
    ) {
        let control_id = if change.2 {
            "parameter-map".to_owned()
        } else {
            change.0.clone()
        };
        if response.changed() {
            if native
                .control
                .as_ref()
                .is_some_and(|control| control != &control_id)
                && let Some(id) = native.preview.take()
            {
                self.dispatch(Command::CommitProcessingPreview(
                    processing::PreviewParams { preview_id: id },
                ));
            }
            native.control = Some(control_id.clone());
            if native.preview.as_ref().is_some_and(|id| {
                self.host
                    .processing(window.owner.clone())
                    .is_ok_and(|s| s.preview_id.as_ref() != Some(id))
            }) {
                native.preview = None;
            }
            if native.preview.is_none()
                && let Ok(state) = self.host.processing(window.owner.clone())
            {
                let response = self.host.execute(Request::new(
                    "native-preview",
                    Command::BeginProcessingPreview(processing::BeginParams {
                        owner: window.owner.clone(),
                        expected_edit_revision: state.committed.edit_revision_id,
                    }),
                ));
                if let Outcome::Ok {
                    result: CommandResult::Processing(state),
                } = response.outcome
                {
                    native.preview = state.preview_id;
                    self.host
                        .register_editor_preview(&window.id, native.preview.clone());
                }
            }
            if let Some(id) = &native.preview {
                self.dispatch(Command::UpdateProcessingPreview(processing::UpdateParams {
                    preview_id: id.clone(),
                    change: Action::SetParameter {
                        effect_id: instance_id.to_owned(),
                        parameter_id: change.0.clone(),
                        value: change.1,
                    },
                }));
            }
        }
        if native.control.as_ref() == Some(&control_id)
            && (response.drag_stopped()
                || response.lost_focus()
                || (response.changed() && !response.dragged() && !ctx.text_edit_focused()))
            && let Some(id) = native.preview.take()
        {
            self.dispatch(Command::CommitProcessingPreview(
                processing::PreviewParams { preview_id: id },
            ));
        }
    }
}

#[derive(Debug)]
pub(super) enum StackEvent {
    Edit(Action),
    Open(String),
}
pub(super) fn stack_rows(
    ui: &mut egui::Ui,
    state: &processing::Processing,
    drag: &mut Option<String>,
    help: &mut Help,
) -> Vec<StackEvent> {
    let mut events = Vec::new();
    for (index, instance) in state.effects.iter().enumerate() {
        let row = egui::Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(4.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let grip = ui.add(egui::Label::new("⠿").sense(Sense::drag()));
                    help.observe(
                        &grip,
                        "Drag to reorder this effect. Processing follows the displayed order.",
                    );
                    if grip.drag_started() {
                        *drag = Some(instance.id.clone());
                    }
                    let mut enabled = !instance.bypassed;
                    let response = ui.checkbox(&mut enabled, "");
                    help.observe(
                        &response,
                        "Enable or bypass this effect with an aligned dry comparison.",
                    );
                    if response.changed() {
                        events.push(StackEvent::Edit(Action::Bypass {
                            effect_id: instance.id.clone(),
                            bypassed: !enabled,
                        }));
                    }
                    ui.add(egui::Label::new(instance.kind.label()).truncate());
                    let open = ui.small_button("↗");
                    help.observe(
                        &open,
                        "Open controls in this effect's separate native window.",
                    );
                    if open.clicked() {
                        events.push(StackEvent::Open(instance.id.clone()));
                    }
                    ui.menu_button("⋯", |ui| {
                        for (label, enabled, change) in [
                            (
                                "Move up",
                                index > 0,
                                Action::Move {
                                    effect_id: instance.id.clone(),
                                    index: index.saturating_sub(1),
                                },
                            ),
                            (
                                "Move down",
                                index + 1 < state.effects.len(),
                                Action::Move {
                                    effect_id: instance.id.clone(),
                                    index: (index + 1).min(state.effects.len() - 1),
                                },
                            ),
                            (
                                "Remove",
                                true,
                                Action::Remove {
                                    effect_id: instance.id.clone(),
                                },
                            ),
                        ] {
                            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                events.push(StackEvent::Edit(change));
                                ui.close();
                            }
                        }
                    });
                });
            })
            .response;
        if row.contains_pointer()
            && ui.input(|i| i.pointer.any_released())
            && let Some(dragged) = drag.take()
            && dragged != instance.id
        {
            events.push(StackEvent::Edit(Action::Move {
                effect_id: dragged,
                index,
            }));
        }
        ui.add_space(4.0);
    }
    if ui.input(|i| i.pointer.any_released() || i.key_pressed(egui::Key::Escape)) {
        *drag = None;
    }
    events
}
