// Copyright (C) 2026 ItsAlphaNeon (reference control/display laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings illustration of a generic transient; no audio measurement or editing.
use super::effect_panel::{self as panel, Dial, Layout};
use super::*;
use wavebob::{dsp::transient_processor as dsp, processing::Instance};

#[derive(Clone, Copy)]
pub(super) struct Control(pub usize);
impl Dial for Control {
    fn label(self) -> &'static str {
        dsp::NAMES[self.0]
    }
    fn range(self) -> (f64, f64, f64) {
        (0.0, 160.0, 1.0)
    }
    fn reset(self) -> f64 {
        dsp::DEFAULTS[self.0] as f64
    }
    fn neutral(self) -> Option<f64> {
        matches!(self.0, 0 | 2 | 3 | 7).then_some(80.0)
    }
    fn value_font_size(self) -> f32 {
        if self.0 == 7 { 16.0 } else { 18.0 }
    }
    fn text(self, value: f64) -> String {
        let raw = value.round() as i32;
        if self.0 == 6 {
            let hz = dsp::split_hz(dsp::position(6, raw));
            if hz >= 1000.0 {
                format!("{:.2} kHz", hz / 1000.0)
            } else {
                format!("{hz:.1} Hz")
            }
        } else if self.0 == 7 && raw != 80 {
            format!(
                "{:.0}% {}",
                (raw - 80).unsigned_abs() as f64 / 0.8,
                if raw < 80 { "low" } else { "high" }
            )
        } else {
            dsp::value_text(self.0, raw)
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().to_lowercase().replace('−', "-");
        if let Some(raw) = text.strip_prefix("raw ") {
            return raw.parse::<f64>().ok().filter(|v| v.is_finite());
        }
        if self.0 == 7 {
            if matches!(text.as_str(), "centred" | "centered" | "centre" | "center") {
                return Some(80.0);
            }
            let low = text.contains("low");
            let high = text.contains("high");
            if low && high {
                return None;
            }
            let n = text
                .replace("low", "")
                .replace("high", "")
                .trim()
                .trim_end_matches('%')
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())?;
            return Some(80.0 + if low { -n.abs() * 0.8 } else { n * 0.8 });
        }
        let khz = text.contains("khz");
        let target = text
            .replace("khz", "")
            .replace("hz", "")
            .replace("db", "")
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())?
            * if khz { 1000.0 } else { 1.0 };
        let physical = |raw| {
            let x = dsp::position(self.0, raw);
            match self.0 {
                0 => dsp::attack(x) as f64 * 100.0,
                2 => dsp::release(x) as f64 * 100.0,
                1 => 20.0 * (dsp::drive(x) as f64).log10(),
                3 => 20.0 * (dsp::gain(x) as f64).log10(),
                _ => dsp::split_hz(x),
            }
        };
        (0..=160)
            .min_by(|a, b| {
                (physical(*a) - target)
                    .abs()
                    .total_cmp(&(physical(*b) - target).abs())
            })
            .map(|v| v as f64)
    }
}
/// Relative envelope of a fixed generic transient, with explicitly illustrative
/// phase timing. Reference laws drive amounts, shape factors, band balance and
/// rational drive/output level, but this is not a detector simulation.
pub(super) fn envelopes(v: [i32; 9]) -> Vec<[f32; 2]> {
    let x = |i| dsp::position(i, v[i]);
    let a = dsp::attack(x(0));
    let r = dsp::release(x(2));
    let ash = dsp::shape(x(4)) as f32;
    let rsh = dsp::shape(x(5)) as f32;
    let drive = dsp::drive(x(1));
    let gain = dsp::gain(x(3));
    let low = (2.0 * (1.0 - x(7))).min(1.0);
    let high = (2.0 * x(7)).min(1.0);
    let h = 1000.0 / (1000.0_f32.powi(2) + (dsp::split_hz(x(6)) as f32).powi(2)).sqrt();
    let l = (1.0 - h * h).sqrt();
    (0..=400)
        .map(|i| {
            let t = i as f32 * 0.5;
            let rise = 1.0 / (1.0 + (-(t - 15.0) / 2.5).exp());
            let original = rise * (-(t - 20.0).max(0.0) / 34.0).exp() * 0.55;
            if v[8] == 0 {
                return [original, original];
            }
            let attack_window = (-((t - 20.0) / (6.0 * ash)).powi(2)).exp();
            let release_window = 1.0 - (-(t - 24.0).max(0.0) / (20.0 * rsh)).exp();
            let s = 2.0 * a * attack_window + 2.0 * r * release_window;
            let shaping = if s >= 0.0 { 1.0 + s } else { 1.0 / (1.0 - s) };
            let u = drive * original * shaping;
            let sat = if u > 3.0 {
                1.0
            } else {
                u * (u * u + 27.0) / (9.0 * u * u + 27.0)
            };
            [
                original,
                (low * l * original + high * h * sat / drive) * gain,
            ]
        })
        .collect()
}
pub(super) fn scene(rect: Rect, v: [i32; 9], scale: f32, bypassed: bool) -> visualization::Scene {
    let mut scene = visualization::Scene::default();
    let data = envelopes(v);
    let peak = data
        .iter()
        .flat_map(|p| *p)
        .fold(0.0_f32, f32::max)
        .max(0.55)
        * 1.3;
    scene.grid(
        rect,
        [10, 5],
        Stroke::new(0.5 * scale, panel::TICK.gamma_multiply(0.3)),
    );
    let point = |i: usize, value: f32| {
        Pos2::new(
            rect.left() + rect.width() * i as f32 / 400.0,
            rect.bottom() - value / peak * (rect.height() - 16.0 * scale),
        )
    };
    let active = v[8] != 0;
    let opacity = if bypassed || !active { 0.28 } else { 1.0 };
    let color = panel::ACCENT.gamma_multiply(opacity);
    scene.path(
        data.iter().enumerate().map(|(i, p)| point(i, p[0])),
        Stroke::new(1.2 * scale, MUTED.gamma_multiply(0.65)),
        None,
    );
    for x in [230.0, 284.0, 590.0] {
        let xx = rect.left() + (x - 218.0) * scale;
        scene.dotted_axis(
            Pos2::new(xx, rect.top() + 15.0 * scale),
            Pos2::new(xx, rect.bottom()),
            6.0 * scale,
            0.4 * scale,
            MUTED.gamma_multiply(0.45),
            None,
        );
    }
    for (x0, x1, y) in [(230.0, 284.0, 112.0), (284.0, 590.0, 151.0)] {
        let a = rect.min + Vec2::new(x0 - 218.0, y - 78.0) * scale;
        let b = rect.min + Vec2::new(x1 - 218.0, y - 78.0) * scale;
        let color = MUTED.gamma_multiply(0.65);
        scene.path([a, b], Stroke::new(0.7 * scale, color), None);
        for (p, d) in [(a, 1.0), (b, -1.0)] {
            scene.fill(
                [
                    p,
                    p + Vec2::new(4.0 * d, -2.5) * scale,
                    p + Vec2::new(4.0 * d, 2.5) * scale,
                ],
                color,
                None,
            );
        }
    }
    let points: Vec<_> = data
        .iter()
        .enumerate()
        .map(|(i, p)| point(i, p[1]))
        .collect();
    scene.path(
        points.clone(),
        Stroke::new(1.8 * scale, color),
        active.then_some(visualization::Emission::new(color, 1.5)),
    );
    if active {
        for i in 0..96 {
            let j = 12 + i * 4;
            let p = points[j] + Vec2::new(0.0, ((i * 37 % 17) as f32 - 8.0) * 0.8 * scale);
            let dot = color.gamma_multiply(0.12);
            scene.disc(
                p,
                0.55 * scale,
                dot,
                Some(visualization::Emission::new(dot, 0.5)),
            );
        }
    }
    scene
}
impl WaveBob {
    pub(super) fn transient_processor_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let v = dsp::values(instance);
        egui::ScrollArea::both().auto_shrink([false,false]).show(ui,|ui| {
            let l=Layout::new(self.effect_panel(ui,window,instance,native));
            for(index,x,y,r)in[(0,100.0,188.0,49.0),(2,100.0,389.0,49.0),(1,753.0,188.0,49.0),(3,753.0,389.0,49.0),(6,297.0,455.0,40.0),(7,444.0,455.0,40.0)] {self.panel_dial(ui,window,instance,native,l,dsp::IDS[index],Control(index),x,y,r);}
            let source=l.r(218.0,78.0,414.0,193.0);self.panel_scene(ui,native,l,source,visualization::revision(instance),scene(source,v,l.scale,instance.bypassed));
            l.text(ui,239.0,101.0,"Attack",10.0,MUTED);l.text(ui,425.0,140.0,"Release",10.0,MUTED);
            for(i,label,color)in[(0,"Original",MUTED),(1,"Shaped",panel::ACCENT)]{let y=91.0+i as f32*17.0;ui.painter().line_segment([l.p(555.0,y),l.p(570.0,y)],Stroke::new(1.5*l.scale,color));l.text(ui,575.0,y,label,11.0,color);}
            for (t,x) in [(0,218.0),(50,321.5),(100,425.0),(150,528.5),(200,632.0)] {ui.painter().text(l.p(x,285.0),egui::Align2::CENTER_CENTER,t.to_string(),panel::font(10.0*l.scale,false),MUTED);}
            ui.painter().text(l.p(425.0,305.0),egui::Align2::CENTER_CENTER,"Time (ms) · envelope illustration",panel::font(11.0*l.scale,false),MUTED);
            self.panel_segments(ui,window,instance,native,l,"attack_shape","Attack shape",l.r(211.0,337.0,207.0,28.0),&[(0,"Sharp"),(1,"Mid"),(2,"Slow")]);
            self.panel_segments(ui,window,instance,native,l,"release_shape","Release shape",l.r(439.0,337.0,207.0,28.0),&[(0,"Sharp"),(1,"Mid"),(2,"Slow")]);
            self.panel_segments(ui,window,instance,native,l,"on","Processing",l.r(550.0,455.0,90.0,28.0),&[(0,"Off"),(1,"On")]);
            l.footer(ui,native,"Envelope settings illustration · Attack/Release are shaping amounts · raw N for exact raw entry");
        });
    }
}
