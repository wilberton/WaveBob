// Copyright (C) 2026 ItsAlphaNeon (reference grouping and control mappings)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Compact native table-waveshaper controls and the exact output transfer.
use super::*;
use wavebob::{
    dsp::fast_distortion as dist,
    processing::{Instance, ParameterValue},
};
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
        egui::ScrollArea::both().max_height(240.0).show(ui,|ui| {
            ui.horizontal_top(|ui| {
                for (heading,width,indices) in [("INPUT",100.0,vec![0]),("DISTORTION",190.0,vec![1]),("CURVE",210.0,vec![]),("OUTPUT",220.0,vec![3,4])] {
                    egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0,BORDER)).corner_radius(4.0).inner_margin(8.0).show(ui,|ui| {ui.vertical(|ui| {
                        ui.set_width(width);section(ui,heading);
                        if heading=="CURVE" {
                            if let Some((_,curve))=&native.fast_dist_curve {
                                let samples:Vec<_>=(0..128).map(|i|curve.sample(-1.0+2.0*i as f32/127.0)*0.5+0.5).collect();
                                let (plot,response)=effect_controls::graph(ui,&samples,120.0);
                                ui.painter().line_segment([Pos2::new(plot.center().x,plot.top()),Pos2::new(plot.center().x,plot.bottom())],Stroke::new(0.5,BORDER));
                                ui.label(RichText::new("Input / output ±1").small().color(MUTED));
                                if response.hovered(){native.help=Some("Exact interpolated A/B transfer including pre, mix and post. Output headroom is preserved beyond the displayed ±1.".into());}
                            }
                        } else {ui.horizontal_top(|ui| {
                            for index in indices {
                                ui.vertical(|ui| {
                                    ui.set_width(100.0);
                                    ui.label(RichText::new(["PRE AMP","THRESHOLD","TYPE","MIX","POST GAIN"][index]).strong());
                                    let mut value=values[index] as f64;
                                    let mut response=effect_controls::number(ui,&mut value,dist::RANGES[index].0 as f64,dist::RANGES[index].1 as f64," raw",true,false);
                                    if response.double_clicked(){value=dist::DEFAULTS[index] as f64;response.mark_changed();}
                                    ui.label(RichText::new(dist::value_text(index,values[index])).small().color(Color32::from_rgb(245,198,75)));
                                    if response.hovered()||response.has_focus(){native.help=Some(format!("{}: {} · Shift for fine adjustment · double-click resets",dist::IDS[index],dist::value_text(index,values[index])));}
                                    self.edit_native_control(window,&instance.id,native,(dist::IDS[index].into(),ParameterValue::Integer(value.round() as i32),false),&response,ui.ctx());
                                });
                            }
                            if heading=="DISTORTION" {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("TYPE").strong());
                                    let mut value=values[2];
                                    let mut response=ui.selectable_value(&mut value,0,"A");response|=ui.selectable_value(&mut value,1,"B");
                                    if response.double_clicked(){value=0;response.mark_changed();}
                                    if response.hovered()||response.has_focus(){native.help=Some("A: threshold compression and makeup gain · B: attenuation power curve".into());}
                                    self.edit_native_control(window,&instance.id,native,("type".into(),ParameterValue::Integer(value),false),&response,ui.ctx());
                                });
                            }
                        });}
                    });});
                }
            });
        });
        ui.label(
            RichText::new("Reference table shaping · no oversampling · output headroom retained")
                .small()
                .color(MUTED),
        );
    }
}
