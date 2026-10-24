// Copyright (C) 2026 ItsAlphaNeon (reference control groups and display laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Chorus/Crossover and three LFO groups following the pinned editor.
use super::*;
use wavebob::{
    dsp::chorus,
    processing::{Instance, ParameterValue},
};
impl WaveBob {
    pub(super) fn chorus_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let v = chorus::values(instance);
        egui::ScrollArea::both().max_height(425.0).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal_top(|ui| {
                    chorus_group(ui, "CHORUS", 506.0, |ui| {
                        ui.horizontal_top(|ui| {
                            for (i, &value) in v.iter().take(3).enumerate() {
                                self.chorus_knob(ui, window, instance, native, i, value);
                            }
                            self.chorus_pick(
                                ui,
                                window,
                                instance,
                                native,
                                (11, &[("Dry + wet", 0), ("Wet only", 1024)]),
                                v[11],
                            );
                        });
                    });
                    chorus_group(ui, "CROSSOVER", 296.0, |ui| {
                        ui.horizontal_top(|ui| {
                            self.chorus_knob(ui, window, instance, native, 10, v[10]);
                            self.chorus_pick(
                                ui,
                                window,
                                instance,
                                native,
                                (9, &[("Highs", 0), ("Lows", 100)]),
                                v[9],
                            );
                        });
                    });
                });
                ui.horizontal_top(|ui| {
                    for i in 0..3 {
                        chorus_group(ui, &format!("LFO {}", i + 1), 262.0, |ui| {
                            ui.horizontal_top(|ui| {
                                self.chorus_knob(ui, window, instance, native, 3 + i, v[3 + i]);
                                self.chorus_pick(
                                    ui,
                                    window,
                                    instance,
                                    native,
                                    (
                                        6 + i,
                                        &[
                                            ("Arc", 0),
                                            ("Sharp arc", 342),
                                            ("Sines", 683),
                                            ("Sharp sines", 1024),
                                        ],
                                    ),
                                    v[6 + i],
                                );
                            });
                        });
                    }
                });
            });
        });
        ui.label(
            RichText::new(
                "Three modulated voices · choose the chorused frequency band · wet only for parallel routing",
            )
            .small()
            .color(MUTED),
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn chorus_knob(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        index: usize,
        raw: i32,
    ) {
        ui.vertical(|ui| {
            ui.set_width(108.0);
            let mut value = raw as f64;
            let (lo, hi) = chorus::RANGES[index];
            let mut response = effect_controls::knob(ui, &mut value, lo as f64, hi as f64, 1.0);
            value = value.round();
            response |= ui.add(
                egui::DragValue::new(&mut value)
                    .range(lo as f64..=hi as f64)
                    .max_decimals(0),
            );
            if response.double_clicked() {
                value = chorus::DEFAULTS[index] as f64;
                response.mark_changed();
            }
            ui.label(RichText::new(chorus::NAMES[index]).strong());
            ui.label(
                RichText::new(chorus::value_text(index, raw))
                    .small()
                    .color(Color32::from_rgb(245, 198, 75)),
            );
            if response.hovered() || response.has_focus() {
                native.help = Some(format!(
                    "{}: {} · exact raw {} · Shift fine / arrows / double-click reset",
                    chorus::NAMES[index],
                    chorus::value_text(index, raw),
                    raw
                ));
            }
            self.edit_native_control(
                window,
                &instance.id,
                native,
                (
                    chorus::IDS[index].into(),
                    ParameterValue::Integer(value.round() as i32),
                    false,
                ),
                &response,
                ui.ctx(),
            );
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn chorus_pick(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        control: (usize, &[(&str, i32)]),
        raw: i32,
    ) {
        let (index, choices) = control;
        ui.vertical(|ui| {
            ui.set_width(140.0);
            let mut value = raw;
            let mut response = ui.label(RichText::new(chorus::NAMES[index]).strong());
            let current = chorus::value_text(index, raw);
            for &(label, canonical) in choices {
                let selected = chorus::value_text(index, canonical) == current;
                let mut choice = ui.selectable_label(selected, label);
                if choice.clicked() {
                    value = canonical;
                    choice.mark_changed();
                }
                response |= choice;
            }
            response |= ui.add(
                egui::DragValue::new(&mut value)
                    .range(chorus::RANGES[index].0..=chorus::RANGES[index].1)
                    .suffix(" raw"),
            );
            if response.double_clicked() {
                value = chorus::DEFAULTS[index];
                response.mark_changed();
            }
            if response.hovered() || response.has_focus() {
                native.help = Some(format!(
                    "{}: {current} · all source raw values are enterable · double-click resets",
                    chorus::NAMES[index]
                ));
            }
            self.edit_native_control(
                window,
                &instance.id,
                native,
                (
                    chorus::IDS[index].into(),
                    ParameterValue::Integer(value),
                    false,
                ),
                &response,
                ui.ctx(),
            );
        });
    }
}
fn chorus_group(ui: &mut egui::Ui, label: &str, width: f32, draw: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                section(ui, label);
                draw(ui);
            });
        });
}
