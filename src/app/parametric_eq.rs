// Copyright (C) 2026 ItsAlphaNeon (reference layout and control mappings)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Standard seven-band EQ graph and exact native controls.
use super::*;
use wavebob::{
    dsp::parametric_eq as eq,
    processing::{self, Instance, ParameterValue},
};
const COLORS: [Color32; 7] = [
    Color32::from_rgb(198, 166, 242),
    Color32::from_rgb(242, 166, 212),
    Color32::from_rgb(245, 178, 154),
    Color32::from_rgb(233, 210, 126),
    Color32::from_rgb(172, 221, 144),
    Color32::from_rgb(146, 217, 199),
    Color32::from_rgb(161, 192, 242),
];
fn raw(instance: &Instance, id: &str) -> i32 {
    match instance.parameters[id] {
        ParameterValue::Integer(v) => v,
        _ => unreachable!(),
    }
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
        egui::ScrollArea::both().max_height(480.0).show(ui,|ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {ui.set_width(740.0);self.eq_graph(ui,window,instance,native,rate);});
                egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0,BORDER)).inner_margin(8.0).show(ui,|ui| {ui.vertical(|ui| {
                    section(ui,"LEVELS");
                    ui.horizontal_top(|ui| {
                        for (band,color) in COLORS.into_iter().chain([Color32::WHITE]).enumerate() {
                            ui.vertical(|ui| {
                                ui.set_width(29.0);ui.spacing_mut().slider_width=240.0;
                                let id=if band==7 {"main".into()}else{eq::parameter_id("level",band)};
                                let mut value=raw(instance,&id) as f64/100.0;
                                let mut response=ui.add(egui::Slider::new(&mut value,-18.0..=18.0).vertical().show_value(false));
                                if response.double_clicked(){value=0.0;response.mark_changed();}
                                ui.label(RichText::new(if band==7 {"M".into()}else{(band+1).to_string()}).color(color));
                                if response.hovered() || response.has_focus(){native.help=Some(format!("{} level: {:.2} dB",if band==7 {"Main".into()}else{format!("Band {}",band+1)},value));}
                                self.edit_native_control(window,&instance.id,native,(id,ParameterValue::Integer((value*100.0).round_ties_even() as i32),false),&response,ui.ctx());
                                if band<7 {
                                    let id=eq::parameter_id("enabled",band);let mut enabled=matches!(instance.parameters[&id],ParameterValue::Boolean(true));
                                    let response=ui.checkbox(&mut enabled,"");
                                    if response.clicked(){native.eq_band=band;}
                                    self.edit_native_control(window,&instance.id,native,(id,ParameterValue::Boolean(enabled),false),&response,ui.ctx());
                                }
                            });
                        }
                    });
                });});
            });
            ui.horizontal(|ui| {for (band,color) in COLORS.into_iter().enumerate(){if ui.selectable_label(native.eq_band==band,RichText::new(format!("Band {}",band+1)).color(color)).clicked(){native.eq_band=band;}}ui.label(RichText::new("Standard IIR · 20 Hz–20 kHz · ±18 dB").small().color(MUTED));});
            let band=native.eq_band.min(6);
            ui.horizontal(|ui| {
                let id=eq::parameter_id("type",band);let mut shape=raw(instance,&id);
                let response=egui::ComboBox::from_id_salt("eq-shape").selected_text(eq::TYPE_NAMES[shape as usize]).show_ui(ui,|ui|{for (index,name) in eq::TYPE_NAMES.iter().enumerate(){ui.selectable_value(&mut shape,index as i32,*name);}}).response;
                let mut response=response;if shape!=raw(instance,&id){response.mark_changed();}
                self.edit_native_control(window,&instance.id,native,(id,ParameterValue::Integer(shape),false),&response,ui.ctx());
                let id=eq::parameter_id("order",band);let mut order=raw(instance,&id);
                let response=egui::ComboBox::from_id_salt("eq-order").selected_text(eq::ORDER_NAMES[(order+3) as usize]).show_ui(ui,|ui|{for (index,name) in eq::ORDER_NAMES.iter().enumerate(){ui.selectable_value(&mut order,index as i32-3,*name);}}).response;
                let mut response=response;if order!=raw(instance,&id){response.mark_changed();}
                self.edit_native_control(window,&instance.id,native,(id,ParameterValue::Integer(order),false),&response,ui.ctx());
                for (group,label,divisor,min,max,suffix) in [("level","Level",100.0,-18.0,18.0," dB"),("freq","Frequency",1.0,20.0,20000.0," Hz"),("width","Bandwidth",655.36,0.0,100.0,"%")] {
                    let id=eq::parameter_id(group,band);let old=raw(instance,&id);
                    let mut value=if group=="freq"{eq::frequency_hz(old)}else{old as f64/divisor};
                    ui.label(label);
                    let mut response=ui.add(egui::DragValue::new(&mut value).range(min..=max).speed(if group=="freq"{1.0}else{0.01}).max_decimals(4).suffix(suffix));
                    if response.double_clicked(){value=match group {"level"=>0.0,"freq"=>eq::frequency_hz(eq::DEFAULT_FREQUENCIES[band]),_=>if matches!(shape,5|7){40124.0/655.36}else if matches!(shape,1|3){36068.0/655.36}else{25412.0/655.36}};response.mark_changed();}
                    let new=if group=="freq"{eq::frequency_raw(value)}else{(value*divisor).round_ties_even() as i32};
                    self.edit_native_control(window,&instance.id,native,(id,ParameterValue::Integer(new),false),&response,ui.ctx());
                }
            });
            ui.horizontal(|ui| {
                ui.label("Main level");let mut main=raw(instance,"main") as f64/100.0;
                let mut response=ui.add(egui::DragValue::new(&mut main).range(-18.0..=18.0).speed(0.01).max_decimals(2).suffix(" dB"));
                if response.double_clicked(){main=0.0;response.mark_changed();}
                self.edit_native_control(window,&instance.id,native,("main".into(),ParameterValue::Integer((main*100.0).round_ties_even() as i32),false),&response,ui.ctx());
                let reset=ui.button("Reset selected band");
                if reset.clicked(){self.eq_reset(window,instance,native,band,&reset,ui.ctx());}
                ui.label(RichText::new("Drag token: frequency / gain · Shift-drag or wheel: width · Alt-click: reset").small().color(MUTED));
            });
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
        rate: u32,
    ) {
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::new(740.0, 300.0), Sense::click_and_drag());
        let plot = rect.shrink2(Vec2::new(32.0, 22.0));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, SURFACE);
        let x = |raw: f64| plot.left() + plot.width() * raw as f32 / 65536.0;
        let y = |db: f64| plot.center().y - plot.height() * db as f32 / 36.0;
        for hz in [
            20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
        ] {
            let xx = x(eq::frequency_raw(hz) as f64);
            painter.line_segment(
                [Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())],
                Stroke::new(0.5, BORDER),
            );
            painter.text(
                Pos2::new(xx, rect.bottom() - 12.0),
                egui::Align2::CENTER_CENTER,
                if hz >= 1000.0 {
                    format!("{}k", hz / 1000.0)
                } else {
                    format!("{hz:.0}")
                },
                egui::FontId::proportional(10.0),
                MUTED,
            );
        }
        for db in [-18, -12, -6, 0, 6, 12, 18] {
            let yy = y(db as f64);
            painter.line_segment(
                [Pos2::new(plot.left(), yy), Pos2::new(plot.right(), yy)],
                Stroke::new(if db == 0 { 1.0 } else { 0.5 }, BORDER),
            );
            painter.text(
                Pos2::new(rect.left() + 2.0, yy),
                egui::Align2::LEFT_CENTER,
                format!("{db:+}"),
                egui::FontId::proportional(10.0),
                MUTED,
            );
        }
        let transport = self.host.transport();
        let spectrum = spectrum(
            &transport.analyzer,
            transport.output.sample_rate.unwrap_or(rate),
        );
        let points: Vec<_> = spectrum
            .iter()
            .enumerate()
            .map(|(i, db)| {
                Pos2::new(
                    plot.left() + plot.width() * i as f32 / 63.0,
                    plot.bottom() - plot.height() * ((db + 90.0) / 90.0).clamp(0.0, 1.0),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.0, Color32::from_rgb(55, 100, 130)),
        ));
        let curve = eq::Response::new(instance, rate);
        let points: Vec<_> = (0..500)
            .map(|i| {
                let raw = i as f64 / 499.0 * 65536.0;
                Pos2::new(
                    x(raw),
                    y(curve.db(eq::frequency_hz(raw.round_ties_even() as i32)))
                        .clamp(plot.top(), plot.bottom()),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(2.0, Color32::from_rgb(245, 198, 75)),
        ));
        let mut centers = [Pos2::ZERO; 7];
        for band in 0..7 {
            let shape = raw(instance, &eq::parameter_id("type", band));
            let db = if matches!(shape, 5..=7) {
                raw(instance, &eq::parameter_id("level", band)) as f64 / 100.0
            } else {
                0.0
            };
            let pos = Pos2::new(
                x(raw(instance, &eq::parameter_id("freq", band)) as f64),
                y(db),
            );
            centers[band] = pos;
            let enabled = matches!(
                instance.parameters[&eq::parameter_id("enabled", band)],
                ParameterValue::Boolean(true)
            );
            painter.circle_filled(
                pos,
                10.0,
                if enabled && shape != 0 {
                    COLORS[band]
                } else {
                    BORDER
                },
            );
            painter.circle_stroke(
                pos,
                if native.eq_band == band { 13.0 } else { 10.0 },
                Stroke::new(1.0, COLORS[band]),
            );
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                (band + 1).to_string(),
                egui::FontId::proportional(11.0),
                SURFACE,
            );
        }
        let hit = response
            .interact_pointer_pos()
            .or_else(|| response.hover_pos())
            .and_then(|p| {
                centers
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i, c.distance(p)))
                    .filter(|(_, d)| *d <= 16.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            });
        if let Some(band) = hit {
            native.help = Some(format!(
                "Band {} · {:.0} Hz · {:.2} dB · width {:.1}%",
                band + 1,
                eq::frequency_hz(raw(instance, &eq::parameter_id("freq", band))),
                raw(instance, &eq::parameter_id("level", band)) as f64 / 100.0,
                raw(instance, &eq::parameter_id("width", band)) as f64 / 655.36
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
                    .filter(|(_, d)| *d <= 16.0)
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
                        - delta.y * 65536.0 / 160.0)
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
                let delta = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta.y = 0.0;
                delta
            });
            if scroll != 0.0 {
                let value = (raw(instance, &eq::parameter_id("width", band))
                    + if scroll > 0.0 { 3277 } else { -3277 })
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
        painter.text(
            Pos2::new(plot.left(), rect.top() + 8.0),
            egui::Align2::LEFT_CENTER,
            "Transport output spectrum: −90..0 dBFS",
            egui::FontId::proportional(10.0),
            MUTED,
        );
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
