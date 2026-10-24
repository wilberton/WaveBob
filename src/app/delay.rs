// Copyright (C) 2026 ItsAlphaNeon (reference organization and display laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native grouped Delay controls; every raw value remains exactly enterable.
use super::*;
use wavebob::{
    dsp::delay,
    processing::{Instance, ParameterValue},
};
impl WaveBob {
    pub(super) fn delay_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let v = delay::values(instance);
        let bpm = self.host.snapshot().arrangement.effect_tempo;
        egui::ScrollArea::both().max_height(520.0).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                delay_group(ui, "INPUT", 88.0, |ui| {
                    self.delay_knob(ui, window, instance, native, 0, &v, bpm)
                });
                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        delay_group(ui, "DELAY TIME", 356.0, |ui| {
                            ui.horizontal_top(|ui| {
                                for i in [4, 5, 6] {
                                    ui.add_enabled_ui(v[1] != 3, |ui| {
                                        self.delay_knob(ui, window, instance, native, i, &v, bpm)
                                    });
                                }
                            });
                            ui.horizontal(|ui| {
                                self.delay_switch(
                                    ui,
                                    window,
                                    instance,
                                    native,
                                    2,
                                    "TEMPO SYNC",
                                    &v,
                                );
                                self.delay_switch(
                                    ui,
                                    window,
                                    instance,
                                    native,
                                    3,
                                    "KEEP PITCH",
                                    &v,
                                );
                            });
                        });
                        delay_group(ui, "DELAY MODEL", 176.0, |ui| {
                            self.delay_choice(
                                ui,
                                window,
                                instance,
                                native,
                                (1, &["Mono", "True stereo", "Ping pong", "Off"]),
                                &v,
                            );
                            self.delay_knob(ui, window, instance, native, 7, &v, bpm);
                        });
                        delay_group(ui, "FEEDBACK", 454.0, |ui| {
                            ui.horizontal_top(|ui| {
                                for i in [14, 16, 17, 21, 22] {
                                    ui.add_enabled_ui(!matches!(i, 16 | 17) || v[15] != 3, |ui| {
                                        self.delay_knob(ui, window, instance, native, i, &v, bpm)
                                    });
                                }
                            });
                            self.delay_choice(
                                ui,
                                window,
                                instance,
                                native,
                                (15, &["Low pass", "High pass", "Band pass", "Off"]),
                                &v,
                            );
                        });
                    });
                    ui.horizontal_top(|ui| {
                        delay_group(ui, "MODULATION", 264.0, |ui| {
                            ui.horizontal_top(|ui| {
                                for i in [8, 9, 10] {
                                    self.delay_knob(ui, window, instance, native, i, &v, bpm);
                                }
                            });
                        });
                        delay_group(ui, "DIFFUSION", 176.0, |ui| {
                            ui.horizontal_top(|ui| {
                                for i in [11, 12] {
                                    self.delay_knob(ui, window, instance, native, i, &v, bpm);
                                }
                            });
                        });
                        delay_group(ui, "FEEDBACK DISTORTION", 546.0, |ui| {
                            ui.horizontal_top(|ui| {
                                for i in [18, 19, 20] {
                                    ui.add_enabled_ui(i == 18 || v[13] != 0, |ui| {
                                        self.delay_knob(ui, window, instance, native, i, &v, bpm)
                                    });
                                }
                                ui.vertical(|ui| {
                                    ui.set_width(258.0);
                                    let distortion = delay::Distortion::of(&v);
                                    let samples: Vec<_> = (0..128)
                                        .map(|i| {
                                            distortion
                                                .curve(v[13] != 0, -2.0 + 4.0 * i as f32 / 127.0)
                                                * 0.25
                                                + 0.5
                                        })
                                        .collect();
                                    effect_controls::graph(ui, &samples, 91.0);
                                    ui.label(
                                        RichText::new(
                                            "Shaper ±2 · limiter also follows its envelope",
                                        )
                                        .small()
                                        .color(MUTED),
                                    );
                                });
                            });
                            self.delay_choice(
                                ui,
                                window,
                                instance,
                                native,
                                (13, &["Limit", "Saturate"]),
                                &v,
                            );
                        });
                    });
                });
                delay_group(ui, "OUTPUT", 176.0, |ui| {
                    ui.horizontal_top(|ui| {
                        for i in [23, 25] {
                            self.delay_knob(ui, window, instance, native, i, &v, bpm);
                        }
                    });
                    self.delay_knob(ui, window, instance, native, 24, &v, bpm);
                });
            });
        });
        self.effect_tempo_controls(ui, native, v[2] != 0);
        ui.label(RichText::new("Intentional echoes · 0 processing delay · feedback ≥100% may reach the 60 s tail cap · Off retains filter/shaper").small().color(MUTED));
    }
    #[allow(clippy::too_many_arguments)]
    fn delay_knob(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        index: usize,
        v: &[i32; 26],
        bpm: f64,
    ) {
        ui.vertical(|ui| {
            ui.set_width(if index == 4 { 164.0 } else { 80.0 });
            let mut value = v[index] as f64;
            let (lo, hi) = delay::RANGES[index];
            let mut response = effect_controls::knob(ui, &mut value, lo as f64, hi as f64, 1.0);
            value = value.round();
            response |= ui.add(
                egui::DragValue::new(&mut value)
                    .range(lo as f64..=hi as f64)
                    .max_decimals(0),
            );
            if response.double_clicked() {
                value = delay::DEFAULTS[index] as f64;
                response.mark_changed();
            }
            ui.label(RichText::new(delay::NAMES[index]).strong());
            ui.label(
                RichText::new(delay::value_text(index, v, bpm))
                    .small()
                    .color(Color32::from_rgb(245, 198, 75)),
            );
            if response.hovered() || response.has_focus() {
                native.help = Some(format!(
                    "{}: {} · raw {} · Shift fine / arrows / double-click reset",
                    delay::NAMES[index],
                    delay::value_text(index, v, bpm),
                    v[index]
                ));
            }
            self.edit_native_control(
                window,
                &instance.id,
                native,
                (
                    delay::IDS[index].into(),
                    ParameterValue::Integer(value.round() as i32),
                    false,
                ),
                &response,
                ui.ctx(),
            );
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn delay_choice(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        control: (usize, &[&str]),
        v: &[i32; 26],
    ) {
        let (index, labels) = control;
        let mut value = v[index];
        ui.horizontal_wrapped(|ui| {
            let mut response = ui.selectable_value(&mut value, 0, labels[0]);
            for (i, label) in labels.iter().enumerate().skip(1) {
                response |= ui.selectable_value(&mut value, i as i32, *label);
            }
            if response.double_clicked() {
                value = delay::DEFAULTS[index];
                response.mark_changed();
            }
            if response.hovered() || response.has_focus() {
                native.help = Some(format!(
                    "{} · prepared transition; double-click resets",
                    delay::NAMES[index]
                ));
            }
            self.edit_native_control(
                window,
                &instance.id,
                native,
                (
                    delay::IDS[index].into(),
                    ParameterValue::Integer(value),
                    false,
                ),
                &response,
                ui.ctx(),
            );
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn delay_switch(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        index: usize,
        label: &str,
        v: &[i32; 26],
    ) {
        let mut value = v[index] != 0;
        let mut response = ui.toggle_value(&mut value, label);
        if response.double_clicked() {
            value = delay::DEFAULTS[index] != 0;
            response.mark_changed();
        }
        if response.hovered() || response.has_focus() {
            native.help = Some(format!(
                "{} · uses the shared explicit project tempo; never selection tempo",
                delay::NAMES[index]
            ));
        }
        self.edit_native_control(
            window,
            &instance.id,
            native,
            (
                delay::IDS[index].into(),
                ParameterValue::Boolean(value),
                true,
            ),
            &response,
            ui.ctx(),
        );
    }
}
fn delay_group(ui: &mut egui::Ui, label: &str, width: f32, draw: impl FnOnce(&mut egui::Ui)) {
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
