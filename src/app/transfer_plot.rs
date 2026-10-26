//! Calibrated transfer-plot grid. Effect modules still sample their own DSP.
use super::effect_panel::font;
use super::*;
pub(super) fn grid(
    painter: &egui::Painter,
    plot: Rect,
    scale: f32,
    input_limit: f32,
    output_limit: f32,
) {
    let clipped = painter.with_clip_rect(plot);
    for i in 0..=16 {
        let t = i as f32 / 16.0;
        let color = if i % 4 == 0 {
            Color32::from_rgb(36, 58, 74)
        } else {
            Color32::from_rgb(21, 38, 49)
        };
        clipped.line_segment(
            [
                Pos2::new(plot.left() + t * plot.width(), plot.top()),
                Pos2::new(plot.left() + t * plot.width(), plot.bottom()),
            ],
            Stroke::new(0.5 * scale, color),
        );
        clipped.line_segment(
            [
                Pos2::new(plot.left(), plot.top() + t * plot.height()),
                Pos2::new(plot.right(), plot.top() + t * plot.height()),
            ],
            Stroke::new(0.5 * scale, color),
        );
    }
    let axis = Color32::from_rgb(103, 157, 197);
    for (a, b) in [
        (plot.left_center(), plot.right_center()),
        (plot.center_top(), plot.center_bottom()),
    ] {
        clipped.line_segment([a, b], Stroke::new(0.7 * scale, axis));
    }
    for i in 0..28 {
        let x = (-1.0 + i as f32 / 14.0) * input_limit;
        let map = |x: f32| {
            Pos2::new(
                plot.left() + (x / input_limit + 1.0) * plot.width() * 0.5,
                plot.center().y - x / output_limit * plot.height() * 0.5,
            )
        };
        clipped.line_segment(
            [map(x), map(x + 0.04 * input_limit)],
            Stroke::new(0.8 * scale, Color32::from_rgb(56, 86, 111)),
        );
    }
    for t in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        let text = |v: f32| {
            if v == 0.0 {
                "0".into()
            } else {
                format!("{v:.1}")
            }
        };
        painter.text(
            Pos2::new(
                plot.left() + (t + 1.0) * plot.width() * 0.5,
                plot.bottom() + 14.0 * scale,
            ),
            egui::Align2::CENTER_CENTER,
            text(t * input_limit),
            font(10.0 * scale, false),
            MUTED,
        );
        painter.text(
            Pos2::new(
                plot.left() - 8.0 * scale,
                plot.center().y - t * plot.height() * 0.5,
            ),
            egui::Align2::RIGHT_CENTER,
            text(t * output_limit),
            font(10.0 * scale, false),
            MUTED,
        );
    }
    painter.text(
        plot.center_bottom() + Vec2::new(0.0, 31.0) * scale,
        egui::Align2::CENTER_CENTER,
        "Input",
        font(11.0 * scale, false),
        MUTED,
    );
    painter.text(
        plot.left_top() - Vec2::new(0.0, 15.0) * scale,
        egui::Align2::LEFT_CENTER,
        "Output",
        font(11.0 * scale, false),
        MUTED,
    );
}
