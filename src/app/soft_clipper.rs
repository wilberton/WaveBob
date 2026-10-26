// Copyright (C) 2026 ItsAlphaNeon (reference mappings)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Two calibrated shared dials and the actual Soft Clipper transfer.
use super::effect_panel::{Dial, dial};
use super::*;
use wavebob::{
    dsp::soft_clipper,
    processing::{Instance, ParameterValue},
};
#[derive(Clone, Copy)]
pub(super) struct Control(pub usize);
impl Dial for Control {
    fn label(self) -> &'static str {
        if self.0 == 0 {
            "Threshold"
        } else {
            "Post gain"
        }
    }
    fn range(self) -> (f64, f64, f64) {
        if self.0 == 0 {
            (1.0, 127.0, 1.0)
        } else {
            (0.0, 160.0, 1.0)
        }
    }
    fn reset(self) -> f64 {
        if self.0 == 0 { 100.0 } else { 128.0 }
    }
    fn text(self, value: f64) -> String {
        let gain = self.gain(value.round() as i32);
        if gain > 0.0 {
            format!("{:.1} dB", 20.0 * gain.log10())
        } else {
            "−∞ dB".into()
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().replace('−', "-");
        let text = text.trim_end_matches("dB").trim();
        if matches!(text, "-∞" | "-inf") {
            return (self.0 == 1).then_some(0.0);
        }
        let db = text.parse::<f64>().ok().filter(|v| v.is_finite())?;
        let target = 10.0_f64.powf(db.clamp(-120.0, 60.0) / 20.0);
        let (min, max, _) = self.range();
        (min as i32..=max as i32)
            .min_by(|a, b| {
                (self.gain(*a) - target)
                    .abs()
                    .total_cmp(&(self.gain(*b) - target).abs())
            })
            .map(|v| v as f64)
    }
}
impl Control {
    fn gain(self, raw: i32) -> f64 {
        if self.0 == 0 {
            raw as f64 / 128.0
        } else {
            soft_clipper::post_gain(raw) as f64
        }
    }
}
pub(super) fn transfer_points(plot: Rect, threshold: i32, post: i32) -> (f32, Vec<Pos2>) {
    let limit = soft_clipper::curve(2.0, threshold, post)
        .abs()
        .max(1.0)
        .ceil();
    let points = (0..=512)
        .map(|i| {
            let x = -2.0 + i as f32 / 128.0;
            Pos2::new(
                plot.left() + (x + 2.0) * plot.width() * 0.25,
                plot.center().y
                    - soft_clipper::curve(x, threshold, post) / limit * plot.height() * 0.5,
            )
        })
        .collect();
    (limit, points)
}
impl WaveBob {
    pub(super) fn soft_clipper_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let integer = |id| match instance.parameters[id] {
            ParameterValue::Integer(v) => v,
            _ => 0,
        };
        let values = [integer("threshold"), integer("post")];
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (canvas, scale, origin) = self.effect_panel(ui, window, instance, native);
                let point = |x, y| origin + Vec2::new(x, y) * scale;
                let painter = ui.painter_at(canvas);
                let plot = Rect::from_min_max(point(242.0, 78.0), point(623.0, 435.0));
                let (limit, points) = transfer_points(plot, values[0], values[1]);
                transfer_plot::grid(&painter, plot, scale, 2.0, limit);
                if let Some(frame) = &mut native.visualization {
                    frame.place(&painter);
                    let mut scene = visualization::Scene::default();
                    let blue = Color32::from_rgba_unmultiplied(
                        0,
                        153,
                        255,
                        if instance.bypassed { 64 } else { 255 },
                    );
                    scene.path(
                        points.clone(),
                        Stroke::new(1.8 * scale, blue),
                        Some(visualization::Emission::new(blue, 2.0)),
                    );
                    scene.path(
                        points,
                        Stroke::new(
                            0.55 * scale,
                            Color32::from_rgba_unmultiplied(
                                143,
                                224,
                                255,
                                if instance.bypassed { 50 } else { 220 },
                            ),
                        ),
                        None,
                    );
                    if let Err(error) = frame.submit(
                        egui::Id::unique("transfer"),
                        visualization::revision(instance),
                        plot,
                        plot.expand(12.0 * scale).intersect(canvas),
                        scene,
                    ) {
                        self.error = Some(error.into());
                    }
                }
                for (i, x) in [(0, 100.0), (1, 753.0)] {
                    let control = Control(i);
                    let mut value = values[i] as f64;
                    let mut dial_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(control.label())
                            .max_rect(canvas),
                    );
                    let response = dial(
                        &mut dial_ui,
                        point(x, 257.0),
                        51.0 * scale,
                        scale,
                        control,
                        &mut value,
                    )
                    .on_hover_text(format!(
                        "{} raw · Shift for fine adjustment · double-click resets",
                        values[i]
                    ));
                    self.edit_native_control(
                        window,
                        &instance.id,
                        native,
                        (
                            ["threshold", "post"][i].into(),
                            ParameterValue::Integer(value.round() as i32),
                            false,
                        ),
                        &response,
                        ui.ctx(),
                    );
                }
            });
    }
}
