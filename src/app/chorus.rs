// Copyright (C) 2026 ItsAlphaNeon (reference display and waveform laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Three settings-driven traces and the complete seven-dial Chorus composition.
use super::effect_panel::{self as panel, Dial, Layout};
use super::*;
use wavebob::{
    dsp::chorus,
    processing::{Instance, ParameterValue},
};
#[derive(Clone, Copy)]
pub(super) struct Control(pub usize);
impl Dial for Control {
    fn label(self) -> &'static str {
        match self.0 {
            3 => "LFO 1",
            4 => "LFO 2",
            5 => "LFO 3",
            10 => "Crossover",
            _ => chorus::NAMES[self.0],
        }
    }
    fn range(self) -> (f64, f64, f64) {
        let (a, b) = chorus::RANGES[self.0];
        (a as f64, b as f64, 1.0)
    }
    fn reset(self) -> f64 {
        chorus::DEFAULTS[self.0] as f64
    }
    fn value_font_size(self) -> f32 {
        if (3..=5).contains(&self.0) {
            16.0
        } else {
            19.0
        }
    }
    fn text(self, value: f64) -> String {
        let text = chorus::value_text(self.0, value.round() as i32);
        if self.0 == 10 && text.ends_with(" kHz") {
            format!(
                "{:.2} kHz",
                text.trim_end_matches(" kHz").parse::<f64>().unwrap()
            )
        } else {
            text.replace(" deg", "°")
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().replace('−', "-");
        if let Some(raw) = text.strip_prefix("raw ") {
            return raw.parse::<f64>().ok().filter(|v| v.is_finite());
        }
        let number = text
            .split_whitespace()
            .next()?
            .trim_end_matches('°')
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())?;
        let target = if text.ends_with("kHz") {
            number * 1000.0
        } else {
            number
        };
        Some(match self.0 {
            0 => (target - 0.05) / 29.95 * 1024.0,
            1 => target * 1000.0,
            2 => (target * 4096.0 / 360.0 + 1.0) / 2.0,
            3..=5 => (target.max(0.0) / 5.0).sqrt() * 5000.0,
            10 => ((target.max(0.001) / 440.0).log2() * 12.0 + 69.0) / 127.0 * 5000.0,
            _ => target,
        })
    }
}
/// Fixed illustrative phases, quantized reference waveform shapes and actual rate/depth settings.
/// This is not an audio-phase readback and never advances or reseeds DSP state.
#[allow(clippy::approx_constant)]
pub(super) fn trace(v: &[i32; 12], voice: usize) -> Vec<(f32, f32)> {
    let hz = 5.0 * (v[3 + voice] as f64 / 5000.0).powi(2);
    let shape = ((v[6 + voice] as f32 / 1024.0 * 3.0) as usize).min(3);
    (0..=384)
        .map(|i| {
            let seconds = i as f64 / 384.0 * 2.0;
            let phase = (seconds * hz + voice as f64 / 3.0).fract();
            let x = (((phase * 4096.0).floor() / 2048.0) as f32).min(1.999_511_7);
            let arc = if x < 1.0 {
                x * 4.0 * (1.0 - x)
            } else {
                (x - 1.0) * -4.0 * (2.0 - x)
            };
            let d = x as f64;
            let sines = (((3.1415 * d).sin() + ((9.4245 * d).sin() + (6.283 * d).sin()) / 3.0)
                * 3.0
                / (2.0 * 2.0_f64.sqrt() + 1.0)) as f32;
            let value = match shape {
                0 => arc,
                1 => arc * arc * arc,
                2 => sines,
                _ => sines * sines * sines,
            };
            (seconds as f32, value * v[1] as f32 / 5000.0 * 5.0)
        })
        .collect()
}
impl WaveBob {
    pub(super) fn chorus_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let v = chorus::values(instance);
        egui::ScrollArea::both().auto_shrink([false,false]).show(ui,|ui| {
            let l=Layout::new(self.effect_panel(ui,window,instance,native));
            for (index,x,y,r) in [(0,100.0,181.0,48.0),(1,100.0,387.0,48.0),(2,753.0,181.0,48.0),(10,753.0,369.0,48.0),(3,270.0,380.0,38.0),(4,425.0,380.0,38.0),(5,580.0,380.0,38.0)] {
                self.panel_dial(ui,window,instance,native,l,chorus::IDS[index],Control(index),x,y,r);
            }
            let plot=l.r(218.0,76.0,419.0,210.0);
            let mut scene=visualization::Scene::default();scene.grid(plot,[6,4],Stroke::new(l.scale,Color32::from_rgb(28,48,60)));
            let colors=[Color32::from_rgb(0,153,255),Color32::from_rgb(0,211,245),Color32::from_rgb(170,201,255)];
            for (voice,color) in colors.into_iter().enumerate() {
                let color=if instance.bypassed {color.gamma_multiply(0.3)}else{color};
                let points:Vec<_>=trace(&v,voice).into_iter().map(|(t,ms)|Pos2::new(plot.left()+t/2.0*plot.width(),plot.center().y-ms/5.0*plot.height()*0.5)).collect();
                match voice {0=>scene.path(points,Stroke::new(1.7*l.scale,color),Some(visualization::Emission::new(color,2.0))),1=>{for chunk in points.chunks(12) {scene.path(chunk.iter().take(7).copied().collect::<Vec<_>>(),Stroke::new(1.5*l.scale,color),Some(visualization::Emission::new(color,1.7)));}},_=>{for p in points.into_iter().step_by(5) {scene.disc(p,l.scale,color,Some(visualization::Emission::new(color,1.7)));}}}
            }
            self.panel_scene(ui,native,l,plot,visualization::revision(&(v,instance.bypassed)),scene);
            for (voice,color) in colors.into_iter().enumerate() {l.text(ui,270.0+voice as f32*155.0,89.0,(voice+1).to_string(),12.0,color);}
            for (y,text) in [(84.0,"+5 ms"),(181.0,"0"),(278.0,"−5 ms")] {l.text(ui,221.0,y,text,10.0,MUTED);}
            for (x,text,align) in [(218.0,"0 s",egui::Align2::LEFT_CENTER),(427.5,"1 s",egui::Align2::CENTER_CENTER),(637.0,"2 s",egui::Align2::RIGHT_CENTER)] {ui.painter().text(l.p(x,294.0),align,text,panel::font(10.0*l.scale,false),MUTED);}
            for x in [351.0,506.0] {ui.painter().line_segment([l.p(x,326.0),l.p(x,466.0)],Stroke::new(l.scale,BORDER));}
            for voice in 0..3 {
                let index=6+voice;let selected=[0,342,683,1024][((v[index] as f32/1024.0*3.0) as usize).min(3)];
                self.panel_choice_selected(ui,window,instance,native,l,chorus::IDS[index],"",l.r(207.0+voice as f32*155.0,438.0,137.0,28.0),&[(0,"Arc"),(342,"Sharp arc"),(683,"Sines"),(1024,"Sharp sines")],selected);
            }
            l.text(ui,680.0,444.0,"Band",13.0,panel::TEXT);
            self.chorus_regions(ui,window,instance,native,l,9,[(0,"Highs"),(100,"Lows")],v[9]>25,l.r(680.0,462.0,148.0,27.0));
            l.text(ui,300.0,496.0,"Output",14.0,panel::TEXT);
            self.chorus_regions(ui,window,instance,native,l,11,[(0,"Dry + wet"),(1024,"Wet only")],v[11]>256,l.r(366.0,482.0,244.0,28.0));
            l.footer(ui,native,"Settings modulation · fixed illustrative phases · right-click selectors for exact raw / reset");
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn chorus_regions(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
        l: Layout,
        index: usize,
        items: [(i32, &str); 2],
        selected: bool,
        rect: Rect,
    ) {
        let old = chorus::values(instance)[index];
        let mut value = old;
        let mut response = ui.interact(
            Rect::NOTHING,
            ui.scope_id().with(("chorus-switch", index)),
            Sense::hover(),
        );
        for (i, (raw, label)) in items.into_iter().enumerate() {
            let mut choice = panel::segment(
                ui,
                Rect::from_min_size(
                    rect.min + Vec2::new(rect.width() * i as f32 / 2.0, 0.0),
                    Vec2::new(rect.width() / 2.0, rect.height()),
                ),
                ("chorus-region", index, i),
                label,
                selected == (i == 1),
                l.scale,
            );
            if choice.clicked() {
                value = raw;
                choice.mark_changed();
            }
            response |= choice;
        }
        response = panel::integer_menu(ui, instance, chorus::IDS[index], &mut value, response);
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
    }
}
