// Copyright (C) 2026 ItsAlphaNeon (reference grouping and meter display laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Dynamics pages and bounded history of the actual instance's meters.
use super::*;
use std::collections::VecDeque;
use wavebob::{
    dsp::limiter,
    metering::{Column, History},
    processing::{Instance, ParameterValue},
};
pub(super) struct View {
    page: usize,
    layers: [bool; 4],
    seconds: f32,
    history: VecDeque<Column>,
    sequence: u64,
    pending: Column,
    pending_seconds: f64,
    compare: Option<([i32; 18], bool)>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            page: 0,
            layers: [true; 4],
            seconds: 2.5,
            history: VecDeque::with_capacity(512),
            sequence: 0,
            pending: Column {
                gain: f32::MAX,
                ..Default::default()
            },
            pending_seconds: 0.0,
            compare: None,
        }
    }
}
impl View {
    fn consume(&mut self, history: &History) {
        for c in &history.columns {
            if c.sequence <= self.sequence {
                continue;
            }
            if self.sequence != 0 && c.sequence > self.sequence + 1 {
                self.history.clear();
                self.pending_seconds = 0.0;
                self.pending = Column {
                    gain: f32::MAX,
                    ..Default::default()
                };
            }
            self.sequence = c.sequence;
            let p = &mut self.pending;
            p.input = p.input.max(c.levels.input);
            p.output = p.output.max(c.levels.output);
            p.level = p.level.max(c.levels.level);
            p.gain = p.gain.min(c.levels.gain);
            self.pending_seconds += c.frames as f64 / c.sample_rate.max(1) as f64;
            let bins = (self.pending_seconds / 0.01).floor() as usize;
            if bins > 0 {
                if bins > 512 {
                    self.history.clear();
                }
                for _ in 0..bins.min(512) {
                    if self.history.len() == 512 {
                        self.history.pop_front();
                    }
                    self.history.push_back(self.pending);
                }
                self.pending_seconds %= 0.01;
                self.pending = if self.pending_seconds > 0.0 {
                    c.levels
                } else {
                    Column {
                        gain: f32::MAX,
                        ..Default::default()
                    }
                };
            }
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Control(pub usize);
impl effect_panel::Dial for Control {
    fn label(self) -> &'static str {
        [
            "Gain",
            "Saturation",
            "Ceiling",
            "Attack",
            "Attack curve",
            "Release",
            "Release curve",
            "Peak window",
            "Threshold",
            "Ratio",
            "Knee",
            "Attack",
            "Release",
            "Envelope curve",
            "RMS window",
            "Noise gain",
            "Noise threshold",
            "Noise release",
        ][self.0]
    }
    fn range(self) -> (f64, f64, f64) {
        let (a, b) = limiter::RANGES[self.0];
        (a as f64, b as f64, 1.0)
    }
    fn reset(self) -> f64 {
        limiter::DEFAULTS[self.0] as f64
    }
    fn value_font_size(self) -> f32 {
        if matches!(self.0, 0 | 1 | 2 | 7) {
            18.0
        } else {
            15.0
        }
    }
    fn text(self, value: f64) -> String {
        let text = limiter::value_text(self.0, value.round() as i32);
        let text = text.split(" · ").next().unwrap().to_owned();
        if text.ends_with(" ms") {
            let ms = text.trim_end_matches(" ms").parse::<f64>().unwrap();
            if ms >= 1000.0 {
                format!("{:.2} s", ms / 1000.0)
            } else {
                format!("{ms:.1} ms")
            }
        } else {
            text
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().replace('−', "-");
        if let Some(raw) = text.strip_prefix("raw ") {
            return raw.parse::<f64>().ok().filter(|v| v.is_finite());
        }
        if matches!(self.0, 0..=2 | 8 | 15 | 16)
            && (text.starts_with("-∞") || text.starts_with("-inf"))
        {
            return Some(0.0);
        }
        if self.0 == 9 {
            let (a, b) = text.split_once(':')?;
            let a = a.trim().parse::<f64>().ok()?;
            let b = b.trim().parse::<f64>().ok()?;
            if !a.is_finite() || !b.is_finite() || a < 1.0 || b < 1.0 {
                return None;
            }
            return if b == 1.0 {
                Some((((a - 1.0) / 0.38 + 1.0).ln() / 51.0_f64.ln()) * 1000.0)
            } else if a == 1.0 {
                Some(-(((b - 1.0) / 0.06 + 1.0).ln() / 51.0_f64.ln()) * 1000.0)
            } else {
                None
            };
        }
        let n = text
            .split_whitespace()
            .next()?
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())?;
        Some(match self.0 {
            0..=2 | 8 | 15 | 16 => {
                let base: f64 = if matches!(self.0, 1 | 2) { 3.0 } else { 7.0 };
                let amplitude = 10.0_f64.powf(n.clamp(-120.0, 60.0) / 20.0);
                (amplitude * (base - 1.0) + 1.0).ln() / base.ln() * 1000.0
            }
            3 | 5 | 7 | 11 | 12 | 14 | 17 => {
                let ms = if text.ends_with(" ms") {
                    n
                } else if text.ends_with(" s") {
                    n * 1000.0
                } else {
                    n
                };
                let denominator = if matches!(self.0, 5 | 12 | 17) {
                    4.0
                } else {
                    2.0
                };
                (ms.max(0.0) / denominator + 1.0).ln() / 501.0_f64.ln() * 10000.0
            }
            10 => n * 10.0,
            _ => n,
        })
    }
}
impl WaveBob {
    pub(super) fn limiter_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        use effect_panel::{self as panel, Layout};
        let values = limiter::values(instance);
        let mut view = native.limiter_view.take().unwrap_or_default();
        let transport = self.host.transport();
        let meter = transport
            .effect_meters
            .iter()
            .find(|m| m.instance_id == instance.id);
        if let Some(meter) = meter {
            view.consume(meter);
        }
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let l = Layout::new(self.effect_panel_layout(
                    ui,
                    window,
                    instance,
                    native,
                    Vec2::new(850.0, 530.0),
                    false,
                ));
                for x in [0.0, 690.0] {
                    ui.painter()
                        .rect_filled(l.r(x, 51.0, 160.0, 479.0), 0.0, panel::BANK);
                }
                for (page, label) in ["Limit", "Comp"].into_iter().enumerate() {
                    if panel::segment(
                        ui,
                        l.r(174.0 + page as f32 * 75.0, 63.0, 75.0, 25.0),
                        ("limiter-page", page),
                        label,
                        view.page == page,
                        l.scale,
                    )
                    .clicked()
                    {
                        self.finish_native_edit(native);
                        view.page = page;
                    }
                }
                if action(ui, l.r(555.0, 63.0, 58.0, 25.0), "Store", l.scale).clicked() {
                    self.finish_native_edit(native);
                    view.compare = Some((
                        values,
                        matches!(
                            instance.parameters["exact_ceiling"],
                            ParameterValue::Boolean(true)
                        ),
                    ));
                }
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("limiter-flip")
                        .max_rect(l.canvas),
                );
                if view.compare.is_none() {
                    child.disable();
                }
                let mut flip = action(&mut child, l.r(623.0, 63.0, 53.0, 25.0), "Flip", l.scale);
                if flip.clicked() {
                    self.finish_native_edit(native);
                    let (bank, exact) = view
                        .compare
                        .replace((
                            values,
                            matches!(
                                instance.parameters["exact_ceiling"],
                                ParameterValue::Boolean(true)
                            ),
                        ))
                        .unwrap();
                    flip.mark_changed();
                    let changes = bank
                        .into_iter()
                        .enumerate()
                        .map(|(i, value)| {
                            (limiter::IDS[i].into(), ParameterValue::Integer(value), true)
                        })
                        .chain(std::iter::once((
                            "exact_ceiling".into(),
                            ParameterValue::Boolean(exact),
                            true,
                        )))
                        .collect();
                    self.edit_native_parameters(
                        window,
                        &instance.id,
                        native,
                        changes,
                        &flip,
                        ui.ctx(),
                    );
                }
                if let Some(frame) = &mut native.visualization {
                    frame.place(&ui.painter_at(l.canvas));
                }
                let (source, scene) = history_scene(ui, l, &view, &values, instance.bypassed);
                self.panel_scene(
                    ui,
                    native,
                    l,
                    source,
                    visualization::revision(&(
                        &view.history,
                        view.seconds,
                        view.layers,
                        values,
                        instance.bypassed,
                    )),
                    scene,
                );
                for (i, label) in ["Input", "Output", "Envelopes", "Markers"]
                    .into_iter()
                    .enumerate()
                {
                    ui.put(
                        l.r(
                            176.0 + [0.0, 64.0, 137.0, 230.0][i],
                            306.0,
                            [64.0, 73.0, 93.0, 80.0][i],
                            22.0,
                        ),
                        egui::Checkbox::new(
                            &mut view.layers[i],
                            RichText::new(label).font(panel::font(11.0 * l.scale, false)),
                        ),
                    );
                }
                let mut history = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("limiter-history")
                        .max_rect(l.r(600.0, 304.0, 75.0, 25.0)),
                );
                history
                    .add(
                        egui::DragValue::new(&mut view.seconds)
                            .range(0.25..=5.0)
                            .speed(0.05)
                            .suffix(" s")
                            .max_decimals(2),
                    )
                    .on_hover_text("History span · 0.25..5 seconds · presentation only");
                for (index, x, y, r) in [
                    (0, 82.0, 183.0, 43.0),
                    (1, 82.0, 374.0, 43.0),
                    (2, 768.0, 183.0, 43.0),
                    (7, 768.0, 374.0, 43.0),
                ] {
                    self.panel_dial(
                        ui,
                        window,
                        instance,
                        native,
                        l,
                        limiter::IDS[index],
                        Control(index),
                        x,
                        y,
                        r,
                    );
                }
                let indices = if view.page == 0 {
                    [3, 5, 16]
                } else {
                    [8, 9, 10]
                };
                for (slot, index) in indices.into_iter().enumerate() {
                    self.panel_dial(
                        ui,
                        window,
                        instance,
                        native,
                        l,
                        limiter::IDS[index],
                        Control(index),
                        270.0 + slot as f32 * 160.0,
                        412.0,
                        33.0,
                    );
                }
                ui.painter().line_segment(
                    [l.p(165.0, 467.0), l.p(685.0, 467.0)],
                    Stroke::new(l.scale, BORDER),
                );
                if view.page == 0 {
                    for (index, x, width) in [
                        (4, 174.0, 72.0),
                        (6, 273.0, 72.0),
                        (15, 372.0, 76.0),
                        (17, 471.0, 76.0),
                    ] {
                        self.limiter_number(
                            ui,
                            window,
                            instance,
                            native,
                            l,
                            Control(index),
                            l.r(x, 492.0, width, 22.0),
                        );
                    }
                } else {
                    for (index, x, width) in [
                        (11, 174.0, 72.0),
                        (12, 273.0, 72.0),
                        (14, 372.0, 76.0),
                        (13, 471.0, 76.0),
                    ] {
                        self.limiter_number(
                            ui,
                            window,
                            instance,
                            native,
                            l,
                            Control(index),
                            l.r(x, 492.0, width, 22.0),
                        );
                    }
                    let gate = action(ui, l.r(335.0, 63.0, 63.0, 25.0), "Gate…", l.scale);
                    egui::Popup::menu(&gate)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(390.0, 180.0) * l.scale,
                                Sense::hover(),
                            );
                            let popup = Layout {
                                canvas: rect,
                                scale: l.scale,
                                origin: rect.min,
                            };
                            for (slot, index) in [16, 15, 17].into_iter().enumerate() {
                                self.panel_dial(
                                    ui,
                                    window,
                                    instance,
                                    native,
                                    popup,
                                    limiter::IDS[index],
                                    Control(index),
                                    65.0 + slot as f32 * 130.0,
                                    108.0,
                                    35.0,
                                );
                            }
                        });
                }
                self.panel_switch(
                    ui,
                    window,
                    instance,
                    native,
                    l,
                    "exact_ceiling",
                    "Exact ceiling",
                    582.0,
                    491.0,
                );
                if ui.rect_contains_pointer(l.r(576.0,469.0,108.0,46.0)) || ui.memory(|m|m.has_focus(ui.scope_id().with("exact_ceiling"))) {
                    native.help=Some("Exact ceiling removes the default 0.975 safety margin · internal stereo-linked detector".into());
                }
                let status = if instance.bypassed {
                    "Bypassed · internal processing history"
                } else if transport.meters_truncated && meter.is_none() {
                    "Meter capacity reached"
                } else if !transport.requested.playing {
                    "History paused · input/output dBFS · detector dB · GR 0…24 dB"
                } else if meter.is_none() {
                    "No live observations from this instance"
                } else {
                    "Input/output dBFS · detector dB · gain reduction 0…24 dB · 10 ms bins"
                };
                l.footer(ui, native, status);
            });
        native.limiter_view = Some(view);
    }
    #[allow(clippy::too_many_arguments)]
    fn limiter_number(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        l: effect_panel::Layout,
        control: Control,
        rect: Rect,
    ) {
        use effect_panel::Dial;
        let id = limiter::IDS[control.0];
        let mut value = limiter::values(instance)[control.0] as f64;
        let (min, max, _) = control.range();
        l.text(
            ui,
            (rect.left() - l.origin.x) / l.scale,
            (rect.top() - l.origin.y) / l.scale - 13.0,
            control.label(),
            11.0,
            effect_panel::TEXT,
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("limiter-field", id))
                .max_rect(rect),
        );
        child.style_mut().text_styles.insert(
            egui::TextStyle::Button,
            effect_panel::font(12.0 * l.scale, false),
        );
        let mut response = child.add(
            egui::DragValue::new(&mut value)
                .range(min..=max)
                .speed(if ui.input(|i| i.modifiers.shift) {
                    0.1
                } else {
                    1.0
                })
                .update_while_editing(false)
                .custom_formatter(move |v, _| control.text(v))
                .custom_parser(move |text| control.parse(text)),
        );
        if response.double_clicked()
            || (response.hovered()
                && ui.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                }))
        {
            response.surrender_focus();
            value = control.reset();
            response.mark_changed();
        }
        if response.hovered() || response.has_focus() {
            native.help = Some(format!(
                "{}: {} · raw {} · raw N for exact raw entry · double-click resets",
                control.label(),
                control.text(value),
                limiter::values(instance)[control.0]
            ));
        }
        self.edit_native_control(
            window,
            &instance.id,
            native,
            (
                id.into(),
                ParameterValue::Integer(value.round() as i32),
                false,
            ),
            &response,
            ui.ctx(),
        );
    }
}
fn action(ui: &mut egui::Ui, rect: Rect, label: &str, scale: f32) -> egui::Response {
    ui.put(
        rect,
        egui::Button::new(RichText::new(label).font(effect_panel::font(14.0 * scale, false)))
            .fill(effect_panel::BACKGROUND)
            .stroke(Stroke::new(scale, BORDER))
            .corner_radius(4.0 * scale),
    )
}
pub(super) fn level_db(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        20.0 * value.log10()
    } else {
        -120.0
    }
}
fn history_scene(
    ui: &egui::Ui,
    l: effect_panel::Layout,
    view: &View,
    values: &[i32; 18],
    bypassed: bool,
) -> (Rect, visualization::Scene) {
    let plot = l.r(201.0, 103.0, 475.0, 147.0);
    let gr = l.r(201.0, 260.0, 475.0, 25.0);
    let upper = view
        .history
        .iter()
        .flat_map(|c| [c.input, c.output, c.level])
        .map(level_db)
        .chain(std::iter::once(level_db(limiter::amplitude(2, values[2]))))
        .fold(6.0_f32, f32::max)
        .clamp(6.0, 120.0);
    let upper = (upper / 6.0).ceil() * 6.0;
    let y =
        |db: f32| plot.bottom() - (db.clamp(-60.0, upper) + 60.0) / (upper + 60.0) * plot.height();
    let mut scene = visualization::Scene::default();
    let step = if upper <= 24.0 {
        12
    } else if upper <= 60.0 {
        24
    } else {
        36
    };
    for db in (-60..upper as i32).step_by(step) {
        let yy = y(db as f32);
        scene.path(
            [Pos2::new(plot.left(), yy), Pos2::new(plot.right(), yy)],
            Stroke::new(l.scale, BORDER),
            None,
        );
        ui.painter().text(
            Pos2::new(plot.left() - 5.0 * l.scale, yy),
            egui::Align2::RIGHT_CENTER,
            db.to_string(),
            effect_panel::font(10.0 * l.scale, false),
            MUTED,
        );
    }
    ui.painter().text(
        Pos2::new(plot.left() - 5.0 * l.scale, plot.top()),
        egui::Align2::RIGHT_CENTER,
        format!("+{upper:.0}"),
        effect_panel::font(10.0 * l.scale, false),
        MUTED,
    );
    for i in 0..=4 {
        let x = plot.left() + i as f32 * plot.width() / 4.0;
        scene.path(
            [Pos2::new(x, plot.top()), Pos2::new(x, gr.bottom())],
            Stroke::new(l.scale, BORDER),
            None,
        );
        ui.painter().text(
            Pos2::new(x, gr.bottom() + 9.0 * l.scale),
            if i == 0 {
                egui::Align2::LEFT_CENTER
            } else if i == 4 {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::CENTER_CENTER
            },
            format!(
                "{:.2} s",
                if i == 4 {
                    0.0
                } else {
                    -view.seconds * (1.0 - i as f32 / 4.0)
                }
            ),
            effect_panel::font(10.0 * l.scale, false),
            MUTED,
        );
    }
    let count = ((view.seconds / 0.01).round() as usize).clamp(1, 512);
    let columns: Vec<_> = view.history.iter().rev().take(count).rev().collect();
    let n = columns.len();
    let x = |i: usize| plot.right() - (n - 1 - i) as f32 / count as f32 * plot.width();
    for (index, color) in [
        (0, Color32::from_rgb(111, 133, 153)),
        (1, effect_panel::ACCENT),
        (2, Color32::from_rgb(0, 206, 245)),
    ] {
        if !view.layers[index] {
            continue;
        }
        let color = if bypassed {
            color.gamma_multiply(0.35)
        } else {
            color
        };
        let points = columns.iter().enumerate().map(|(i, c)| {
            Pos2::new(
                x(i),
                y(level_db(match index {
                    0 => c.input,
                    1 => c.output,
                    _ => c.level,
                })),
            )
        });
        scene.path(
            points,
            Stroke::new(if index == 1 { 1.7 * l.scale } else { l.scale }, color),
            (index != 0).then_some(visualization::Emission::new(
                color,
                if index == 1 { 1.7 } else { 0.8 },
            )),
        );
    }
    if view.layers[2] {
        let points: Vec<_> = columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                Pos2::new(
                    x(i),
                    gr.top() + (-level_db(c.gain)).clamp(0.0, 24.0) / 24.0 * gr.height(),
                )
            })
            .collect();
        for pair in points.windows(2) {
            scene.fill(
                [
                    Pos2::new(pair[0].x, gr.top()),
                    pair[0],
                    pair[1],
                    Pos2::new(pair[1].x, gr.top()),
                ],
                Color32::from_rgba_unmultiplied(0, 110, 230, 40),
                None,
            );
        }
        let color =
            Color32::from_rgb(109, 183, 255).gamma_multiply(if bypassed { 0.35 } else { 1.0 });
        scene.path(
            points,
            Stroke::new(l.scale, color),
            Some(visualization::Emission::new(color, 0.5)),
        );
    }
    if view.layers[3] {
        for (index, label) in [(2, "Ceil"), (8, "Comp")] {
            if index == 8 && values[8] == 1000 {
                continue;
            }
            let db = level_db(limiter::amplitude(index, values[index]));
            if !(-60.0..=upper).contains(&db) {
                continue;
            }
            let yy = y(db);
            scene.path(
                [Pos2::new(plot.left(), yy), Pos2::new(plot.right(), yy)],
                Stroke::new(l.scale, Color32::from_rgb(69, 137, 205)),
                None,
            );
            ui.painter().text(
                Pos2::new(plot.left() + 4.0 * l.scale, yy - 2.0 * l.scale),
                egui::Align2::LEFT_BOTTOM,
                format!("{label} {db:.1} dB"),
                effect_panel::font(10.0 * l.scale, false),
                MUTED,
            );
        }
    }
    l.text(ui, 204.0, 273.0, "GR 0…24 dB", 9.0, MUTED);
    if view.history.is_empty() {
        ui.painter().text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "No observations",
            effect_panel::font(12.0 * l.scale, false),
            MUTED,
        );
    }
    (Rect::from_min_max(plot.min, gr.max), scene)
}
#[cfg(test)]
mod tests {
    use super::*;
    use wavebob::metering::Observation;
    #[test]
    fn callback_bins_keep_sample_time_deduplicate_and_bound_hidden_gaps() {
        let levels = Column {
            input: 1.5,
            output: 0.8,
            level: 1.2,
            gain: 0.4,
        };
        let source = History {
            instance_id: "test".into(),
            bypassed: false,
            columns: (1..=32)
                .map(|sequence| Observation {
                    sequence,
                    frames: 256,
                    sample_rate: 48000,
                    levels,
                })
                .collect(),
        };
        let mut view = View::default();
        view.consume(&source);
        assert_eq!(view.history.len(), 17);
        assert!(view.pending_seconds < 0.01);
        view.consume(&source);
        assert_eq!(view.history.len(), 17);
        let gap = History {
            columns: vec![Observation {
                sequence: 1000,
                frames: 4800000,
                sample_rate: 48000,
                levels,
            }],
            ..source
        };
        view.consume(&gap);
        assert_eq!(view.history.len(), 512);
        assert!(view.history.iter().all(|c| *c == levels));
        assert!(view.pending_seconds < 0.01);
    }
    #[test]
    fn history_geometry_uses_real_db_and_bounded_bins_without_inventing_signal() {
        let ctx = egui::Context::default();
        theme(&ctx);
        let mut view = View::default();
        for scale in [1.0, 0.8] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(850.0, 530.0))),
                    ..Default::default()
                },
                |ui| {
                    let l = effect_panel::Layout {
                        canvas: ui.max_rect(),
                        scale,
                        origin: Pos2::ZERO,
                    };
                    let (_, empty) = history_scene(ui, l, &view, &limiter::DEFAULTS, false);
                    assert!(empty.marks.iter().all(|mark| mark.emission.is_none()));
                    view.history.extend(
                        [Column {
                            input: 1.0,
                            output: 0.5,
                            level: 0.75,
                            gain: 0.5,
                        }; 2],
                    );
                    let (source, scene) = history_scene(ui, l, &view, &limiter::DEFAULTS, false);
                    let curve = scene
                        .marks
                        .iter()
                        .find_map(|mark| match &mark.shape {
                            egui::Shape::Path(path)
                                if (path.stroke.width - 1.7 * scale).abs() < 0.001 =>
                            {
                                Some(path)
                            }
                            _ => None,
                        })
                        .unwrap();
                    let expected = (250.0 - (-6.0206 + 60.0) / 66.0 * 147.0) * scale;
                    assert!((curve.points[0].y - expected).abs() < 0.001);
                    assert!(curve.points.iter().all(|p| source.contains(*p)));
                    assert!((level_db(0.5) + 6.0206).abs() < 0.001);
                    view.history.resize(
                        512,
                        Column {
                            input: 1.0,
                            output: 0.5,
                            level: 0.75,
                            gain: 0.5,
                        },
                    );
                    let (_, dense) = history_scene(ui, l, &view, &limiter::DEFAULTS, false);
                    assert!(dense.marks.len() < 600);
                    view.history.clear();
                },
            );
            output.textures_delta.clear();
        }
    }
}
