use super::*;
use wavebob::processing::{self, Action, Owner, ParameterValue};
fn default_geometry(_kind: processing::EffectKind) -> WindowGeometry {
    WindowGeometry {
        width: effect_panel::PANEL_SIZE.x,
        height: effect_panel::PANEL_SIZE.y,
        ..Default::default()
    }
}
#[derive(Default)]
pub(super) struct NativeEditor {
    pub(super) visualization: Option<visualization::Frame>,
    visualization_health: Option<std::sync::Arc<std::sync::Mutex<Option<String>>>>,
    desired_size: Option<Vec2>,
    resize_pending: Option<Vec2>,
    pub(super) limiter_view: Option<limiter::View>,
    focus: u64,
    close_requested: bool,
    preview: Option<String>,
    control: Option<String>,
    pub(super) help: Option<String>,
    pub(super) eq_band: usize,
    pub(super) eq_initialized: bool,
    pub(super) eq_drag: Option<(usize, bool)>,
    pub(super) effect_page: usize,
    pub(super) effect_details: bool,
    pub(super) tempo_text: Option<String>,
    pub(super) fast_dist_curve: Option<([i32; 5], wavebob::dsp::fast_distortion::Curve)>,
    cancelled_drag: bool,
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
        let footer = 132.0;
        let stack_height = (content.available_height() - footer - 12.0).max(40.0);
        egui::ScrollArea::vertical()
            .id_salt("effect-stack")
            .max_height(stack_height)
            .auto_shrink([false, false])
            .show(&mut content, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if state.effects.is_empty() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(16, 22, 29))
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(6.0)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.set_min_width((ui.available_width()).max(0.0));
                            ui.label(RichText::new("Shape your sound").strong().size(13.0));
                            ui.label(
                                RichText::new("Add an effect to start your chain.")
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                }
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
                ui.scope(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(12.0, 8.0);
                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill =
                        Color32::from_rgb(25, 39, 55);
                    ui.style_mut().spacing.interact_size.x = ui.available_width();
                    egui::containers::menu::MenuButton::from_button(
                        centered_button("+ Add effect")
                            .min_size(Vec2::new(ui.available_width(), 32.0)),
                    )
                    .ui(ui, |ui| {
                        ui.style_mut().spacing.interact_size.x = 150.0;
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
                                if let Ok(added) = self.host.processing(owner.clone())
                                    && let Some(instance) =
                                        added.committed.effects.iter().find(|effect| {
                                            !snapshot
                                                .committed
                                                .effects
                                                .iter()
                                                .any(|previous| previous.id == effect.id)
                                        })
                                {
                                    self.dispatch(Command::OpenEffectEditor(EditorParams {
                                        owner: owner.clone(),
                                        instance_id: instance.id.clone(),
                                        expected_presentation_revision: None,
                                    }));
                                }
                                ui.close();
                            }
                        }
                    });
                });
            });
        let footer_area = Rect::from_min_max(
            Pos2::new(
                area.left(),
                (area.bottom() - footer).max(content.cursor().top() + 8.0),
            ),
            area.max,
        );
        let mut content = child(&mut content, footer_area);
        content.separator();
        content.add_space(6.0);
        let mut bypass = state.bypassed;
        let response = content
            .horizontal(|ui| {
                let response = stack_switch(ui, &mut bypass, "Bypass all");
                ui.label(RichText::new("Bypass all").size(12.0));
                response
            })
            .inner;
        help.observe(
            &response,
            "Bypass this owner's entire stack; the envelope remains active.",
        );
        if response.changed() {
            self.processing_action(&owner, Action::BypassAll { bypassed: bypass });
        }
        let transport = self.host.transport();
        content.add_space(8.0);
        output_meter(&mut content, &transport, help);
        if transport.processing.pending {
            content.label(RichText::new("Preparing sound…").small().color(MUTED));
        }
        if let Some(error) = transport.processing.preparation_error {
            content.colored_label(Color32::LIGHT_RED, error.message);
        }
    }
    pub(super) fn effect_windows(&mut self, ctx: &egui::Context) {
        let presentation = self.host.presentation();
        self.native_editors.retain(|id, _| {
            let keep = presentation.editor_windows.iter().any(|w| &w.id == id);
            if !keep {
                self.visualization
                    .retire(egui::ViewportId::from_hash_of(id));
            }
            keep
        });
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
                // Recover saved positions on opening. Once native, leave user moves
                // intact, including negative coordinates and secondary monitors.
                let position = if self.native_editors.contains_key(&window.id) {
                    [x, y]
                } else {
                    [
                        x.clamp(0.0, (monitor.x - geometry.width).max(0.0)),
                        y.clamp(0.0, (monitor.y - geometry.height).max(0.0)),
                    ]
                };
                builder = builder.with_position(position);
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
        if let Some(health) = native.visualization_health.take()
            && let Some(error) = health.lock().unwrap().as_ref()
        {
            self.error = Some(error.clone());
        }
        native.visualization = Some(visualization::Frame::new(
            ui.ctx().viewport_id(),
            ui.ctx().viewport_rect(),
            ui.ctx().pixels_per_point(),
        ));
        let size = ui.ctx().viewport_rect().size();
        let desired = Vec2::new(geometry.width, geometry.height);
        if native.desired_size != Some(desired) {
            native.desired_size = Some(desired);
            let monitor = ui.input(|i| i.viewport().monitor_size).unwrap_or(desired);
            native.resize_pending = Some(desired.min(monitor));
        }
        if let Some(target) = native.resize_pending {
            if (size - target).abs().max_elem() <= 1.0 {
                native.resize_pending = None;
            } else {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(target));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        if !self.host.frozen() && native.resize_pending.is_none() {
            // ViewportBuilder::with_position expects the OUTER window origin.
            // Feeding back the client origin adds the decoration offset each frame.
            let position = ui.input(|input| input.viewport().outer_rect.map(|rect| rect.min));
            let next = WindowGeometry {
                width: size.x.clamp(320.0, 4096.0),
                height: size.y.clamp(240.0, 2160.0),
                x: position.map(|point| point.x),
                y: position.map(|point| point.y),
            };
            if next != geometry {
                // An acknowledged user resize is feedback, not a fresh request.
                native.desired_size = Some(Vec2::new(next.width, next.height));
                self.dispatch(Command::SetEditorGeometry(EditorGeometryParams {
                    editor_id: window.id.clone(),
                    geometry: next,
                    expected_presentation_revision: None,
                }));
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.host.frozen() {
                    ui.visuals_mut().disabled_alpha = 1.0;
                    ui.disable();
                }
                native.help = None;
                self.native_parameters(ui, &window, &instance, &mut native);
                if let Some(error) = &self.error {
                    ui.painter().text(
                        ui.max_rect().left_bottom() + Vec2::new(12.0, -6.0),
                        egui::Align2::LEFT_BOTTOM,
                        error,
                        egui::FontId::proportional(12.0),
                        Color32::LIGHT_RED,
                    );
                }
            });
        let health = self
            .visualization
            .finish(native.visualization.take().unwrap());
        if native.resize_pending.is_none() {
            self.host
                .end_editor_frame_checked(ui.ctx(), &window.id, health.clone());
        }
        native.visualization_health = Some(health);
        self.native_editors.insert(window.id, native);
        if ui.input(|input| !input.events.is_empty()) || self.host.frozen() {
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        }
        if self.host.transport().applied.playing || self.host.transport().processing.pending {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    pub(super) fn native_parameters(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &processing::Instance,
        native: &mut NativeEditor,
    ) {
        if ui.is_enabled()
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
            && let Some(id) = native.preview.take()
        {
            self.dispatch(Command::CancelProcessingPreview(
                processing::PreviewParams { preview_id: id },
            ));
            native.eq_drag = None;
            native.cancelled_drag = ui.input(|i| i.pointer.any_down());
            // Draw the restored state in this frame, including while a cancelled
            // pointer is still held. Taking preview above bounds this recursion.
            if let Ok(state) = self.host.processing(window.owner.clone())
                && let Some(restored) = state.committed.effects.iter().find(|e| e.id == instance.id)
            {
                self.native_parameters(ui, window, restored, native);
            }
            return;
        }
        if native.cancelled_drag {
            if ui.input(|i| i.pointer.any_down()) {
                ui.visuals_mut().disabled_alpha = 1.0;
                ui.disable();
            } else {
                native.cancelled_drag = false;
            }
        }
        match instance.kind {
            processing::EffectKind::SoftClipper => {
                self.soft_clipper_controls(ui, window, instance, native)
            }
            processing::EffectKind::Reverb => self.reverb_controls(ui, window, instance, native),
            processing::EffectKind::FastDistortion => {
                self.fast_distortion_controls(ui, window, instance, native)
            }
            processing::EffectKind::ParametricEq => {
                self.parametric_eq_controls(ui, window, instance, native)
            }
            processing::EffectKind::Delay => self.delay_controls(ui, window, instance, native),
            processing::EffectKind::Chorus => self.chorus_controls(ui, window, instance, native),
            processing::EffectKind::Limiter => self.limiter_controls(ui, window, instance, native),
            processing::EffectKind::Balance => self.balance_controls(ui, window, instance, native),
            processing::EffectKind::StereoEnhancer => {
                self.stereo_enhancer_controls(ui, window, instance, native)
            }
            processing::EffectKind::StereoShaper => {
                self.stereo_shaper_controls(ui, window, instance, native)
            }
            processing::EffectKind::TransientProcessor => {
                self.transient_processor_controls(ui, window, instance, native)
            }
        }
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
        self.edit_native_parameters(window, instance_id, native, vec![change], response, ctx);
    }
    pub(super) fn finish_native_edit(&mut self, native: &mut NativeEditor) {
        if let Some(id) = native.preview.take() {
            self.dispatch(Command::CommitProcessingPreview(
                processing::PreviewParams { preview_id: id },
            ));
        }
        native.control = None;
    }
    pub(super) fn edit_native_parameters(
        &mut self,
        window: &EditorWindow,
        instance_id: &str,
        native: &mut NativeEditor,
        changes: Vec<(String, ParameterValue, bool)>,
        response: &egui::Response,
        ctx: &egui::Context,
    ) {
        if self.host.frozen() {
            return;
        }
        let change = &changes[0];
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
                for change in changes {
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

fn stack_switch(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(28.0, 22.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *value, label)
    });
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let track = Rect::from_center_size(rect.center(), Vec2::new(28.0, 15.0));
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        track,
        8.0,
        if *value {
            BLUE
        } else {
            Color32::from_rgb(48, 60, 74)
        },
        Stroke::new(
            1.0,
            if response.has_focus() || response.hovered() {
                visuals.fg_stroke.color
            } else {
                BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    let x = if *value {
        track.right() - 7.5
    } else {
        track.left() + 7.5
    };
    ui.painter().circle_filled(
        Pos2::new(x, track.center().y),
        5.0,
        if *value { Color32::WHITE } else { MUTED },
    );
    response
}

fn output_meter(
    ui: &mut egui::Ui,
    transport: &wavebob::transport::TransportSnapshot,
    help: &mut Help,
) {
    let peak = transport.peak_level;
    let db = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        -120.0
    };
    egui::Frame::new().fill(Color32::from_rgb(12, 18, 24))
        .stroke(Stroke::new(1.0, BORDER)).corner_radius(6.0).inner_margin(10.0)
        .show(ui, |ui| {
            let (header, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
            ui.painter().text(header.left_center(), egui::Align2::LEFT_CENTER, "OUTPUT", egui::FontId::proportional(11.0), MUTED);
            ui.painter().text(header.right_center(), egui::Align2::RIGHT_CENTER,
                if peak <= 0.0 { "−∞ dBFS".into() } else { format!("{db:.1} dBFS") },
                egui::FontId::monospace(11.0), if peak > 1.0 { Color32::from_rgb(255, 107, 100) } else { Color32::from_rgb(209, 222, 237) });
            ui.add_space(5.0);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 13.0), Sense::hover());
            help.observe(&response, "Live transport output peak in dBFS, including effects and browser audition. Red indicates overload above 0 dBFS.");
            let count = (rect.width() / 7.0).floor().max(1.0) as usize;
            let step = rect.width() / count as f32;
            let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
            for index in 0..count {
                let fraction = (index + 1) as f32 / count as f32;
                let color = if fraction > 0.95 && peak > 1.0 { Color32::from_rgb(244, 100, 92) }
                    else if fraction > 0.8 { Color32::from_rgb(231, 189, 80) }
                    else { Color32::from_rgb(58, 195, 163) };
                let segment = Rect::from_min_size(rect.min + Vec2::new(index as f32 * step, 0.0), Vec2::new((step - 2.0).max(1.0), rect.height()));
                ui.painter().rect_filled(segment, 1.0, if fraction <= level { color } else { color.gamma_multiply(0.12) });
            }
            let (scale, _) = ui.allocate_exact_size(Vec2::new(rect.width(), 13.0), Sense::hover());
            for (fraction, label, align) in [(0.0, "−60", egui::Align2::LEFT_TOP), (0.4, "−36", egui::Align2::CENTER_TOP), (0.7, "−18", egui::Align2::CENTER_TOP), (1.0, "0", egui::Align2::RIGHT_TOP)] {
                ui.painter().text(Pos2::new(scale.left() + fraction * scale.width(), scale.top()), align, label, egui::FontId::monospace(9.0), MUTED);
            }
        });
}

pub(super) fn stack_rows(
    ui: &mut egui::Ui,
    state: &processing::Processing,
    drag: &mut Option<String>,
    help: &mut Help,
) -> Vec<StackEvent> {
    let mut events = Vec::new();
    for (index, instance) in state.effects.iter().enumerate() {
        ui.push_id(&instance.id, |ui| {
            let active = !state.bypassed && !instance.bypassed;
            let narrow = ui.available_width() < 240.0;
            let (row, response) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), if narrow { 66.0 } else { 48.0 }),
                Sense::hover(),
            );
            ui.painter().rect(
                row,
                6.0,
                if active {
                    Color32::from_rgb(24, 35, 47)
                } else {
                    Color32::from_rgb(19, 25, 32)
                },
                Stroke::new(
                    1.0,
                    if active {
                        Color32::from_rgb(49, 72, 95)
                    } else {
                        BORDER
                    },
                ),
                egui::StrokeKind::Inside,
            );
            let y = if narrow {
                row.top() + 47.0
            } else {
                row.center().y
            };
            let grip_rect =
                Rect::from_center_size(Pos2::new(row.left() + 13.0, y), Vec2::new(16.0, 26.0));
            let grip = ui
                .interact(grip_rect, ui.scope_id().with("grip"), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::Grab);
            for x in [-2.0, 2.0] {
                for dy in [-4.0, 0.0, 4.0] {
                    ui.painter()
                        .circle_filled(grip_rect.center() + Vec2::new(x, dy), 1.1, MUTED);
                }
            }
            help.observe(
                &grip,
                "Drag to reorder this effect. Processing follows the displayed order.",
            );
            if grip.drag_started() {
                *drag = Some(instance.id.clone());
            }
            let switch_rect =
                Rect::from_center_size(Pos2::new(row.left() + 40.0, y), Vec2::new(28.0, 22.0));
            let mut switch_ui = child(ui, switch_rect);
            let mut enabled = !instance.bypassed;
            let switch = stack_switch(&mut switch_ui, &mut enabled, instance.kind.label());
            help.observe(
                &switch,
                "Enable or bypass this effect with an aligned dry comparison.",
            );
            if switch.changed() {
                events.push(StackEvent::Edit(Action::Bypass {
                    effect_id: instance.id.clone(),
                    bypassed: !enabled,
                }));
            }
            let title_rect = if narrow {
                Rect::from_min_max(
                    row.min + Vec2::new(12.0, 5.0),
                    Pos2::new(row.right() - 12.0, row.top() + 28.0),
                )
            } else {
                Rect::from_min_max(
                    Pos2::new(row.left() + 62.0, y - 12.0),
                    Pos2::new(row.right() - 68.0, y + 12.0),
                )
            };
            let mut title_ui = child(ui, title_rect);
            title_ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                let title = ui.add(
                    egui::Label::new(RichText::new(instance.kind.label()).size(12.0).color(
                        if active {
                            Color32::from_rgb(221, 232, 245)
                        } else {
                            MUTED
                        },
                    ))
                    .truncate(),
                );
                help.observe(&title, instance.kind.label());
            });
            let open = ui.put(
                Rect::from_center_size(Pos2::new(row.right() - 49.0, y), Vec2::new(26.0, 26.0)),
                egui::Button::new(RichText::new("↗").color(if active {
                    Color32::from_rgb(125, 187, 255)
                } else {
                    MUTED
                })),
            );
            help.observe(
                &open,
                "Open controls in this effect's separate native window.",
            );
            if open.clicked() {
                events.push(StackEvent::Open(instance.id.clone()));
            }
            let mut menu_ui = child(
                ui,
                Rect::from_center_size(Pos2::new(row.right() - 19.0, y), Vec2::new(26.0, 26.0)),
            );
            menu_ui.menu_button("…", |ui| {
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
            if response.contains_pointer()
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
        });
    }
    if ui.input(|i| i.pointer.any_released() || i.key_pressed(egui::Key::Escape)) {
        *drag = None;
    }
    events
}
