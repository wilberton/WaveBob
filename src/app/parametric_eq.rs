// Copyright (C) 2026 ItsAlphaNeon (reference mappings)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Graph-led seven-band EQ, with exact reference units and sharp node hit targets.
use super::effect_panel::{self as panel, BACKGROUND, BANK, Dial, TEXT, dial, font};
use super::visualization::{Axis, Emission, Plot, Scene};
use super::*;
use wavebob::{
    dsp::parametric_eq as eq,
    processing::{self, Instance, ParameterValue},
};
const GRAPH_HELP: &str =
    "Drag: frequency / gain · Shift-drag or wheel: bandwidth · Alt-click: reset";
fn raw(instance: &Instance, id: &str) -> i32 {
    match instance.parameters[id] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!(),
    }
}
#[derive(Clone, Copy)]
pub(super) struct Control {
    pub slot: usize,
    pub band: usize,
    pub shape: i32,
}
impl Control {
    fn id(self) -> String {
        if self.slot == 3 {
            "main".into()
        } else {
            eq::parameter_id(["freq", "level", "width"][self.slot], self.band)
        }
    }
    fn value(self, instance: &Instance) -> f64 {
        let v = raw(instance, &self.id());
        match self.slot {
            0 => eq::frequency_hz(v),
            2 => v as f64 / 65536.0,
            _ => v as f64 / 100.0,
        }
    }
    fn raw(self, value: f64) -> i32 {
        match self.slot {
            0 => eq::frequency_raw(value),
            2 => (value * 65536.0).round_ties_even() as i32,
            _ => (value * 100.0).round_ties_even() as i32,
        }
    }
}
impl Dial for Control {
    fn label(self) -> &'static str {
        ["Frequency", "Gain", "Bandwidth", "Output"][self.slot]
    }
    fn range(self) -> (f64, f64, f64) {
        match self.slot {
            0 => (20.0, 20000.0, 1.0),
            2 => (0.0, 1.0, 1.0 / 65536.0),
            _ => (-18.0, 18.0, 0.01),
        }
    }
    fn reset(self) -> f64 {
        match self.slot {
            0 => eq::frequency_hz(eq::DEFAULT_FREQUENCIES[self.band]),
            2 => {
                if matches!(self.shape, 5 | 7) {
                    40124.0 / 65536.0
                } else if matches!(self.shape, 1 | 3) {
                    36068.0 / 65536.0
                } else {
                    25412.0 / 65536.0
                }
            }
            _ => 0.0,
        }
    }
    fn text(self, v: f64) -> String {
        match self.slot {
            0 => {
                if v >= 1000.0 {
                    format!("{:.2} kHz", v / 1000.0)
                } else {
                    format!("{v:.1} Hz")
                }
            }
            2 => format!("{v:.5}"),
            _ => format!("{v:+.2} dB"),
        }
    }
    fn value_font_size(self) -> f32 {
        if matches!(self.slot, 1 | 3) {
            15.0
        } else {
            16.0
        }
    }
    fn fraction(self, v: f64) -> f64 {
        if self.slot == 0 {
            (v.clamp(20.0, 20000.0) / 20.0).ln() / 1000.0_f64.ln()
        } else {
            let (a, b, _) = self.range();
            ((v - a) / (b - a)).clamp(0.0, 1.0)
        }
    }
    fn at_fraction(self, t: f64) -> f64 {
        if self.slot == 0 {
            20.0 * 1000.0_f64.powf(t.clamp(0.0, 1.0))
        } else {
            let (a, b, _) = self.range();
            a + (b - a) * t.clamp(0.0, 1.0)
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().to_lowercase().replace('−', "-");
        let factor = if self.slot == 0 && (text.ends_with("khz") || text.ends_with('k')) {
            1000.0
        } else if self.slot == 2 && text.ends_with('%') {
            0.01
        } else {
            1.0
        };
        text.trim_end_matches("khz")
            .trim_end_matches("hz")
            .trim_end_matches('k')
            .trim_end_matches("db")
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|v| v * factor)
    }
}
#[derive(Clone, Copy)]
struct Graph {
    rect: Rect,
    plot: Rect,
    scale: f32,
    rate: u32,
}
fn mapping(plot: Rect) -> Plot {
    Plot {
        rect: plot,
        x: Axis::logarithmic(20.0, 20000.0).unwrap(),
        y: Axis::linear(-18.0, 18.0).unwrap(),
    }
}
fn model_center(instance: &Instance, plot: Rect, band: usize) -> Pos2 {
    let shape = raw(instance, &eq::parameter_id("type", band));
    let db = if matches!(shape, 5..=7) {
        raw(instance, &eq::parameter_id("level", band)) as f64 / 100.0
    } else {
        0.0
    };
    mapping(plot).point(
        eq::frequency_hz(raw(instance, &eq::parameter_id("freq", band))),
        db,
    )
}
/// Deterministic sharp handles; leaders retain each displaced node's true anchor.
pub(super) fn node_centers(instance: &Instance, plot: Rect, scale: f32) -> [Pos2; 7] {
    let mut centers = [Pos2::ZERO; 7];
    for band in 0..7 {
        let anchor = model_center(instance, plot, band);
        let mut chosen = anchor;
        'search: for ring in 0..=4 {
            for direction in [
                Vec2::ZERO,
                Vec2::new(0.0, -1.0),
                Vec2::Y,
                -Vec2::X,
                Vec2::X,
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(-1.0, 1.0),
                Vec2::splat(1.0),
            ] {
                let offset = direction * (24.0 * ring as f32 * scale);
                let p = anchor + offset;
                if (offset == Vec2::ZERO || plot.contains(p))
                    && centers[..band]
                        .iter()
                        .all(|old| old.distance(p) >= 22.0 * scale)
                {
                    chosen = p;
                    break 'search;
                }
            }
        }
        centers[band] = chosen;
    }
    centers
}
pub(super) fn response_points(instance: &Instance, rate: u32, plot: Rect) -> Vec<Pos2> {
    let response = eq::Response::new(instance, rate);
    let map = mapping(plot);
    let max = 20000.0_f64.min(rate as f64 * 0.5 * (1.0 - 1e-6));
    (0..=512)
        .filter_map(|i| {
            let hz = 20.0 * 1000.0_f64.powf(i as f64 / 512.0);
            (hz <= max).then(|| {
                Pos2::new(
                    map.point(hz, 0.0).x,
                    plot.center().y - response.db(hz) as f32 / 36.0 * plot.height(),
                )
            })
        })
        .collect()
}
impl WaveBob {
    pub(super) fn parametric_eq_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let snapshot = self.host.snapshot();
        let rate = match &window.owner {
            processing::Owner::Document(id) => snapshot
                .documents
                .iter()
                .find(|d| &d.id == id)
                .map(|d| d.sample_rate),
            processing::Owner::Clip(id) => snapshot.arrangement.clip(id).and_then(|(_, c)| {
                snapshot
                    .documents
                    .iter()
                    .find(|d| d.id == c.document_id)
                    .map(|d| d.sample_rate)
            }),
        }
        .unwrap_or(48000);
        if !native.eq_initialized {
            native.eq_initialized = true;
            native.eq_band = 3;
        }
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (canvas, scale, origin) = self.effect_panel_layout(
                    ui,
                    window,
                    instance,
                    native,
                    Vec2::new(850.0, 530.0),
                    false,
                );
                let point = |x, y| origin + Vec2::new(x, y) * scale;
                let painter = ui.painter_at(canvas);
                self.eq_graph(
                    ui,
                    window,
                    instance,
                    native,
                    Graph {
                        rect: Rect::from_min_max(point(32.0, 51.0), point(840.0, 314.0)),
                        plot: Rect::from_min_max(point(56.0, 66.0), point(817.0, 277.0)),
                        scale,
                        rate,
                    },
                );
                painter.rect_filled(
                    Rect::from_min_max(point(0.0, 322.0), point(850.0, 530.0)),
                    0.0,
                    BANK,
                );
                for y in [322.0, 361.0, 511.0] {
                    painter
                        .line_segment([point(0.0, y), point(850.0, y)], Stroke::new(scale, BORDER));
                }
                painter.text(
                    point(16.0, 343.0),
                    egui::Align2::LEFT_CENTER,
                    "Band",
                    font(15.0 * scale, false),
                    TEXT,
                );
                for band in 0..7 {
                    let x = 75.0 + band as f32 * 50.0;
                    let response = panel::segment(
                        ui,
                        Rect::from_min_max(point(x, 330.0), point(x + 44.0, 354.0)),
                        ("eq-band", band),
                        &(band + 1).to_string(),
                        native.eq_band == band,
                        scale,
                    );
                    if response.clicked() {
                        self.finish_native_edit(native);
                        native.eq_band = band;
                    }
                }
                let band = native.eq_band.min(6);
                let shape = raw(instance, &eq::parameter_id("type", band));
                painter.text(
                    point(447.0, 343.0),
                    egui::Align2::LEFT_CENTER,
                    "Enabled",
                    font(14.0 * scale, false),
                    TEXT,
                );
                let id = eq::parameter_id("enabled", band);
                let mut enabled = matches!(instance.parameters[&id], ParameterValue::Boolean(true));
                let response = panel::toggle(
                    ui,
                    Rect::from_min_max(point(516.0, 332.0), point(554.0, 354.0)),
                    "eq-enable",
                    "Enabled",
                    &mut enabled,
                    scale,
                );
                self.edit_native_control(
                    window,
                    &instance.id,
                    native,
                    (id, ParameterValue::Boolean(enabled), false),
                    &response,
                    ui.ctx(),
                );
                for (slot, x) in [(0, 100.0), (1, 263.0), (2, 426.0), (3, 754.0)] {
                    let control = Control { slot, band, shape };
                    let mut value = control.value(instance);
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(("eq-dial", band, slot))
                            .max_rect(canvas),
                    );
                    let response = dial(
                        &mut child,
                        point(x, 450.0),
                        41.0 * scale,
                        scale,
                        control,
                        &mut value,
                    )
                    .on_hover_text(format!(
                        "{} · {} raw · double-click resets",
                        control.text(value),
                        raw(instance, &control.id())
                    ));
                    self.edit_native_control(
                        window,
                        &instance.id,
                        native,
                        (
                            control.id(),
                            ParameterValue::Integer(control.raw(value)),
                            false,
                        ),
                        &response,
                        ui.ctx(),
                    );
                }
                for x in [510.0, 660.0] {
                    painter.line_segment(
                        [point(x, 371.0), point(x, 508.0)],
                        Stroke::new(scale, BORDER),
                    );
                }
                for (group, label, y, items) in [
                    (
                        "type",
                        "Shape",
                        389.0,
                        eq::TYPE_NAMES
                            .iter()
                            .enumerate()
                            .map(|(i, n)| (i as i32, *n))
                            .collect::<Vec<_>>(),
                    ),
                    (
                        "order",
                        "Order",
                        454.0,
                        eq::ORDER_NAMES
                            .iter()
                            .enumerate()
                            .map(|(i, n)| (i as i32 - 3, *n))
                            .collect::<Vec<_>>(),
                    ),
                ] {
                    painter.text(
                        point(526.0, y),
                        egui::Align2::LEFT_CENTER,
                        label,
                        font(13.0 * scale, false),
                        TEXT,
                    );
                    let id = eq::parameter_id(group, band);
                    let mut value = raw(instance, &id);
                    let response = panel::combo(
                        ui,
                        Rect::from_min_max(point(526.0, y + 13.0), point(647.0, y + 40.0)),
                        group,
                        &mut value,
                        &items,
                        scale,
                    );
                    self.edit_native_control(
                        window,
                        &instance.id,
                        native,
                        (id, ParameterValue::Integer(value), false),
                        &response,
                        ui.ctx(),
                    );
                }
                painter.text(
                    point(14.0, 521.0),
                    egui::Align2::LEFT_CENTER,
                    native.help.as_deref().unwrap_or(GRAPH_HELP),
                    font(10.0 * scale, false),
                    MUTED,
                );
            });
    }
    fn eq_reset(
        &mut self,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        band: usize,
        response: &egui::Response,
        ctx: &egui::Context,
    ) {
        let shape = raw(instance, &eq::parameter_id("type", band));
        let mut changed = response.clone();
        changed.mark_changed();
        self.edit_native_parameters(
            window,
            &instance.id,
            native,
            vec![
                (
                    eq::parameter_id("level", band),
                    ParameterValue::Integer(0),
                    true,
                ),
                (
                    eq::parameter_id("freq", band),
                    ParameterValue::Integer(eq::DEFAULT_FREQUENCIES[band]),
                    true,
                ),
                (
                    eq::parameter_id("width", band),
                    ParameterValue::Integer(if matches!(shape, 5 | 7) {
                        40124
                    } else if matches!(shape, 1 | 3) {
                        36068
                    } else {
                        25412
                    }),
                    true,
                ),
            ],
            &changed,
            ctx,
        );
    }

    fn eq_graph(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        graph: Graph,
    ) {
        let Graph {
            rect,
            plot,
            scale,
            rate,
        } = graph;
        let mut response = ui.interact(
            rect,
            ui.scope_id().with("eq-graph"),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        let mut scene = Scene::default();
        let map = mapping(plot);
        let blue =
            Color32::from_rgba_unmultiplied(0, 153, 255, if instance.bypassed { 64 } else { 255 });
        for hz in [
            20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
        ] {
            let xx = map.point(hz, 0.0).x;
            scene.background.push(egui::Shape::line_segment(
                [Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())],
                Stroke::new(0.6 * scale, BORDER),
            ));
        }
        for db in [-18, -12, -6, 0, 6, 12, 18] {
            let yy = map.point(20.0, db as f64).y;
            scene.background.push(egui::Shape::line_segment(
                [Pos2::new(plot.left(), yy), Pos2::new(plot.right(), yy)],
                Stroke::new(
                    if db == 0 { scale } else { 0.5 * scale },
                    if db == 0 {
                        Color32::from_rgb(53, 82, 105)
                    } else {
                        BORDER
                    },
                ),
            ));
        }
        let transport = self.host.transport();
        let bins = spectrum(
            &transport.analyzer,
            transport.output.sample_rate.unwrap_or(rate),
        );
        let spectrum_points: Vec<_> = bins
            .iter()
            .enumerate()
            .map(|(i, db)| {
                Pos2::new(
                    plot.left() + plot.width() * i as f32 / 63.0,
                    plot.bottom() - plot.height() * 0.5 * ((db + 90.0) / 90.0).clamp(0.0, 1.0),
                )
            })
            .collect();
        let mut fill = egui::epaint::Mesh::default();
        for pair in spectrum_points.windows(2) {
            let base = fill.vertices.len() as u32;
            for (p, color) in [
                (pair[0], Color32::from_rgba_unmultiplied(77, 120, 154, 110)),
                (pair[1], Color32::from_rgba_unmultiplied(77, 120, 154, 110)),
                (
                    Pos2::new(pair[1].x, plot.bottom()),
                    Color32::from_rgba_unmultiplied(28, 57, 79, 35),
                ),
                (
                    Pos2::new(pair[0].x, plot.bottom()),
                    Color32::from_rgba_unmultiplied(28, 57, 79, 35),
                ),
            ] {
                fill.colored_vertex(p, color);
            }
            fill.add_triangle(base, base + 1, base + 2);
            fill.add_triangle(base, base + 2, base + 3);
        }
        scene.background.push(egui::Shape::mesh(fill));
        scene.path(
            spectrum_points,
            Stroke::new(0.7 * scale, Color32::from_rgb(65, 101, 130)),
            None,
        );
        let points = response_points(instance, rate, plot);
        scene.path(
            points.clone(),
            Stroke::new(1.6 * scale, blue),
            Some(Emission::new(blue, 1.6)),
        );
        scene.path(
            points,
            Stroke::new(
                0.55 * scale,
                Color32::from_rgba_unmultiplied(
                    139,
                    222,
                    255,
                    if instance.bypassed { 50 } else { 225 },
                ),
            ),
            None,
        );
        if (rate as f64 * 0.5) < 20000.0 {
            let xx = map.point(rate as f64 * 0.5, 0.0).x;
            scene.background.push(egui::Shape::rect_filled(
                Rect::from_min_max(Pos2::new(xx, plot.top()), plot.right_bottom()),
                0.0,
                Color32::from_rgba_unmultiplied(100, 110, 120, 35),
            ));
        }
        let centers = node_centers(instance, plot, scale);
        let mut nodes = Scene::default();
        for (band, pos) in centers.into_iter().enumerate() {
            let enabled = matches!(
                instance.parameters[&eq::parameter_id("enabled", band)],
                ParameterValue::Boolean(true)
            ) && raw(instance, &eq::parameter_id("type", band)) != 0;
            let selected = native.eq_band == band;
            let color = if enabled { blue } else { MUTED };
            let anchor = model_center(instance, plot, band);
            if pos.distance(anchor) > 1.0 {
                nodes.path([anchor, pos], Stroke::new(0.7 * scale, MUTED), None);
            }
            nodes.disc(
                pos,
                8.5 * scale,
                if selected && enabled {
                    blue
                } else {
                    BACKGROUND
                },
                if selected && enabled {
                    Some(Emission::new(blue, 0.6))
                } else {
                    None
                },
            );
            nodes.path(
                (0..=48).map(|i| {
                    pos + Vec2::angled(i as f32 / 48.0 * std::f32::consts::TAU) * 9.0 * scale
                }),
                Stroke::new(if selected { 1.7 * scale } else { scale }, color),
                None,
            );
        }
        if let Some(frame) = &mut native.visualization {
            frame.place(&painter);
            let revision = visualization::revision(&(instance, rate));
            let _ = frame.submit(
                egui::Id::unique("eq-response"),
                revision,
                plot,
                plot.expand(12.0 * scale),
                scene,
            );
            let _ = frame.submit(
                egui::Id::unique("eq-handles"),
                visualization::revision(&(instance, native.eq_band)),
                rect,
                rect,
                nodes,
            );
        }
        for hz in [
            20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
        ] {
            painter.text(
                Pos2::new(map.point(hz, 0.0).x, plot.bottom() + 13.0 * scale),
                egui::Align2::CENTER_CENTER,
                if hz >= 1000.0 {
                    format!("{}k", hz / 1000.0)
                } else {
                    format!("{hz:.0}")
                },
                font(10.0 * scale, false),
                MUTED,
            );
        }
        for db in [-18, -12, -6, 0, 6, 12, 18] {
            painter.text(
                Pos2::new(plot.left() - 10.0 * scale, map.point(20.0, db as f64).y),
                egui::Align2::RIGHT_CENTER,
                if db == 0 {
                    "0".into()
                } else {
                    format!("{db:+}")
                },
                font(10.0 * scale, false),
                MUTED,
            );
        }
        painter.text(
            plot.center_bottom() + Vec2::new(0.0, 28.0 * scale),
            egui::Align2::CENTER_CENTER,
            "Frequency (Hz)",
            font(11.0 * scale, false),
            MUTED,
        );
        painter.text(
            plot.right_bottom() + Vec2::new(-4.0, 33.0) * scale,
            egui::Align2::RIGHT_CENTER,
            "Spectrum · −90…0 dBFS",
            font(8.0 * scale, false),
            MUTED,
        );
        for (band, pos) in centers.into_iter().enumerate() {
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                (band + 1).to_string(),
                font(10.0 * scale, false),
                if instance.bypassed { MUTED } else { TEXT },
            );
        }
        if response.hovered() || response.has_focus() {
            native.help = Some(GRAPH_HELP.into());
        }
        let hit = native.eq_drag.map(|(band, _)| band).or_else(|| {
            response
                .interact_pointer_pos()
                .or_else(|| response.hover_pos())
                .and_then(|p| {
                    centers
                        .iter()
                        .enumerate()
                        .map(|(i, c)| (i, c.distance(p)))
                        .filter(|(_, d)| *d <= 16.0 * scale)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i)
                })
        });
        if let Some(band) = hit {
            native.help = Some(format!(
                "Band {} · {:.0} Hz · {:.2} dB · BW {:.5} · {GRAPH_HELP}",
                band + 1,
                eq::frequency_hz(raw(instance, &eq::parameter_id("freq", band))),
                raw(instance, &eq::parameter_id("level", band)) as f64 / 100.0,
                raw(instance, &eq::parameter_id("width", band)) as f64 / 65536.0
            ));
            if response.clicked() {
                native.eq_band = band;
                response.request_focus();
                if ui.input(|i| i.modifiers.alt) {
                    self.eq_reset(window, instance, native, band, &response, ui.ctx());
                }
            }
        }
        if response.drag_started() {
            // Drag recognition happens after the pointer leaves the token's
            // small hit circle. Resolve the band at the original press point.
            let pressed = ui.input(|i| i.pointer.press_origin()).and_then(|p| {
                centers
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i, c.distance(p)))
                    .filter(|(_, d)| *d <= 16.0 * scale)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            });
            native.eq_drag = pressed.map(|band| (band, ui.input(|i| i.modifiers.shift)));
            if let Some((band, _)) = native.eq_drag {
                native.eq_band = band;
            }
        }
        if let Some((band, width)) = native.eq_drag {
            let mut changes = Vec::new();
            if response.dragged() {
                let delta = ui.input(|i| i.pointer.delta());
                if width {
                    let value = (raw(instance, &eq::parameter_id("width", band)) as f32
                        - delta.y * 65536.0 / (160.0 * scale))
                        .round()
                        .clamp(0.0, 65536.0) as i32;
                    changes.push((
                        eq::parameter_id("width", band),
                        ParameterValue::Integer(value),
                        true,
                    ));
                } else {
                    let freq = (raw(instance, &eq::parameter_id("freq", band)) as f32
                        + delta.x * 65536.0 / plot.width())
                    .round()
                    .clamp(0.0, 65536.0) as i32;
                    changes.push((
                        eq::parameter_id("freq", band),
                        ParameterValue::Integer(freq),
                        true,
                    ));
                    let shape = raw(instance, &eq::parameter_id("type", band));
                    if matches!(shape, 5..=7) {
                        let gain = (raw(instance, &eq::parameter_id("level", band)) as f32
                            - delta.y * 3600.0 / plot.height())
                        .round()
                        .clamp(-1800.0, 1800.0) as i32;
                        changes.push((
                            eq::parameter_id("level", band),
                            ParameterValue::Integer(gain),
                            true,
                        ));
                    }
                }
                response.mark_changed();
            }
            if changes.is_empty() {
                changes.push((
                    eq::parameter_id("freq", band),
                    instance.parameters[&eq::parameter_id("freq", band)].clone(),
                    true,
                ));
            }
            self.edit_native_parameters(window, &instance.id, native, changes, &response, ui.ctx());
            if response.drag_stopped() {
                native.eq_drag = None;
            }
        }
        if response.secondary_clicked()
            && let Some(band) = hit
        {
            native.eq_band = band;
        }
        response.context_menu(|ui| {
            let band = native.eq_band.min(6);
            ui.label(format!("Band {}", band + 1));
            let reset = ui.button("Reset band");
            if reset.clicked() {
                self.eq_reset(window, instance, native, band, &reset, ui.ctx());
                ui.close();
            }
            for (group, heading, names, offset) in [
                ("type", "Shape", eq::TYPE_NAMES.as_slice(), 0),
                ("order", "Order", eq::ORDER_NAMES.as_slice(), -3),
            ] {
                ui.menu_button(heading, |ui| {
                    let id = eq::parameter_id(group, band);
                    let current = raw(instance, &id);
                    for (index, name) in names.iter().enumerate() {
                        let value = index as i32 + offset;
                        let mut selected = ui.selectable_label(current == value, *name);
                        if selected.clicked() {
                            selected.mark_changed();
                            self.edit_native_control(
                                window,
                                &instance.id,
                                native,
                                (id.clone(), ParameterValue::Integer(value), false),
                                &selected,
                                ui.ctx(),
                            );
                            ui.close();
                        }
                    }
                });
            }
        });
        if response.hovered()
            && let Some(band) = hit
        {
            let scroll = ui.input_mut(|i| {
                // Smoothing must not turn one wheel event into repeated edits.
                let mut ticks = 0.0;
                for event in &mut i.events {
                    if let egui::Event::MouseWheel { delta, .. } = event
                        && delta.y != 0.0
                    {
                        ticks += delta.y.signum();
                        delta.y = 0.0;
                    }
                }
                i.smooth_scroll_delta.y = 0.0;
                ticks
            });
            if scroll != 0.0 {
                let value = (raw(instance, &eq::parameter_id("width", band))
                    + (scroll * 3277.0).round() as i32)
                    .clamp(0, 65536);
                response.mark_changed();
                self.edit_native_control(
                    window,
                    &instance.id,
                    native,
                    (
                        eq::parameter_id("width", band),
                        ParameterValue::Integer(value),
                        false,
                    ),
                    &response,
                    ui.ctx(),
                );
            }
        }
    }
}
fn spectrum(frames: &[[f32; 2]], rate: u32) -> [f32; 64] {
    std::array::from_fn(|bin| {
        let hz = 20.0 * 1000.0_f64.powf(bin as f64 / 63.0);
        let n = frames.len().min(128);
        if n < 2 || hz >= rate as f64 * 0.5 {
            return -90.0;
        }
        let (mut real, mut imaginary, mut right_real, mut right_imaginary) = (0.0, 0.0, 0.0, 0.0);
        for (i, frame) in frames.iter().take(n).enumerate() {
            let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos();
            let angle = std::f64::consts::TAU * hz * i as f64 / rate as f64;
            let sample = frame[0] as f64 * window;
            real += sample * angle.cos();
            imaginary -= sample * angle.sin();
            right_real += frame[1] as f64 * window * angle.cos();
            right_imaginary -= frame[1] as f64 * window * angle.sin();
        }
        (20.0
            * (((real * real
                + imaginary * imaginary
                + right_real * right_real
                + right_imaginary * right_imaginary)
                * 0.5)
                .sqrt()
                * 4.0
                / n as f64)
                .max(1e-12)
                .log10()) as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_spectrum_preserves_anti_phase_stereo_energy_and_handles_silence() {
        let same: Vec<_> = (0..128)
            .map(|i| {
                let v = (i as f32 * std::f32::consts::TAU * 2000.0 / 48000.0).sin() * 0.5;
                [v, v]
            })
            .collect();
        let opposite: Vec<_> = same.iter().map(|v| [v[0], -v[1]]).collect();
        assert_eq!(spectrum(&same, 48000), spectrum(&opposite, 48000));
        assert!(spectrum(&same, 48000).into_iter().all(|v| v.is_finite()));
        assert!(
            spectrum(&[[0.0; 2]; 128], 48000)
                .into_iter()
                .all(|v| v <= -90.0)
        );
    }
}
