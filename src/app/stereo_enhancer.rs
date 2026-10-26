// Copyright (C) 2026 ItsAlphaNeon (reference parameter/matrix laws)
// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic settings field; never a vectorscope or audio-phase analyzer.
use super::effect_panel::{self as panel, Dial, Layout};
use super::*;
use wavebob::{dsp::stereo_enhancer as dsp, processing::Instance};
#[derive(Clone, Copy)]
pub(super) struct Control {
    pub index: usize,
    pub direction: i32,
}
impl Dial for Control {
    fn label(self) -> &'static str {
        dsp::NAMES[self.index]
    }
    fn range(self) -> (f64, f64, f64) {
        let (a, b) = dsp::RANGES[self.index];
        (a as f64, b as f64, 1.0)
    }
    fn reset(self) -> f64 {
        dsp::DEFAULTS[self.index] as f64
    }
    fn text(self, value: f64) -> String {
        let raw = value.round() as i32;
        match self.index {
            0 => super::balance::Control(0).text(value),
            1 => dsp::value_text(1, raw)
                .split(" · ")
                .next()
                .unwrap()
                .to_owned(),
            2 => format!("{:.1}%", raw.unsigned_abs() as f64 / 128.0 * 100.0),
            _ => {
                let ms = (1001.0_f64.powf(raw.unsigned_abs() as f64 / 512.0) - 1.0) * 0.5;
                if ms >= 100.0 {
                    format!("{ms:.1} ms")
                } else {
                    format!("{ms:.2} ms")
                }
            }
        }
    }
    fn parse(self, text: &str) -> Option<f64> {
        let text = text.trim().to_lowercase().replace('−', "-");
        if let Some(raw) = text.strip_prefix("raw ") {
            return raw.parse::<f64>().ok().filter(|v| v.is_finite());
        }
        match self.index {
            0 => super::balance::Control(0).parse(&text),
            1 => {
                if text.starts_with("-inf") || text.starts_with("-∞") || text == "mute" {
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
                        (dsp::volume_gain(*a) as f64 - target)
                            .abs()
                            .total_cmp(&(dsp::volume_gain(*b) as f64 - target).abs())
                    })
                    .map(|v| v as f64)
            }
            2 => {
                if matches!(text.as_str(), "none" | "off") {
                    return Some(0.0);
                }
                let negative = text.contains("separated");
                let positive = text.contains("merged");
                if negative && positive {
                    return None;
                }
                let number = text
                    .replace("separated", "")
                    .replace("merged", "")
                    .trim()
                    .trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())?;
                Some(signed(number, negative, positive, self.direction) * 1.28)
            }
            _ => {
                if matches!(text.as_str(), "off" | "none") {
                    return Some(0.0);
                }
                let negative =
                    text.contains("left") || text.starts_with('l') || text.ends_with('l');
                let positive =
                    text.contains("right") || text.starts_with('r') || text.ends_with('r');
                if negative && positive {
                    return None;
                }
                let number = text
                    .replace("left", "")
                    .replace("right", "")
                    .replace("ms", "")
                    .trim_matches(|c: char| c.is_whitespace() || c == 'l' || c == 'r')
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())?;
                let sign = signed(number, negative, positive, self.direction).signum();
                Some(sign * (number.abs() * 2.0 + 1.0).ln() / 1001.0_f64.ln() * 512.0)
            }
        }
    }
}
fn signed(number: f64, negative: bool, positive: bool, current: i32) -> f64 {
    if negative {
        -number.abs()
    } else if positive {
        number.abs()
    } else if number < 0.0 || current < 0 {
        -number.abs()
    } else {
        number
    }
}
pub(super) fn field(rect: Rect, v: [i32; 6], scale: f32, bypassed: bool) -> visualization::Scene {
    let mut scene = visualization::Scene::default();
    let center = rect.center();
    let rx = rect.width() * 0.475;
    let ry = rect.height() * 0.47;
    let grid = Color32::from_rgba_unmultiplied(41, 87, 124, 100);
    scene.grid(
        rect.shrink(20.0 * scale),
        [8, 8],
        Stroke::new(0.5 * scale, grid.gamma_multiply(0.35)),
    );
    for fraction in [0.33, 0.66, 1.0] {
        scene.path(
            (0..=128).map(|i| {
                let a = i as f32 / 128.0 * std::f32::consts::TAU;
                center + Vec2::new(a.cos() * rx, a.sin() * ry) * fraction
            }),
            Stroke::new(0.7 * scale, grid),
            None,
        );
    }
    for (a, b) in [
        (rect.left_center(), rect.right_center()),
        (rect.center_top(), rect.center_bottom()),
        (
            rect.left_top() + Vec2::splat(12.0 * scale),
            rect.right_bottom() - Vec2::splat(12.0 * scale),
        ),
        (
            rect.right_top() + Vec2::new(-12.0, 12.0) * scale,
            rect.left_bottom() + Vec2::new(12.0, -12.0) * scale,
        ),
    ] {
        scene.dotted_axis(a, b, 5.0 * scale, 0.45 * scale, grid, None);
    }
    let [ll, rr, rl, lr] = dsp::gains(&v);
    let matrix = [[ll, rl], [lr, rr]];
    let volume = dsp::volume_gain(v[1]);
    let separation = v[2] as f32 / 128.0;
    let magnitude = v[3].unsigned_abs() as f32 / 512.0;
    let delayed = usize::from(v[3] >= 0);
    for (side, row) in matrix.iter().enumerate() {
        let gain = row[0].hypot(row[1]);
        if gain <= 1e-6 {
            continue;
        }
        let influence = if v[3] == 0 {
            0.0
        } else if v[4] == 1 {
            f32::from(side == delayed)
        } else {
            row[delayed].abs() / (row[0].abs() + row[1].abs()).max(1e-9)
        };
        let radius = rx
            * (gain / volume.max(1e-9)).clamp(0.0, 1.0).sqrt()
            * (1.0 - 0.12 * magnitude * influence);
        let height = rect.height() * 0.225 * (1.0 - separation);
        let direction = if side == 0 { -1.0 } else { 1.0 };
        let inverted = v[5] == side as i32 + 1;
        let color = if inverted {
            Color32::from_rgb(135, 170, 255)
        } else {
            panel::ACCENT
        }
        .gamma_multiply(gain.min(1.0).sqrt() * if bypassed { 0.3 } else { 1.0 });
        let points: Vec<_> = (0..=128)
            .map(|i| {
                let t = i as f32 / 128.0 * std::f32::consts::PI;
                center
                    + Vec2::new(
                        direction * t.sin() * radius,
                        (2.0 * t).sin() * height
                            + t.sin()
                                * height
                                * 0.35
                                * magnitude
                                * influence
                                * v[3].signum() as f32,
                    )
            })
            .collect();
        // Each lobe is convex, including its affine offset shear. Exclude the
        // repeated closing center; a fan of near-degenerate triangles makes
        // antialiasing fringes extend far beyond the intended surface.
        scene.fill(
            points.iter().take(128).copied(),
            color.gamma_multiply(0.025),
            None,
        );
        for contour in [0.42, 0.68] {
            scene.path(
                points.iter().map(|p| center + (*p - center) * contour),
                Stroke::new(scale, color.gamma_multiply(0.35)),
                None,
            );
        }
        let emission = Some(visualization::Emission::new(color, 1.6 * gain.min(2.0)));
        if inverted {
            for part in points.chunks(9) {
                scene.path(
                    part.iter().take(6).copied().collect::<Vec<_>>(),
                    Stroke::new(1.6 * scale, color),
                    emission,
                );
            }
        } else {
            scene.path(points.clone(), Stroke::new(1.6 * scale, color), emission);
        }
        for i in 0..96 {
            let p = points[i * 13 % 129]
                + Vec2::new((i * 29 % 17) as f32 - 8.0, (i * 37 % 19) as f32 - 9.0) * scale * 0.65;
            let dot = color.gamma_multiply(0.18);
            scene.disc(
                p,
                0.55 * scale,
                dot,
                Some(visualization::Emission::new(dot, 0.45 * gain.min(2.0))),
            );
        }
    }
    scene
}
impl WaveBob {
    pub(super) fn stereo_enhancer_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        let values = dsp::values(instance);
        egui::ScrollArea::both().auto_shrink([false,false]).show(ui,|ui| {
            let l=Layout::new(self.effect_panel(ui,window,instance,native));
            for(index,x,y)in[(2,100.0,188.0),(0,100.0,389.0),(3,753.0,188.0),(1,753.0,389.0)] {self.panel_dial(ui,window,instance,native,l,dsp::IDS[index],Control{index,direction:values[index].signum()},x,y,49.0);}
            let source=l.r(214.0,75.0,422.0,365.0);let scene=field(source,values,l.scale,instance.bypassed);
            self.panel_scene(ui,native,l,source,visualization::revision(instance),scene);
            for(x,y,label)in[(100.0,213.0,if values[2]<0 {"separated"}else if values[2]>0 {"merged"}else{"None"}),(753.0,213.0,if values[3]<0 {"Left"}else if values[3]>0 {"Right"}else{"Off"})] {ui.painter().text(l.p(x,y),egui::Align2::CENTER_CENTER,label,panel::font(12.0*l.scale,false),MUTED);}
            l.text(ui,223.0,88.0,"L",15.0,MUTED);ui.painter().text(l.p(627.0,88.0),egui::Align2::RIGHT_CENTER,"R",panel::font(15.0*l.scale,false),MUTED);
            self.panel_segments(ui,window,instance,native,l,"placement","Offset placement",l.r(245.0,481.0,134.0,28.0),&[(0,"Pre"),(1,"Post")]);
            self.panel_segments(ui,window,instance,native,l,"invert","Invert",l.r(439.0,481.0,184.0,28.0),&[(0,"None"),(1,"Left"),(2,"Right")]);
            l.footer(ui,native,"Settings illustration · numeric edits keep separation/offset direction · raw N for exact raw entry");
        });
    }
}
