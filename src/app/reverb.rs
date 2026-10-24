// Copyright (C) 2026 ItsAlphaNeon (reference layout and visual laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Reverb editor following the pinned Lemon INPUT/ROOM/DECAY/MIX layout.
use super::*;
use wavebob::{
    dsp::reverb,
    processing::{Instance, ParameterValue},
};

impl WaveBob {
    pub(super) fn reverb_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let knobs: [i32; 15] = std::array::from_fn(|i| match instance.parameters[reverb::IDS[i]] {
            ParameterValue::Integer(v) => v,
            _ => unreachable!(),
        });
        let sync = matches!(
            instance.parameters["tempo_sync"],
            ParameterValue::Boolean(true)
        );
        egui::ScrollArea::both().max_height(405.0).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    section(ui, "ROOM VIEW");
                    room_picture(ui, &knobs);
                    ui.label(RichText::new("Size · diffusion · filters · wet glow").small().color(MUTED));
                    ui.label(RichText::new("Plate / tank reverb").strong());
                });
                for (heading,width,rows) in [
                    ("INPUT",96.0,vec![vec![],vec![1],vec![0]]),
                    ("ROOM",204.0,vec![vec![2],vec![3,14],vec![4,13]]),
                    ("DECAY",204.0,vec![vec![7,5],vec![8,6],vec![]]),
                ] {
                    egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0,BORDER)).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| { ui.vertical(|ui| {
                        ui.set_width(width); section(ui,heading);
                        for (row, indices) in rows.iter().enumerate() {
                            ui.allocate_ui_with_layout(Vec2::new(width,111.0),egui::Layout::left_to_right(egui::Align::Min),|ui| {
                                for &index in indices { self.reverb_knob(ui,window,instance,native,index,&knobs,sync); }
                                if row==0 && heading=="INPUT" { self.reverb_switch(ui,window,instance,native,"mid_side","M/S","Mid / side input: excite the tank with left minus right"); }
                                if row==0 && heading=="ROOM" { self.reverb_switch(ui,window,instance,native,"tempo_sync","TEMPO","Predelay in project beats (192 raw = one beat); otherwise seconds"); }
                                if row==2 && heading=="DECAY" { ui.label(RichText::new("REVERB").size(20.0).color(MUTED)); }
                            });
                        }
                    }); });
                }
                egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0,BORDER)).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| { ui.vertical(|ui| {
                    ui.set_width(170.0);section(ui,"MIX");
                    ui.horizontal_top(|ui| {
                        for (index, caption) in [(10,"DRY"),(11,"ER"),(12,"WET")] {
                            ui.vertical(|ui| {
                                ui.set_width(48.0);
                                let mut value = knobs[index] as f64;
                                let mut response = ui.add(egui::Slider::new(&mut value,0.0..=160.0).vertical().show_value(false).integer());
                                response |= ui.add(egui::DragValue::new(&mut value).range(0.0..=160.0).max_decimals(0));
                                if response.double_clicked() { value=128.0;response.mark_changed(); }
                                ui.label(RichText::new(caption).strong());
                                ui.label(RichText::new(reverb::value_text(index,knobs[index],sync)).small());
                                if response.hovered() || response.has_focus() {native.help=Some(format!("{caption}: {} raw · double-click resets to 128",knobs[index]));}
                                self.edit_native_control(window,&instance.id,native,(reverb::IDS[index].into(),ParameterValue::Integer(value.round() as i32),false),&response,ui.ctx());
                            });
                        }
                    });
                    ui.add_space(10.0);
                    self.reverb_knob(ui,window,instance,native,9,&knobs,sync);
                }); });
            });
        });
        if sync {
            let bpm = self.host.snapshot().arrangement.effect_tempo;
            ui.horizontal(|ui| {
                ui.label("Project effect tempo");
                let text = native.tempo_text.get_or_insert_with(|| bpm.to_string());
                let response = ui.add(egui::TextEdit::singleline(text).desired_width(64.0));
                ui.label("BPM");
                let apply = ui.button("Set tempo");
                if apply.clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    if let Ok(bpm) = text.parse::<f64>() {
                        self.dispatch(Command::SetEffectTempo(wavebob::processing::TempoParams {
                            bpm,
                        }));
                    } else {
                        self.error = Some("Effect tempo must be 20..400 BPM".into());
                    }
                    native.tempo_text = None;
                }
                ui.label(
                    RichText::new(format!("Current: {bpm} BPM · shared by all synced effects"))
                        .small()
                        .color(MUTED),
                );
            });
        } else {
            native.tempo_text = None;
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn reverb_knob(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        index: usize,
        knobs: &[i32; 15],
        sync: bool,
    ) {
        let caption = [
            "L.CUT", "H.CUT", "DELAY", "SIZE", "DIFF", "DEC", "DAMP", "BASS", "CROSS", "SEP",
            "DRY", "ER", "WET", "SPEED", "MOD",
        ][index];
        ui.vertical(|ui| {
            ui.set_width(94.0);
            let mut value = knobs[index] as f64;
            let mut response = effect_controls::knob(
                ui,
                &mut value,
                reverb::RANGES[index].0 as f64,
                reverb::RANGES[index].1 as f64,
                1.0,
            );
            value = value.round();
            response |= ui.add(
                egui::DragValue::new(&mut value)
                    .range(reverb::RANGES[index].0 as f64..=reverb::RANGES[index].1 as f64)
                    .max_decimals(0)
                    .suffix(" raw"),
            );
            if response.double_clicked() {
                value = reverb::DEFAULTS[index] as f64;
                response.mark_changed();
            }
            ui.label(RichText::new(caption).strong());
            ui.label(
                RichText::new(reverb::value_text(index, knobs[index], sync))
                    .small()
                    .color(Color32::from_rgb(245, 198, 75)),
            );
            if response.hovered() || response.has_focus() {
                native.help = Some(format!(
                    "{caption}: {} · Shift for fine adjustment · double-click resets",
                    reverb::value_text(index, knobs[index], sync)
                ));
            }
            self.edit_native_control(
                window,
                &instance.id,
                native,
                (
                    reverb::IDS[index].into(),
                    ParameterValue::Integer(value.round() as i32),
                    false,
                ),
                &response,
                ui.ctx(),
            );
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn reverb_switch(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        id: &str,
        caption: &str,
        help: &str,
    ) {
        let mut value = matches!(instance.parameters[id], ParameterValue::Boolean(true));
        ui.vertical(|ui| {
            let mut response = ui.toggle_value(&mut value, caption);
            if response.double_clicked() {
                value = false;
                response.mark_changed();
            }
            if response.hovered() || response.has_focus() {
                native.help = Some(help.into());
            }
            self.edit_native_control(
                window,
                &instance.id,
                native,
                (id.into(), ParameterValue::Boolean(value), true),
                &response,
                ui.ctx(),
            );
            ui.label(
                RichText::new(if value { "On" } else { "Off" })
                    .small()
                    .color(MUTED),
            );
        });
    }
}

fn room_picture(ui: &mut egui::Ui, knobs: &[i32; 15]) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(224.0), Sense::hover());
    let look = reverb_room::look(knobs);
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        4.0,
        Color32::from_rgb(look.tint[0] as u8, look.tint[1] as u8, look.tint[2] as u8),
    );
    let angle = reverb_room::angle_at(ui.input(|i| i.time));
    let scale = rect.width() / reverb_room::FRAME;
    for [a, b] in reverb_room::edges(&look, angle) {
        let points = [
            rect.min + Vec2::new(a.0, a.1) * scale,
            rect.min + Vec2::new(b.0, b.1) * scale,
        ];
        let shade = (a.2 + b.2) * 0.5;
        for (width, alpha) in [(14.0, 0.014), (8.0, 0.025), (4.0, 0.04)] {
            painter.line_segment(
                points,
                Stroke::new(
                    width * scale,
                    Color32::from_white_alpha((255.0 * alpha * look.glow * shade).min(255.0) as u8),
                ),
            );
        }
        painter.line_segment(
            points,
            Stroke::new(
                reverb_room::LINE_WIDTH * scale,
                Color32::from_rgba_unmultiplied(
                    reverb_room::LINE[0] as u8,
                    reverb_room::LINE[1] as u8,
                    reverb_room::LINE[2] as u8,
                    (255.0 * shade) as u8,
                ),
            ),
        );
    }
    response.on_hover_text("Rotating room: diffusion changes the sides, size changes the geometry, filters and decay change the tint, wet changes the glow.");
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));
}
