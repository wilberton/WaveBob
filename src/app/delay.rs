// Copyright (C) 2026 ItsAlphaNeon (reference display laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings-driven echo illustration and complete paged Delay controls.
use super::effect_panel::{self as panel, Dial, Layout};
use super::*;
use wavebob::{
    dsp::delay,
    processing::{self, Instance},
};
#[derive(Clone, Copy)]
pub(super) struct Control {
    pub index: usize,
    pub values: [i32; 26],
    pub bpm: f64,
}
impl Dial for Control {
    fn label(self) -> &'static str {
        match self.index {
            8 => "Rate",
            9 => "Time amount",
            10 => "Cutoff amount",
            _ => delay::NAMES[self.index],
        }
    }
    fn range(self) -> (f64, f64, f64) {
        let (a, b) = delay::RANGES[self.index];
        (a as f64, b as f64, 1.0)
    }
    fn reset(self) -> f64 {
        delay::DEFAULTS[self.index] as f64
    }
    fn value_font_size(self) -> f32 {
        match self.index {
            0 | 5 | 6 | 16 | 17 => 13.0,
            4 | 14 | 23 | 25 => 19.0,
            _ => 16.0,
        }
    }
    fn text(self, value: f64) -> String {
        let mut v = self.values;
        v[self.index] = value.round() as i32;
        let text = delay::value_text(self.index, &v, self.bpm);
        if self.index == 4 {
            let ms = text
                .split(" · ")
                .last()
                .unwrap()
                .trim_end_matches(" ms")
                .parse::<f64>()
                .unwrap();
            format!("{ms:.1} ms")
        } else if self.index == 16 || self.index == 21 {
            let hz = text.trim_end_matches(" Hz").parse::<f64>().unwrap();
            if hz >= 1000.0 {
                format!("{:.1} kHz", hz / 1000.0)
            } else {
                text
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
        if self.index == 4
            && self.values[2] != 0
            && let Some((beats, ticks)) = text.split_once(':')
        {
            return Some(beats.parse::<f64>().ok()? * 48.0 + ticks.parse::<f64>().ok()?);
        }
        if self.index == 22 && text.eq_ignore_ascii_case("max bits") {
            return Some(6000.0);
        }
        let mut target = text
            .split_whitespace()
            .next()?
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())?;
        if !matches!(self.index, 4 | 8 | 16 | 21 | 22) {
            return Some(match self.index {
                5 => (target / 200.0 + 0.5) * 6000.0,
                24 => target * 30.0,
                14 => target * 48.0,
                6 => target,
                _ => target * 60.0,
            });
        }
        if text.ends_with("kHz") {
            target *= 1000.0;
        }
        let millis = self.index == 4;
        let (lo, hi) = delay::RANGES[self.index];
        (lo..=hi)
            .filter_map(|raw| {
                let mut v = self.values;
                v[self.index] = raw;
                let formatted = delay::value_text(self.index, &v, self.bpm);
                let selected = if millis {
                    formatted.split(" · ").last()?
                } else {
                    formatted.as_str()
                };
                let number = selected
                    .split_whitespace()
                    .next()?
                    .trim_end_matches('%')
                    .parse::<f64>()
                    .ok()?;
                Some((raw, (number - target).abs()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(raw, _)| raw as f64)
    }
}
/// Approximate settings timing/routing; never an audio measurement. Bounded to eight echoes.
pub(super) fn echoes(v: &[i32; 26], bpm: f64) -> Vec<(f32, f32)> {
    if v[1] == 3 {
        return Vec::new();
    }
    let displayed = delay::value_text(4, v, bpm);
    let time = displayed
        .split(" · ")
        .last()
        .unwrap()
        .trim_end_matches(" ms")
        .parse::<f32>()
        .unwrap()
        / 1000.0;
    let pan = v[5] as f32 / 6000.0 - 0.5;
    let offset = pan.signum() * if v[1] == 0 { pan.abs() } else { pan * pan } * time;
    let feedback = v[14] as f32 / 6000.0 * 1.25;
    let mut result = Vec::with_capacity(16);
    for echo in 0..8 {
        let amplitude = feedback.powi(echo).min(1.5);
        if amplitude < 0.002 {
            break;
        }
        let t = time * (echo + 1) as f32;
        if v[1] == 2 {
            result.push((
                (t + if echo % 2 == 0 {
                    offset.max(0.0)
                } else {
                    (-offset).max(0.0)
                })
                .max(0.0),
                if echo % 2 == 0 { amplitude } else { -amplitude },
            ));
        } else {
            result.push(((t + offset.max(0.0)).max(0.0), amplitude));
            result.push(((t + (-offset).max(0.0)).max(0.0), -amplitude));
        }
    }
    result
}
impl WaveBob {
    pub(super) fn delay_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let values = delay::values(instance);
        let bpm = self.host.snapshot().arrangement.effect_tempo;
        egui::ScrollArea::both().auto_shrink([false,false]).show(ui,|ui| {
            let l=Layout::new(self.effect_panel(ui,window,instance,native));
            let pages=["Main","Modulation","Diffusion","Feedback shaper"];
            for (page,label) in pages.iter().enumerate() {
                if panel::segment(ui,l.r([209.0,299.0,407.0,507.0][page],63.0,[90.0,108.0,100.0,136.0][page],25.0),("delay-page",page),label,native.effect_page==page,l.scale).clicked() {self.finish_native_edit(native);native.effect_page=page;}
            }
            if native.effect_page==0 {
                for (index,x,y,r) in [(4,100.0,188.0,48.0),(14,100.0,399.0,48.0),(23,753.0,188.0,48.0),(25,753.0,399.0,48.0),(7,348.0,470.0,35.0),(24,505.0,470.0,35.0)] {
                    self.panel_dial(ui,window,instance,native,l,delay::IDS[index],Control{index,values,bpm},x,y,r);
                }
                if values[2]!=0 {ui.painter().text(l.p(100.0,219.0),egui::Align2::CENTER_CENTER,format!("{}:{:02}",values[4]/48,values[4]%48),panel::font(12.0*l.scale,false),MUTED);}
                if panel::segment(ui,l.r(212.0,311.0,153.0,24.0),"delay-details","Timing / filter",native.effect_details,l.scale).clicked() {self.finish_native_edit(native);native.effect_details = !native.effect_details;}
                if native.effect_details {
                    for (index,x,y) in [(0,257.0,154.0),(5,425.0,154.0),(6,592.0,154.0),(16,425.0,266.0),(17,592.0,266.0)] {
                        self.panel_dial(ui,window,instance,native,l,delay::IDS[index],Control{index,values,bpm},x,y,29.0);
                    }
                    self.panel_choice(ui,window,instance,native,l,"filter","Feedback filter",l.r(212.0,253.0,116.0,27.0),&[(0,"Low pass"),(1,"High pass"),(2,"Band pass"),(3,"Off")]);
                } else {
                    self.delay_echo_scene(ui,instance,native,l,&values,bpm);
                }
                self.panel_choice(ui,window,instance,native,l,"type","Model",l.r(212.0,361.0,132.0,28.0),&[(0,"Mono"),(1,"True stereo"),(2,"Ping pong"),(3,"Off")]);
                self.panel_switch(ui,window,instance,native,l,"tempo_sync","Tempo sync",365.0,361.0);
                let mut child=ui.new_child(egui::UiBuilder::new().id_salt("delay-tempo").max_rect(l.r(410.0,360.0,108.0,28.0)));
                let text=native.tempo_text.get_or_insert_with(||bpm.to_string());
                let response=child.add(egui::TextEdit::singleline(text).desired_width(64.0*l.scale).font(panel::font(13.0*l.scale,false)));
                l.text(ui,487.0,374.0,"BPM",12.0,MUTED);
                if (response.has_focus() || response.lost_focus()) && ui.input(|i|i.key_pressed(egui::Key::Enter)) {
                    if let Ok(bpm)=text.parse::<f64>() {self.dispatch(Command::SetEffectTempo(processing::TempoParams{bpm}));}
                    else {self.error=Some("Effect tempo must be 20..400 BPM".into());}
                    native.tempo_text=None;
                    response.surrender_focus();
                }
                if !response.has_focus() {native.tempo_text=None;}
                self.panel_switch(ui,window,instance,native,l,"keep_pitch","Keep pitch",553.0,361.0);
            } else {
                let indices:&[usize]=match native.effect_page {1=>&[8,9,10],2=>&[11,12],_=>&[18,19,20,21,22]};
                for (slot,&index) in indices.iter().enumerate() {
                    let (x,y)=if indices.len()<=3 {(286.0+slot as f32*139.0,304.0)} else {(270.0+(slot%3) as f32*148.0,217.0+(slot/3) as f32*220.0)};
                    self.panel_dial(ui,window,instance,native,l,delay::IDS[index],Control{index,values,bpm},x,y,39.0);
                }
                if native.effect_page==3 {self.panel_choice(ui,window,instance,native,l,"distortion_type","Feedback shaper",l.r(528.0,421.0,116.0,28.0),&[(0,"Limit"),(1,"Saturate")]);}
            }
            l.footer(ui,native,"Settings illustration · raw N for exact raw entry · feedback ≥100% uses the 60 s tail cap");
        });
    }
    fn delay_echo_scene(
        &mut self,
        ui: &egui::Ui,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        l: Layout,
        v: &[i32; 26],
        bpm: f64,
    ) {
        let plot = l.r(228.0, 110.0, 397.0, 171.0);
        let mut scene = visualization::Scene::default();
        scene.grid(
            plot,
            [8, 4],
            Stroke::new(l.scale, Color32::from_rgb(28, 48, 60)),
        );
        scene.path(
            vec![plot.left_center(), plot.right_center()],
            Stroke::new(l.scale, panel::TICK),
            None,
        );
        let points = echoes(v, bpm);
        let span = points.iter().map(|(t, _)| *t).fold(1.0_f32, f32::max);
        let alpha = if instance.bypassed { 64 } else { 235 };
        let blue = Color32::from_rgba_unmultiplied(0, 153, 255, alpha);
        for (time, amplitude) in points {
            let x = plot.left() + time / span * plot.width() * 0.94;
            let y = plot.center().y - amplitude.clamp(-1.5, 1.5) / 1.5 * plot.height() * 0.42;
            scene.path(
                vec![Pos2::new(x, plot.center().y), Pos2::new(x, y)],
                Stroke::new(1.8 * l.scale, blue),
                Some(visualization::Emission::new(blue, 2.2)),
            );
            scene.disc(
                Pos2::new(x, y),
                2.0 * l.scale,
                blue,
                Some(visualization::Emission::new(blue, 2.0)),
            );
            let trail: Vec<_> = (0..=32)
                .map(|i| {
                    let f = i as f32 / 32.0;
                    Pos2::new(
                        x + f * plot.width() * 0.09,
                        y + (plot.center().y - y) * f * f,
                    )
                })
                .collect();
            scene.path(trail, Stroke::new(l.scale, blue.gamma_multiply(0.6)), None);
        }
        self.panel_scene(
            ui,
            native,
            l,
            plot,
            visualization::revision(&(v, bpm, instance.bypassed)),
            scene,
        );
        l.text(ui, 211.0, 143.0, "L", 12.0, MUTED);
        l.text(ui, 211.0, 248.0, "R", 12.0, MUTED);
        for i in 0..=4 {
            ui.painter().text(
                l.p(228.0 + i as f32 * 99.25, 294.0),
                egui::Align2::CENTER_CENTER,
                format!("{:.2} s", span * i as f32 / 4.0),
                panel::font(10.0 * l.scale, false),
                MUTED,
            );
        }
    }
}
