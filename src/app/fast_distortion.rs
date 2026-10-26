// Copyright (C) 2026 ItsAlphaNeon (reference control mappings)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Four shared blue dials and the exact, glowing Distortion transfer curve.
use super::effect_panel::{ACCENT, BACKGROUND, BANK, Dial, TEXT, dial, font};
use super::*;
use wavebob::{
    dsp::fast_distortion as dist,
    processing::{Instance, ParameterValue},
};

#[derive(Clone, Copy)]
pub(super) struct Control(pub(super) usize);

impl Dial for Control {
    fn label(self) -> &'static str {
        ["Preamp", "Threshold", "Type", "Mix", "Post gain"][self.0]
    }
    fn range(self) -> (f64, f64, f64) {
        let (min, max) = dist::RANGES[self.0];
        (min as f64, max as f64, 1.0)
    }
    fn reset(self) -> f64 {
        dist::DEFAULTS[self.0] as f64
    }
    fn text(self, value: f64) -> String {
        dist::value_text(self.0, value.round() as i32)
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().replace('−', "-");
        let text = text.trim_end_matches("dB").trim_end_matches('%').trim();
        if matches!(text, "-∞" | "-inf") && self.0 == 4 {
            return Some(0.0);
        }
        let displayed = text.parse::<f64>().ok().filter(|v| v.is_finite())?;
        if self.0 == 1 {
            return Some(displayed.round().clamp(1.0, 10.0));
        }
        let gain = if self.0 == 3 {
            (displayed / 100.0).clamp(0.0, 1.0)
        } else {
            10.0_f64.powf(displayed.clamp(-120.0, 60.0) / 20.0)
        };
        // Nearest representable reference value, including exact mute.
        // Opening/formatting never rewrites existing settings.
        let (min, max) = dist::RANGES[self.0];
        (min..=max)
            .min_by(|a, b| {
                (dist::level(*a) as f64 - gain)
                    .abs()
                    .total_cmp(&(dist::level(*b) as f64 - gain).abs())
            })
            .map(|raw| raw as f64)
    }
}

impl WaveBob {
    pub(super) fn fast_distortion_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let values = dist::values(instance);
        if native
            .fast_dist_curve
            .as_ref()
            .is_none_or(|(old, _)| *old != values)
        {
            match dist::Curve::new(values) {
                Ok(curve) => native.fast_dist_curve = Some((values, curve)),
                Err(error) => {
                    self.error = Some(error.message);
                    return;
                }
            }
        }
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (canvas, scale, origin) = self.effect_panel(ui, window, instance, native);
                let point = |x, y| origin + Vec2::new(x, y) * scale;
                let painter = ui.painter_at(canvas);
                if let Some((_, curve)) = &native.fast_dist_curve {
                    transfer(
                        &painter,
                        Rect::from_min_max(point(242.0, 78.0), point(623.0, 402.0)),
                        scale,
                        curve,
                        instance.bypassed,
                        native.visualization.as_mut(),
                        visualization::revision(instance),
                    );
                }
                for (index, x, y) in [
                    (0, 100.0, 194.0),
                    (1, 100.0, 393.0),
                    (4, 753.0, 194.0),
                    (3, 753.0, 393.0),
                ] {
                    let control = Control(index);
                    let mut control_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(control.label())
                            .max_rect(canvas),
                    );
                    let mut value = values[index] as f64;
                    let response = dial(
                        &mut control_ui,
                        point(x, y),
                        46.0 * scale,
                        scale,
                        control,
                        &mut value,
                    );
                    self.edit_native_control(
                        window,
                        &instance.id,
                        native,
                        (
                            dist::IDS[index].into(),
                            ParameterValue::Integer(value.round() as i32),
                            false,
                        ),
                        &response,
                        ui.ctx(),
                    );
                }
                painter.text(
                    point(370.0, 474.0),
                    egui::Align2::RIGHT_CENTER,
                    "Type",
                    font(16.0 * scale, false),
                    TEXT,
                );
                let mut kind = values[2];
                for (value, label, x) in [(0, "A", 386.0), (1, "B", 442.0)] {
                    let previous = kind;
                    let rect = Rect::from_min_max(point(x, 458.0), point(x + 56.0, 490.0));
                    let mut response = ui.interact(
                        rect,
                        ui.scope_id().with(("distortion-type", value)),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            ui.is_enabled(),
                            format!("Type {label}"),
                        )
                    });
                    if response.clicked() {
                        response.request_focus();
                        kind = value;
                    }
                    if response.has_focus()
                        && ui.input(|i| {
                            i.key_pressed(egui::Key::ArrowLeft)
                                || i.key_pressed(egui::Key::ArrowRight)
                        })
                    {
                        kind = 1 - kind;
                    }
                    if response.double_clicked() {
                        kind = dist::DEFAULTS[2];
                    }
                    if kind != previous {
                        response.mark_changed();
                    }
                    painter.rect(
                        rect,
                        4.0 * scale,
                        if kind == value {
                            ACCENT
                        } else if response.hovered() {
                            BANK
                        } else {
                            BACKGROUND
                        },
                        Stroke::new(scale, if response.has_focus() { TEXT } else { MUTED }),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        font(14.0 * scale, false),
                        TEXT,
                    );
                    self.edit_native_control(
                        window,
                        &instance.id,
                        native,
                        ("type".into(), ParameterValue::Integer(kind), false),
                        &response,
                        ui.ctx(),
                    );
                }
            });
    }
}

/// Same table/interpolator as the audio processor, with headroom visible.
pub(super) fn transfer_points(plot: Rect, curve: &dist::Curve) -> (f32, Vec<Pos2>) {
    let limit = curve.sample(1.0).abs().max(1.0).ceil();
    let points = (0..=512)
        .map(|i| {
            let input = -1.0 + i as f32 / 256.0;
            Pos2::new(
                plot.left() + (input + 1.0) * 0.5 * plot.width(),
                plot.center().y - curve.sample(input) / limit * plot.height() * 0.5,
            )
        })
        .collect();
    (limit, points)
}

fn transfer(
    painter: &egui::Painter,
    plot: Rect,
    scale: f32,
    curve: &dist::Curve,
    bypassed: bool,
    frame: Option<&mut visualization::Frame>,
    revision: u64,
) {
    let (limit, points) = transfer_points(plot, curve);
    transfer_plot::grid(painter, plot, scale, 1.0, limit);
    if let Some(frame) = frame {
        frame.place(painter);
        let mut scene = visualization::Scene::default();
        let color = Color32::from_rgba_unmultiplied(0, 153, 255, if bypassed { 64 } else { 255 });
        scene.path(
            points.clone(),
            Stroke::new(1.8 * scale, color),
            Some(visualization::Emission::new(color, 2.0)),
        );
        scene.path(
            points,
            Stroke::new(
                0.55 * scale,
                Color32::from_rgba_unmultiplied(143, 224, 255, if bypassed { 50 } else { 220 }),
            ),
            None,
        );
        let glow_clip = plot.expand(12.0 * scale).intersect(painter.clip_rect());
        let _ = frame.submit(
            egui::Id::unique("transfer"),
            revision,
            plot,
            glow_clip,
            scene,
        );
    }
}
