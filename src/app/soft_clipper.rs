// Copyright (C) 2026 ItsAlphaNeon (reference control layout)
// SPDX-License-Identifier: GPL-3.0-or-later
//! WaveBob's compact native Soft Clipper editor. Source layout/control mapping:
//! Lemon-Studio 0af860dfd31c7ee9cbf8af6747617e031a1a7742,
//! plugins/lemon-small-fx/src/soft_clipper.rs (GPL-3.0-or-later).
use super::*;
use wavebob::{
    dsp::soft_clipper,
    processing::{Instance, ParameterValue},
};
impl WaveBob {
    pub(super) fn soft_clipper_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let integer = |id| match instance.parameters[id] {
            ParameterValue::Integer(value) => value,
            _ => 0,
        };
        let threshold = integer("threshold");
        let post = integer("post");
        ui.columns(2,|columns| {
            egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0,BORDER)).corner_radius(4.0).inner_margin(10.0).show(&mut columns[0],|ui| {
                section(ui,"CLIPPER");
                ui.add_space(8.0);
                for (id,label,raw,minimum,maximum,default,level) in [
                    ("threshold","Threshold",threshold,1.0,127.0,100,threshold as f32/128.0),
                    ("post","Post gain",post,0.0,160.0,128,soft_clipper::post_gain(post)),
                ] {
                    ui.label(RichText::new(label).strong());
                    let mut value = raw as f64;
                    let mut response = ui.horizontal(|ui|effect_controls::number(ui,&mut value,minimum,maximum," raw",true,false)).inner;
                    if response.double_clicked() {value = default as f64;response.mark_changed();}
                    let db = if level>0.0 {format!("{:.2} dB",20.0*level.log10())}else{"−∞ dB (mute)".into()};
                    ui.label(RichText::new(db).color(Color32::from_rgb(245,198,75)));
                    if response.hovered() || response.has_focus() {native.help = Some(format!("{label}: {raw} raw · Shift for fine adjustment · double-click resets"));}
                    self.edit_native_control(window,&instance.id,native,(id.into(),ParameterValue::Integer(value.round() as i32),false),&response,ui.ctx());
                    ui.add_space(8.0);
                }
            });
            egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0,BORDER)).corner_radius(4.0).inner_margin(10.0).show(&mut columns[1],|ui| {
                section(ui,"CURVE");
                let values:Vec<_> = (0..128).map(|index| {
                    let input = -2.0+4.0*index as f32/127.0;
                    soft_clipper::curve(input,threshold,post)/2.5+0.5
                }).collect();
                let (plot,_) = effect_controls::graph(ui,&values,172.0);
                let painter = ui.painter_at(plot);
                painter.line_segment([Pos2::new(plot.center().x,plot.top()),Pos2::new(plot.center().x,plot.bottom())],Stroke::new(0.75,BORDER));
                ui.label(RichText::new("Input −2 to +2 · Output ±1.25").small().color(MUTED));
                ui.label(RichText::new("Curve includes post gain").small().color(MUTED));
            });
        });
    }
}
