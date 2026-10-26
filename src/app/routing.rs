//! Bounded stereo-matrix settings illustration, shared by Balance and Stereo Shaper.
//! Rows are outputs, columns inputs. Dashed blue-violet routes mean negative polarity.
use super::effect_panel::{ACCENT, BACKGROUND, TICK};
use super::*;
pub(super) fn scene(
    rect: Rect,
    matrix: [[f32; 2]; 2],
    scale: f32,
    bypassed: bool,
) -> visualization::Scene {
    let mut scene = visualization::Scene::default();
    let left = rect.left() + 12.0 * scale;
    let right = rect.right() - 12.0 * scale;
    let ys = [
        rect.top() + rect.height() * 0.25,
        rect.top() + rect.height() * 0.75,
    ];
    let opacity = if bypassed { 0.3 } else { 1.0 };
    for input in 0..2 {
        for output in 0..2 {
            let gain = matrix[output][input];
            let active = gain.abs() > 1e-6;
            let strength = gain.abs().clamp(0.0, 2.0);
            let alpha = if active {
                (30.0 + 210.0 * strength.min(1.0).sqrt()) as u8
            } else {
                52
            };
            let color = if !active {
                Color32::from_rgba_unmultiplied(67, 100, 125, alpha)
            } else if gain < 0.0 {
                Color32::from_rgba_unmultiplied(135, 166, 255, alpha)
            } else {
                Color32::from_rgba_unmultiplied(0, 153, 255, alpha)
            }
            .gamma_multiply(opacity);
            let emission = active.then_some(visualization::Emission::new(color, 1.4 * strength));
            let start = Pos2::new(left + 5.0 * scale, ys[input]);
            let end = Pos2::new(right - 7.0 * scale, ys[output]);
            let span = end.x - start.x;
            let a = start + Vec2::new(span * 0.45, 0.0);
            let b = end - Vec2::new(span * 0.45, 0.0);
            let points: Vec<_> = (0..=64)
                .map(|i| {
                    let t = i as f32 / 64.0;
                    let u = 1.0 - t;
                    (start.to_vec2() * u.powi(3)
                        + a.to_vec2() * 3.0 * u * u * t
                        + b.to_vec2() * 3.0 * u * t * t
                        + end.to_vec2() * t.powi(3))
                    .to_pos2()
                })
                .collect();
            let width = (if active {
                1.0 + 1.4 * strength.sqrt()
            } else {
                0.7
            }) * scale;
            if gain < -1e-6 {
                for chunk in points.chunks(9) {
                    scene.path(
                        chunk.iter().take(6).copied().collect::<Vec<_>>(),
                        Stroke::new(width, color),
                        emission,
                    );
                }
            } else {
                scene.path(points, Stroke::new(width, color), emission);
            }
            if active {
                scene.fill(
                    [
                        end,
                        end + Vec2::new(-7.0, -3.0) * scale,
                        end + Vec2::new(-7.0, 3.0) * scale,
                    ],
                    color,
                    emission,
                );
            }
        }
    }
    for (side, x) in [(0, left), (1, right)] {
        for channel in 0..2 {
            let sum = if side == 0 {
                matrix.iter().map(|row| row[channel].abs()).sum::<f32>()
            } else {
                matrix[channel].iter().map(|g| g.abs()).sum()
            };
            let active = sum > 1e-6;
            let strength = sum.clamp(0.0, 2.0);
            let color = if active {
                ACCENT.gamma_multiply(opacity)
            } else {
                TICK.gamma_multiply(opacity)
            };
            let center = Pos2::new(x, ys[channel]);
            let emission = active.then_some(visualization::Emission::new(color, strength * 1.7));
            scene.disc(center, 5.2 * scale, color, emission);
            scene.disc(center, 3.6 * scale, BACKGROUND, None);
            if active {
                let direction = if side == 0 { 1.0 } else { -1.0 };
                for i in 0..18 {
                    let fraction = i as f32 / 18.0;
                    let spread = ((i * 37 % 19) as f32 / 18.0 - 0.5) * 22.0;
                    let p =
                        center + Vec2::new(direction * (10.0 + fraction * 37.0), spread) * scale;
                    let dot = color.gamma_multiply((1.0 - fraction) * 0.45 * strength.min(1.0));
                    scene.disc(
                        p,
                        0.6 * scale,
                        dot,
                        Some(visualization::Emission::new(dot, 0.8 * strength)),
                    );
                }
            }
        }
    }
    scene
}
