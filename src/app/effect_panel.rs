//! Shared Reverb/Distortion panel chrome and segmented dial interactions.
use super::*;
use wavebob::processing::{self, Action, Instance};
pub(super) const BACKGROUND: Color32 = Color32::from_rgb(13, 23, 30);
pub(super) const BANK: Color32 = Color32::from_rgb(22, 34, 44);
pub(super) const TEXT: Color32 = Color32::from_rgb(231, 237, 245);
pub(super) const TICK: Color32 = Color32::from_rgb(67, 87, 104);
pub(super) const ACCENT: Color32 = Color32::from_rgb(0, 142, 255);
/// Full dial travel in logical screen points, independent of units and layout size.
/// The old Width value drag covered 200 / (100 / 64) = 128 points: 1.4x ≈ 180.
const DIAL_TRAVEL: f64 = 180.0;
/// Panels open at 75% of their original composition; dials shrink a further 80%.
pub(super) const PANEL_SIZE: Vec2 = Vec2::new(637.5, 397.5);
const DIAL_SCALE: f32 = 0.8;
pub(super) fn font(size: f32, semibold: bool) -> egui::FontId {
    egui::FontId::new(
        size,
        egui::FontFamily::Name(
            if semibold {
                "Inter Semibold"
            } else {
                "Inter Medium"
            }
            .into(),
        ),
    )
}

/// Logical panel geometry shared by the effect-specific compositions.
#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub canvas: Rect,
    pub scale: f32,
    pub origin: Pos2,
}
impl Layout {
    pub fn new((canvas, scale, origin): (Rect, f32, Pos2)) -> Self {
        Self {
            canvas,
            scale,
            origin,
        }
    }
    pub fn p(self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.scale
    }
    pub fn r(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.p(x, y), Vec2::new(w, h) * self.scale)
    }
    pub fn text(
        self,
        ui: &egui::Ui,
        x: f32,
        y: f32,
        text: impl ToString,
        size: f32,
        color: Color32,
    ) {
        ui.painter_at(self.canvas).text(
            self.p(x, y),
            egui::Align2::LEFT_CENTER,
            text,
            font(size * self.scale, false),
            color,
        );
    }
    pub fn footer(self, ui: &egui::Ui, native: &effects::NativeEditor, text: &str) {
        self.text(
            ui,
            14.0,
            521.0,
            native.help.as_deref().unwrap_or(text),
            10.0,
            MUTED,
        );
    }
}

/// Presentation and mapping for the shared segmented blue dial.
pub(super) trait Dial: Copy + 'static {
    fn label(self) -> &'static str;
    fn range(self) -> (f64, f64, f64);
    fn reset(self) -> f64;
    fn text(self, value: f64) -> String;
    fn value_font_size(self) -> f32 {
        19.0
    }
    /// A physical neutral point for signed controls; old unipolar arcs are unchanged.
    fn neutral(self) -> Option<f64> {
        None
    }
    fn fraction(self, value: f64) -> f64 {
        let (min, max, _) = self.range();
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
    fn at_fraction(self, fraction: f64) -> f64 {
        let (min, max, _) = self.range();
        min + (max - min) * fraction.clamp(0.0, 1.0)
    }
    fn parse(self, text: &str) -> Option<f64> {
        text.split_whitespace()
            .next()?
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }
}
impl WaveBob {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn panel_segments(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        layout: Layout,
        id: &str,
        label: &str,
        rect: Rect,
        items: &[(i32, &str)],
    ) {
        let mut value = match instance.parameters[id] {
            processing::ParameterValue::Integer(v) => v,
            _ => return,
        };
        layout.text(
            ui,
            (rect.left() - layout.origin.x) / layout.scale,
            (rect.top() - layout.origin.y) / layout.scale - 17.0,
            label,
            14.0,
            TEXT,
        );
        let mut response = ui.interact(
            Rect::NOTHING,
            ui.scope_id().with(("segments", id)),
            Sense::hover(),
        );
        for (i, (raw, label)) in items.iter().enumerate() {
            let r = Rect::from_min_size(
                rect.min + Vec2::new(rect.width() * i as f32 / items.len() as f32, 0.0),
                Vec2::new(rect.width() / items.len() as f32, rect.height()),
            );
            let mut choice = segment(ui, r, (id, i), label, value == *raw, layout.scale);
            if choice.clicked() {
                value = *raw;
                choice.mark_changed();
            }
            response |= choice;
        }
        response = integer_menu(ui, instance, id, &mut value, response);
        self.edit_native_control(
            window,
            &instance.id,
            native,
            (id.into(), processing::ParameterValue::Integer(value), false),
            &response,
            ui.ctx(),
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn panel_dial<C: Dial>(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        layout: Layout,
        id: &str,
        control: C,
        x: f32,
        y: f32,
        radius: f32,
    ) {
        let raw = match instance.parameters[id] {
            processing::ParameterValue::Integer(v) => v,
            _ => return,
        };
        let mut value = raw as f64;
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(layout.canvas));
        let response = dial(
            &mut child,
            layout.p(x, y),
            radius * layout.scale,
            layout.scale,
            control,
            &mut value,
        );
        if response.hovered() || response.has_focus() {
            native.help = Some(format!(
                "{}: {} · raw {} · Shift fine · arrows · double-click reset",
                control.label(),
                control.text(value),
                raw
            ));
        }
        self.edit_native_control(
            window,
            &instance.id,
            native,
            (
                id.into(),
                processing::ParameterValue::Integer(value.round() as i32),
                false,
            ),
            &response,
            ui.ctx(),
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn panel_choice(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        layout: Layout,
        id: &str,
        label: &str,
        rect: Rect,
        items: &[(i32, &str)],
    ) {
        let value = match instance.parameters[id] {
            processing::ParameterValue::Integer(v) => v,
            _ => return,
        };
        self.panel_choice_selected(
            ui, window, instance, native, layout, id, label, rect, items, value,
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn panel_choice_selected(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        layout: Layout,
        id: &str,
        label: &str,
        rect: Rect,
        items: &[(i32, &str)],
        selected: i32,
    ) {
        let raw = match instance.parameters[id] {
            processing::ParameterValue::Integer(v) => v,
            _ => return,
        };
        let mut value = raw;
        let mut selection = selected;
        layout.text(
            ui,
            (rect.left() - layout.origin.x) / layout.scale,
            (rect.top() - layout.origin.y) / layout.scale - 12.0,
            label,
            13.0,
            TEXT,
        );
        let mut response = combo(ui, rect, id, &mut selection, items, layout.scale);
        if response.changed() {
            value = selection;
        }
        response = integer_menu(ui, instance, id, &mut value, response);
        if response.hovered() || response.has_focus() {
            native.help = Some(format!(
                "{} · right-click for exact raw value / reset",
                if label.is_empty() { id } else { label }
            ));
        }
        self.edit_native_control(
            window,
            &instance.id,
            native,
            (id.into(), processing::ParameterValue::Integer(value), false),
            &response,
            ui.ctx(),
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn panel_switch(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        layout: Layout,
        id: &str,
        label: &str,
        x: f32,
        y: f32,
    ) {
        let mut value = matches!(
            instance.parameters[id],
            processing::ParameterValue::Boolean(true)
        );
        layout.text(ui, x, y - 19.0, label, 13.0, TEXT);
        let response = toggle(
            ui,
            layout.r(x, y, 36.0, 22.0),
            id,
            label,
            &mut value,
            layout.scale,
        );
        self.edit_native_control(
            window,
            &instance.id,
            native,
            (id.into(), processing::ParameterValue::Boolean(value), false),
            &response,
            ui.ctx(),
        );
    }
    pub(super) fn panel_scene(
        &mut self,
        ui: &egui::Ui,
        native: &mut effects::NativeEditor,
        layout: Layout,
        source: Rect,
        revision: u64,
        scene: visualization::Scene,
    ) {
        if let Some(frame) = &mut native.visualization {
            frame.place(&ui.painter_at(layout.canvas));
            if let Err(error) = frame.submit(
                egui::Id::unique("effect-scene"),
                revision,
                source,
                source.expand(12.0 * layout.scale).intersect(layout.canvas),
                scene,
            ) {
                self.error = Some(error.into());
            }
        }
    }
    pub(super) fn effect_panel(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) -> (Rect, f32, Pos2) {
        self.effect_panel_layout(ui, window, instance, native, Vec2::new(850.0, 530.0), true)
    }
    pub(super) fn effect_panel_layout(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        base: Vec2,
        side_banks: bool,
    ) -> (Rect, f32, Pos2) {
        let size = ui.available_size().max(base * 0.7);
        let (canvas, _) = ui.allocate_exact_size(size, Sense::hover());
        let scale = (size.x / base.x).min(size.y / base.y);
        let origin = canvas.center() - base * scale * 0.5;
        let point = |x, y| origin + Vec2::new(x, y) * scale;
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0.0, BACKGROUND);
        painter.rect_filled(
            Rect::from_min_max(point(0.0, 0.0), point(base.x, 50.0)),
            0.0,
            Color32::from_rgb(19, 30, 39),
        );
        painter.line_segment(
            [point(0.0, 50.0), point(base.x, 50.0)],
            Stroke::new(scale, BORDER),
        );
        if side_banks {
            for (a, b) in [(0.0, 195.0), (base.x - 195.0, base.x)] {
                painter.rect_filled(
                    Rect::from_min_max(point(a, 51.0), point(b, base.y)),
                    0.0,
                    BANK,
                );
            }
        }
        painter.text(
            point(14.0, 27.0),
            egui::Align2::LEFT_CENTER,
            instance.kind.label(),
            font(24.0 * scale, true),
            TEXT,
        );
        let divider = 14.0
            + painter
                .layout_no_wrap(instance.kind.label().into(), font(24.0 * scale, true), TEXT)
                .size()
                .x
                / scale
            + 20.0;
        painter.line_segment(
            [point(divider, 18.0), point(divider, 37.0)],
            Stroke::new(scale, BORDER),
        );
        // Clip long owner names before the bypass button rather than overlap it.
        painter
            .with_clip_rect(Rect::from_min_max(
                point(divider + 18.0, 0.0),
                point(base.x - 185.0, 50.0),
            ))
            .text(
                point(divider + 18.0, 28.0),
                egui::Align2::LEFT_CENTER,
                self.host.owner_name(&window.owner),
                egui::FontId::proportional(13.0 * scale),
                MUTED,
            );
        let right = |x, y| point(x + base.x - 850.0, y);
        let bypass_rect = Rect::from_min_max(right(690.0, 14.0), right(795.0, 44.0));
        let bypass = ui.interact(
            bypass_rect,
            ui.scope_id().with("panel-bypass"),
            Sense::click(),
        );
        bypass.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Bypass")
        });
        let active = instance.bypassed;
        painter.rect(
            bypass_rect,
            5.0 * scale,
            if active {
                Color32::from_rgb(15, 66, 108)
            } else if bypass.hovered() {
                BANK
            } else {
                BACKGROUND
            },
            Stroke::new(
                scale,
                if active || bypass.has_focus() {
                    ACCENT
                } else {
                    MUTED
                },
            ),
            egui::StrokeKind::Inside,
        );
        let power = right(713.0, 28.5);
        let arc: Vec<_> = (0..=30)
            .map(|i| power + Vec2::angled(-1.05 + i as f32 / 30.0 * 5.24) * 7.0 * scale)
            .collect();
        painter.add(egui::Shape::line(arc, Stroke::new(1.3 * scale, TEXT)));
        painter.line_segment(
            [
                power - Vec2::new(0.0, 9.0) * scale,
                power - Vec2::new(0.0, 1.0) * scale,
            ],
            Stroke::new(1.3 * scale, TEXT),
        );
        painter.text(
            right(731.0, 28.5),
            egui::Align2::LEFT_CENTER,
            "Bypass",
            egui::FontId::proportional(14.0 * scale),
            TEXT,
        );
        if bypass.clicked() {
            self.finish_native_edit(native);
            self.processing_action(
                &window.owner,
                Action::Bypass {
                    effect_id: instance.id.clone(),
                    bypassed: !active,
                },
            );
        }
        let close_rect = Rect::from_center_size(right(825.0, 29.0), Vec2::splat(24.0 * scale));
        let close = ui.interact(
            close_rect,
            ui.scope_id().with("panel-close"),
            Sense::click(),
        );
        close.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                ui.is_enabled(),
                format!("Close {}", instance.kind.label()),
            )
        });
        for direction in [1.0, -1.0] {
            painter.line_segment(
                [
                    right(820.0, 29.0 - 5.0 * direction),
                    right(830.0, 29.0 + 5.0 * direction),
                ],
                Stroke::new(1.3 * scale, if close.hovered() { TEXT } else { MUTED }),
            );
        }
        if close.clicked() {
            self.dispatch(Command::CloseEffectEditor(EditorParams {
                owner: window.owner.clone(),
                instance_id: instance.id.clone(),
                expected_presentation_revision: None,
            }));
        }
        (canvas, scale, origin)
    }
}
pub(super) fn dial<C: Dial>(
    ui: &mut egui::Ui,
    centre: Pos2,
    radius: f32,
    scale: f32,
    control: C,
    value: &mut f64,
) -> egui::Response {
    let (minimum, maximum, step) = control.range();
    let old = *value;
    // One hit target includes the label, ring and value. The value text never
    // installs a competing DragValue handler over the middle of the dial.
    let rect = Rect::from_min_max(
        centre - Vec2::new(radius * 1.4, radius * 1.75),
        centre + Vec2::splat(radius * 1.4),
    );
    let radius = radius * DIAL_SCALE;
    let knob_scale = scale * DIAL_SCALE;
    let id = ui.scope_id().with("dial");
    let edit_id = id.with("entry");
    let editing = ui.data(|data| data.get_temp::<String>(edit_id));
    let mut response = ui
        .interact(rect, id, Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeVertical);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Slider, ui.is_enabled(), control.label())
    });
    if response.clicked() {
        response.request_focus();
    }
    let fine = ui.input(|i| if i.modifiers.shift { 0.1 } else { 1.0 });
    if response.drag_started() {
        ui.data_mut(|data| data.insert_temp(id.with("drag-fraction"), control.fraction(*value)));
    }
    if response.dragged() {
        let fraction = ui
            .data(|data| data.get_temp::<f64>(id.with("drag-fraction")))
            .unwrap_or_else(|| control.fraction(*value));
        // egui's drag_delta is incremental, not displacement from the press.
        // Keep the unquantized dial position between frames, even when callers
        // round the resulting parameter to a supported reference integer.
        let movement = if response.drag_started() {
            // Include movement accumulated before egui crossed its drag threshold.
            response.total_drag_delta().unwrap_or(response.drag_delta())
        } else {
            response.drag_delta()
        };
        let dy = movement.y as f64;
        if dy != 0.0 && editing.is_none() {
            let fraction = (fraction - dy / DIAL_TRAVEL * fine).clamp(0.0, 1.0);
            *value = control.at_fraction(fraction);
            ui.data_mut(|data| data.insert_temp(id.with("drag-fraction"), fraction));
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<f64>(id.with("drag-fraction")));
    }
    if response.has_focus() && !ui.ctx().text_edit_focused() {
        let delta = ui.input_mut(|i| {
            if i.modifiers.command || i.modifiers.ctrl || i.modifiers.alt {
                return 0;
            }
            let modifiers = i.modifiers;
            let up = i.consume_key(modifiers, egui::Key::ArrowUp)
                || i.consume_key(modifiers, egui::Key::ArrowRight);
            let down = i.consume_key(modifiers, egui::Key::ArrowDown)
                || i.consume_key(modifiers, egui::Key::ArrowLeft);
            i32::from(up) - i32::from(down)
        });
        if delta != 0 {
            *value = (*value + delta as f64 * step).clamp(minimum, maximum);
        }
    }
    let painter = ui.painter();
    painter.circle_filled(centre, radius, BACKGROUND);
    painter.circle_stroke(
        centre,
        radius,
        Stroke::new(
            1.7 * knob_scale,
            if response.has_focus() { ACCENT } else { BORDER },
        ),
    );
    painter.text(
        centre - Vec2::new(0.0, radius * 1.85),
        egui::Align2::CENTER_CENTER,
        control.label(),
        font(16.0 * scale, false),
        TEXT,
    );
    // Draw the unquantized position during a gesture, including idle frames.
    // A coarse reference parameter may round to the same value for many pixels.
    let fraction = if response.dragged() {
        ui.data(|data| data.get_temp::<f64>(id.with("drag-fraction")))
            .unwrap_or_else(|| control.fraction(*value))
    } else {
        control.fraction(*value)
    } as f32;
    let neutral = control.neutral().map(|v| control.fraction(v) as f32);
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    for tick in 0..=32 {
        let t = tick as f32 / 32.0;
        let direction = Vec2::angled(start + sweep * t);
        painter.line_segment(
            [
                centre + direction * radius * 1.13,
                centre + direction * radius * 1.24,
            ],
            Stroke::new(
                3.1 * knob_scale,
                if neutral.map_or(t <= fraction, |zero| {
                    t >= zero.min(fraction) && t <= zero.max(fraction)
                }) {
                    ACCENT
                } else {
                    TICK
                },
            ),
        );
    }
    if let Some(zero) = neutral {
        let direction = Vec2::angled(start + sweep * zero);
        painter.line_segment(
            [
                centre + direction * radius * 1.04,
                centre + direction * radius * 1.29,
            ],
            Stroke::new(1.3 * knob_scale, TEXT),
        );
    }
    let direction = Vec2::angled(start + sweep * fraction);
    painter.line_segment(
        [
            centre + direction * radius * 1.10,
            centre + direction * radius * 1.28,
        ],
        Stroke::new(8.0 * knob_scale, ACCENT),
    );
    let value_rect = Rect::from_center_size(centre, Vec2::new(radius * 1.9, 28.0 * scale));
    if response.double_clicked() {
        *value = control.reset();
        ui.data_mut(|data| data.remove::<String>(edit_id));
    } else {
        let begin_entry = response.clicked()
            && response
                .interact_pointer_pos()
                .is_some_and(|p| value_rect.contains(p))
            || (response.has_focus()
                && !ui.ctx().text_edit_focused()
                && ui.input(|i| i.key_pressed(egui::Key::F2)));
        if let Some(mut text) = editing.or_else(|| begin_entry.then(|| control.text(*value))) {
            let entry = ui.put(
                Rect::from_center_size(
                    centre,
                    Vec2::new((radius * 2.4).max(80.0 * scale), 26.0 * scale),
                ),
                egui::TextEdit::singleline(&mut text)
                    .id(edit_id)
                    .font(font(14.0 * scale, true))
                    .align(egui::Align2::CENTER_CENTER)
                    .clip_text(true),
            );
            if begin_entry {
                entry.request_focus();
                let mut state = egui::TextEdit::load_state(ui.ctx(), edit_id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::select_all_str(&text)));
                state.store(ui.ctx(), edit_id);
            }
            let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let reset = entry.double_clicked();
            let enter = !begin_entry && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let accept = entry.lost_focus() || enter;
            if reset || cancel || accept {
                if reset {
                    *value = control.reset();
                } else if !cancel && let Some(parsed) = control.parse(&text) {
                    *value = parsed.clamp(minimum, maximum);
                }
                ui.data_mut(|data| data.remove::<String>(edit_id));
                if reset || cancel || enter {
                    entry.surrender_focus();
                    response.request_focus();
                }
            } else {
                ui.data_mut(|data| data.insert_temp(edit_id, text));
            }
            response |= entry;
        } else {
            let text = control.text(*value);
            let mut size = control.value_font_size() * scale;
            let mut galley = painter.layout_no_wrap(text.clone(), font(size, true), TEXT);
            if galley.size().x > value_rect.width() {
                size *= value_rect.width() / galley.size().x;
                galley = painter.layout_no_wrap(text, font(size, true), TEXT);
            }
            painter.galley(centre - galley.size() * 0.5, galley, TEXT);
        }
    }
    // Editing the draft text alone must not start an audio preview/undo entry.
    response
        .flags
        .set(egui::response::Flags::CHANGED, *value != old);
    response
}
pub(super) fn segment(
    ui: &mut egui::Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    selected: bool,
    scale: f32,
) -> egui::Response {
    let response = ui.interact(rect, ui.scope_id().with(id), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::RadioButton, ui.is_enabled(), selected, label)
    });
    if response.clicked() {
        response.request_focus();
    }
    ui.painter().rect(
        rect,
        4.0 * scale,
        if selected { ACCENT } else { BACKGROUND },
        Stroke::new(
            scale,
            if response.hovered() || response.has_focus() {
                ACCENT
            } else {
                MUTED
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font(14.0 * scale, false),
        TEXT,
    );
    response
}
pub(super) fn toggle(
    ui: &mut egui::Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut bool,
    scale: f32,
) -> egui::Response {
    let mut response = ui.interact(rect, ui.scope_id().with(id), Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
        response.request_focus();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *value, label)
    });
    ui.painter().rect(
        rect,
        rect.height() * 0.5,
        if *value { ACCENT } else { TICK },
        Stroke::new(scale, if response.has_focus() { TEXT } else { BORDER }),
        egui::StrokeKind::Inside,
    );
    let radius = rect.height() * 0.39;
    let x = if *value {
        rect.right() - radius - 3.0 * scale
    } else {
        rect.left() + radius + 3.0 * scale
    };
    ui.painter()
        .circle_filled(Pos2::new(x, rect.center().y), radius, TEXT);
    response
}
pub(super) fn combo(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &str,
    value: &mut i32,
    items: &[(i32, &str)],
    scale: f32,
) -> egui::Response {
    let old = *value;
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(rect));
    child
        .style_mut()
        .text_styles
        .insert(egui::TextStyle::Button, font(14.0 * scale, false));
    child.visuals_mut().widgets.inactive.weak_bg_fill = BACKGROUND;
    child.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(scale, BORDER);
    child.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(scale, ACCENT);
    let text = items
        .iter()
        .find(|(v, _)| *v == *value)
        .map_or("—", |(_, s)| *s);
    let mut response = egui::ComboBox::from_id_salt(id)
        .width(rect.width())
        .wrap_mode(egui::TextWrapMode::Truncate)
        .selected_text(text)
        .show_ui(&mut child, |ui| {
            for (v, label) in items {
                ui.selectable_value(value, *v, *label);
            }
        })
        .response;
    if *value != old {
        response.mark_changed();
    }
    response
}

/// Keep arbitrary reference switch-region values enterable without permanent raw labels.
pub(super) fn integer_menu(
    _ui: &egui::Ui,
    instance: &Instance,
    id: &str,
    value: &mut i32,
    mut response: egui::Response,
) -> egui::Response {
    let old = *value;
    let mut entry = None;
    egui::Popup::context_menu(&response)
        .id(response.id.with("exact-raw-menu"))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            if let Some(parameter) = processing::catalogue()
                .into_iter()
                .find(|e| e.kind == instance.kind)
                .and_then(|e| e.parameters.into_iter().find(|p| p.id == id))
            {
                ui.label("Exact raw value");
                entry = Some(
                    ui.add(
                        egui::DragValue::new(value)
                            .range(parameter.minimum as i32..=parameter.maximum as i32)
                            .update_while_editing(false),
                    ),
                );
                if ui.button("Reset").clicked() {
                    if let processing::ParameterValue::Integer(default) = parameter.default {
                        *value = default;
                    }
                    ui.close();
                }
            }
        });
    if let Some(entry) = entry {
        response = entry;
    }
    if old != *value {
        response.mark_changed();
    }
    response
}
