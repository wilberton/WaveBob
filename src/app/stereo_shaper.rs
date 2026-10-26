// Copyright (C) 2026 ItsAlphaNeon (reference matrix/delay/all-pass mappings)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Signed matrix controls and a settings routing diagram, main stereo output only.
use super::effect_panel::{self as panel, Dial, Layout};
use super::*;
use wavebob::{dsp::stereo_shaper as dsp, processing::Instance};

#[derive(Clone, Copy)]
pub(super) struct Control {
    pub index: usize,
    pub direction: i32,
}
impl Dial for Control {
    fn label(self) -> &'static str {
        ["R → L", "L → L", "R → R", "L → R", "Delay", "Phase"][self.index]
    }
    fn range(self) -> (f64, f64, f64) {
        let (lo, hi) = dsp::RANGES[self.index];
        (lo as f64, hi as f64, 1.0)
    }
    fn reset(self) -> f64 {
        dsp::DEFAULTS[self.index] as f64
    }
    fn neutral(self) -> Option<f64> {
        Some(0.0)
    }
    fn value_font_size(self) -> f32 {
        18.0
    }
    fn text(self, value: f64) -> String {
        let raw = value.round() as i32;
        if self.index < 4 {
            let p = raw as f64 / 128.0;
            if raw == 0 {
                "0%".into()
            } else if raw.unsigned_abs() < 128 {
                format!("{p:+.3}%")
            } else {
                format!("{p:+.1}%")
            }
        } else if raw == 0 {
            "Off".into()
        } else if self.index == 4 {
            let ms = (1001.0_f64.powf(raw.unsigned_abs() as f64 / 4096.0) - 1.0) * 0.5;
            if ms < 10.0 {
                format!("{ms:.3} ms")
            } else {
                format!("{ms:.1} ms")
            }
        } else {
            let hz = dsp::phase_hz(raw);
            if hz >= 1000.0 {
                format!("{:.2} kHz", hz / 1000.0)
            } else {
                format!("{hz:.1} Hz")
            }
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().to_lowercase().replace('−', "-");
        if let Some(raw) = text.strip_prefix("raw ") {
            return raw.parse::<f64>().ok().filter(|v| v.is_finite());
        }
        if self.index < 4 {
            return text
                .trim_end_matches('%')
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| v * 128.0);
        }
        if matches!(text.as_str(), "off" | "none") {
            return Some(0.0);
        }
        let left = text.contains("left") || text.starts_with('l') || text.ends_with('l');
        let right = text.contains("right") || text.starts_with('r') || text.ends_with('r');
        if left && right {
            return None;
        }
        let khz = text.contains("khz");
        let number = text
            .replace("left", "")
            .replace("right", "")
            .replace("khz", "")
            .replace("hz", "")
            .replace("ms", "")
            .trim_matches(|c: char| c.is_whitespace() || c == 'l' || c == 'r')
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())?;
        let sign = if left || (!right && (number < 0.0 || self.direction < 0)) {
            -1.0
        } else {
            1.0
        };
        if self.index == 4 {
            Some(sign * (number.abs() * 2.0 + 1.0).ln() / 1001.0_f64.ln() * 4096.0)
        } else if number == 0.0 {
            Some(0.0)
        } else {
            let target = number.abs() * if khz { 1000.0 } else { 1.0 };
            (1..=4096)
                .min_by(|a, b| {
                    (dsp::phase_hz(*a) as f64 - target)
                        .abs()
                        .total_cmp(&(dsp::phase_hz(*b) as f64 - target).abs())
                })
                .map(|v| v as f64 * sign)
        }
    }
}
pub(super) fn matrix(v: [i32; 7]) -> [[f32; 2]; 2] {
    let [ll, rr, rl, lr] = dsp::gains(&v);
    [[ll, rl], [lr, rr]]
}
impl WaveBob {
    pub(super) fn stereo_shaper_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let values = dsp::values(instance);
        egui::ScrollArea::both().auto_shrink([false,false]).show(ui,|ui| {
            let l=Layout::new(self.effect_panel(ui,window,instance,native));
            for (index,x,y,r) in [(1,100.0,188.0,49.0),(0,100.0,389.0,49.0),(2,753.0,188.0,49.0),(3,753.0,389.0,49.0),(4,337.0,389.0,47.0),(5,514.0,389.0,47.0)] {self.panel_dial(ui,window,instance,native,l,dsp::IDS[index],Control{index,direction:values[index].signum()},x,y,r);}
            for (x,title) in [(100.0,"Left output"),(753.0,"Right output")] {
                ui.painter().text(l.p(x,72.0),egui::Align2::CENTER_CENTER,title,panel::font(17.0*l.scale,true),panel::TEXT);
                for y in [255.0,456.0] {for (dx,text) in [(-54.0,"−200%"),(0.0,"0"),(54.0,"+200%")] {ui.painter().text(l.p(x+dx,y),egui::Align2::CENTER_CENTER,text,panel::font(10.0*l.scale,false),MUTED);}}
            }
            let source=l.r(235.0,72.0,380.0,218.0);
            self.panel_scene(ui,native,l,source,visualization::revision(instance),routing::scene(source,matrix(values),l.scale,instance.bypassed));
            for (y,label) in [(126.5,"L"),(235.5,"R")] {ui.painter().text(l.p(234.0,y),egui::Align2::RIGHT_CENTER,label,panel::font(14.0*l.scale,false),panel::TEXT);l.text(ui,616.0,y,label,14.0,panel::TEXT);}
            ui.painter().line_segment([l.p(195.0,284.0),l.p(655.0,284.0)],Stroke::new(l.scale,BORDER));
            for (x,index) in [(337.0,4),(514.0,5)] {ui.painter().text(l.p(x,414.0),egui::Align2::CENTER_CENTER,if values[index]<0 {"Left"}else if values[index]>0 {"Right"}else{"Off"},panel::font(12.0*l.scale,false),MUTED);}
            self.panel_segments(ui,window,instance,native,l,"position","Effects position",l.r(317.0,482.0,216.0,28.0),&[(-1,"Off"),(0,"Pre"),(1,"Post")]);
            l.footer(ui,native,"Routing settings · dashed = inverted polarity · Phase = all-pass corner · raw N for exact raw entry");
        });
    }
}
