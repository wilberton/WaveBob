// Copyright (C) 2026 ItsAlphaNeon (reference balance/volume laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Two shared dials and the actual folding stereo matrix, not an audio meter.
use super::effect_panel::{self as panel, Dial, Layout};
use super::*;
use wavebob::{dsp::balance as dsp, processing::Instance};
#[derive(Clone, Copy)]
pub(super) struct Control(pub usize);
impl Dial for Control {
    fn label(self) -> &'static str {
        if self.0 == 0 { "Balance" } else { "Volume" }
    }
    fn range(self) -> (f64, f64, f64) {
        let (a, b) = dsp::RANGES[self.0];
        (a as f64, b as f64, 1.0)
    }
    fn reset(self) -> f64 {
        dsp::DEFAULTS[self.0] as f64
    }
    fn text(self, value: f64) -> String {
        let raw = value.round() as i32;
        if self.0 == 0 {
            if raw == 0 {
                "Centred".into()
            } else {
                format!(
                    "{:.1}% {}",
                    raw.unsigned_abs() as f64 / 128.0 * 100.0,
                    if raw < 0 { "L" } else { "R" }
                )
            }
        } else {
            dsp::value_text(1, raw)
                .split(" · ")
                .next()
                .unwrap()
                .to_owned()
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().to_lowercase().replace('−', "-");
        if let Some(raw) = text.strip_prefix("raw ") {
            return raw.parse::<f64>().ok().filter(|v| v.is_finite());
        }
        if self.0 == 0 {
            if matches!(text.as_str(), "centred" | "centered" | "centre" | "center") {
                return Some(0.0);
            }
            let left = text.contains("left") || text.starts_with('l') || text.ends_with('l');
            let right = text.contains("right") || text.starts_with('r') || text.ends_with('r');
            if left && right {
                return None;
            }
            let number = text
                .replace("left", "")
                .replace("right", "")
                .trim_matches(|c: char| c == 'l' || c == 'r' || c == '%' || c.is_whitespace())
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())?;
            Some(
                (if left {
                    -number.abs()
                } else if right {
                    number.abs()
                } else {
                    number
                }) * 1.28,
            )
        } else {
            if text.starts_with("-∞") || text.starts_with("-inf") || text == "mute" {
                return Some(0.0);
            }
            let db = text
                .trim_end_matches("db")
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())?;
            let target = 10.0_f64.powf(db.clamp(-120.0, 60.0) / 20.0);
            (0..=320)
                .min_by(|a, b| {
                    (dsp::gains(0, *a)[0] as f64 - target)
                        .abs()
                        .total_cmp(&(dsp::gains(0, *b)[0] as f64 - target).abs())
                })
                .map(|raw| raw as f64)
        }
    }
}
pub(super) fn matrix(values: [i32; 2]) -> [[f32; 2]; 2] {
    let [ll, rr, rl, lr] = dsp::gains(values[0], values[1]);
    [[ll, rl], [lr, rr]]
}
impl WaveBob {
    pub(super) fn balance_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let values = dsp::values(instance);
        egui::ScrollArea::both().auto_shrink([false,false]).show(ui,|ui| {
            let l=Layout::new(self.effect_panel(ui,window,instance,native));
            for (index,x) in [(0,100.0),(1,753.0)] {self.panel_dial(ui,window,instance,native,l,dsp::IDS[index],Control(index),x,268.0,56.0);}
            let source=l.r(235.0,150.0,380.0,250.0);let scene=routing::scene(source,matrix(values),l.scale,instance.bypassed);
            self.panel_scene(ui,native,l,source,visualization::revision(instance),scene);
            ui.painter().text(l.p(425.0,108.0),egui::Align2::CENTER_CENTER,"Stereo routing",panel::font(18.0*l.scale,true),panel::TEXT);
            for (y,input,output) in [(212.5,"L in","L out"),(337.5,"R in","R out")] {
                ui.painter().text(l.p(234.0,y),egui::Align2::RIGHT_CENTER,input,panel::font(13.0*l.scale,false),panel::TEXT);
                l.text(ui,616.0,y,output,13.0,panel::TEXT);
            }
            l.text(ui,39.0,329.0,"L",12.0,MUTED);l.text(ui,151.0,329.0,"R",12.0,MUTED);
            l.footer(ui,native,"Routing settings · attenuated input folds into the favored output · raw N for exact raw entry");
        });
    }
}
