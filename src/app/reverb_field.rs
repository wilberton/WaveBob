//! Deterministic, parameter-driven reflection illustration (not an audio meter).
use super::visualization::{Emission, Scene};
use super::*;
use wavebob::processing::{Instance, ParameterValue};

pub(super) fn scene(rect: Rect, instance: &Instance) -> (Rect, Scene) {
    let rect = Rect::from_center_size(rect.center(), rect.size() * 0.86);
    let get = |id: &str| match instance.parameters[id] {
        ParameterValue::Integer(v) => v as f32,
        _ => 0.0,
    };
    let mut scene = Scene::default();
    let centre = rect.center();
    let size = get("size") / 100.0;
    let decay = get("decay") / 10.0;
    let width = 1.0 - get("separation") / 64.0;
    let damping = get("damping") / 221.0;
    let predelay = get("predelay") / 384.0;
    let scale = rect.width() / 436.0;
    let extent = Vec2::new(
        rect.width() * (0.37 + 0.12 * size) * (0.62 + 0.38 * width.min(1.5)),
        rect.height() * 0.48,
    );
    let blue = |alpha: f32| {
        Color32::from_rgba_unmultiplied(
            0,
            153,
            255,
            (alpha * if instance.bypassed { 0.30 } else { 1.0 }).clamp(0.0, 255.0) as u8,
        )
    };
    // These are broad, faint source shapes. All light is blurred by the renderer.
    for (extent, alpha) in [
        (Vec2::new(80.0, 32.0) * scale, 4.0),
        (Vec2::new(extent.x * 0.85, 3.0 * scale), 25.0),
    ] {
        scene.fill(
            (0..64)
                .map(|i| centre + Vec2::angled(i as f32 / 64.0 * std::f32::consts::TAU) * extent),
            Color32::TRANSPARENT,
            Some(Emission::new(blue(alpha), 1.0)),
        );
    }
    scene.disc(
        centre,
        4.5 * scale,
        Color32::TRANSPARENT,
        Some(Emission::new(blue(255.0), 5.0)),
    );
    for radius in [0.22, 0.43, 0.66, 0.83, 1.0] {
        for segment in 0..100 {
            let a = segment as f32 / 100.0 * std::f32::consts::TAU;
            let point = |angle: f32| centre + Vec2::angled(angle) * extent * radius;
            scene.path(
                [point(a), point(a + 0.025)],
                Stroke::new(0.65 * scale, blue(115.0)),
                None,
            );
        }
    }
    let noise = |seed: u32| {
        let mut x = seed.wrapping_mul(747796405).wrapping_add(2891336453);
        x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277803737);
        ((x >> 22) ^ x) as f32 / u32::MAX as f32
    };
    for ring in 0..22 {
        let r = 0.07 + ring as f32 * 0.043;
        let count = 40 + ring * 5;
        for dot in 0..count {
            let seed = ring * 503 + dot * 7 + 1;
            if noise(seed) < 0.30 {
                continue;
            }
            let angle = dot as f32 / count as f32 * std::f32::consts::TAU;
            let horizontal = angle.sin().abs();
            let radius = r + (noise(seed + 2) - 0.5) * 0.035 + predelay * 0.05;
            let point = centre + Vec2::angled(angle) * extent * radius;
            let fade = (1.0 - r * 0.65).powf(1.0 / (0.6 + decay * 0.2));
            let alpha = (140.0 + 115.0 * noise(seed + 3)) * fade * (1.0 - 0.45 * horizontal);
            scene.disc(
                point,
                (0.75 + noise(seed + 4) * 0.6) * scale,
                blue(alpha),
                Some(Emission::new(blue(alpha), 0.6)),
            );
            if dot % 4 == 0 && horizontal < 0.9 {
                let end = centre + Vec2::angled(angle + 0.035 + damping * 0.025) * extent * radius;
                scene.path([point, end], Stroke::new(0.85 * scale, blue(alpha)), None);
            }
        }
    }
    for dot in 0..380 {
        let x = noise(dot * 3 + 17) * 2.0 - 1.0;
        let spread = (noise(dot * 3 + 18) - 0.5) * (4.0 + 26.0 * (1.0 - x.abs()));
        let point = centre + Vec2::new(x * extent.x, spread * scale);
        let color = blue(130.0 + 125.0 * (1.0 - x.abs()));
        scene.disc(
            point,
            (0.6 + noise(dot * 3 + 19) * 0.7) * scale,
            color,
            Some(Emission::new(color, 1.0)),
        );
    }
    for (direction, length) in [
        (Vec2::X, rect.width() * 0.49),
        (Vec2::Y, rect.height() * 0.48),
    ] {
        let spacing = 11.0 * scale;
        let steps = (length / spacing).floor();
        scene.dotted_axis(
            centre - direction * steps * spacing,
            centre + direction * steps * spacing,
            spacing,
            1.5 * scale,
            blue(235.0),
            Some(Emission::new(blue(235.0), 0.7)),
        );
    }
    for (radius, alpha) in [(3.5, 100.0), (1.5, 255.0)] {
        scene.disc(centre, radius * scale, blue(alpha), None);
    }
    for direction in [Vec2::X, Vec2::Y] {
        scene.path(
            [
                centre - direction * 16.0 * scale,
                centre + direction * 16.0 * scale,
            ],
            Stroke::new(0.6 * scale, blue(130.0)),
            None,
        );
    }
    scene.disc(
        centre,
        1.3 * scale,
        Color32::from_rgba_unmultiplied(210, 242, 255, if instance.bypassed { 75 } else { 255 }),
        None,
    );
    (rect, scene)
}
