//! Shared native-editor input. Controls emit values; the host owns transactions.
use super::*;
pub(super) fn knob(
    ui: &mut egui::Ui,
    value: &mut f64,
    minimum: f64,
    maximum: f64,
    step: f64,
) -> egui::Response {
    let old = *value;
    let (rect, mut response) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::click_and_drag());
    if response.clicked() {
        response.request_focus();
    }
    let fine = ui.input(|input| if input.modifiers.shift { 0.1 } else { 1.0 });
    if response.dragged() {
        *value -=
            ui.input(|input| input.pointer.delta().y) as f64 * (maximum - minimum) / 160.0 * fine;
    }
    if response.has_focus() {
        let delta = ui.input_mut(|input| {
            if input.modifiers.command || input.modifiers.ctrl || input.modifiers.alt {
                return 0;
            }
            let modifiers = input.modifiers;
            let up = input.consume_key(modifiers, egui::Key::ArrowUp)
                || input.consume_key(modifiers, egui::Key::ArrowRight);
            let down = input.consume_key(modifiers, egui::Key::ArrowDown)
                || input.consume_key(modifiers, egui::Key::ArrowLeft);
            i32::from(up) - i32::from(down)
        });
        *value += delta as f64 * step * fine;
    }
    *value = value.clamp(minimum, maximum);
    let fraction = (*value - minimum) / (maximum - minimum);
    let center = rect.center();
    let radius = 19.0;
    let gold = Color32::from_rgb(245, 198, 75);
    ui.painter().circle_filled(center, radius, SURFACE);
    ui.painter().circle_stroke(
        center,
        radius,
        Stroke::new(
            if response.has_focus() { 2.0 } else { 1.0 },
            if response.has_focus() { gold } else { BORDER },
        ),
    );
    let angle = std::f32::consts::PI * (0.75 + fraction as f32 * 1.5);
    let direction = Vec2::angled(angle);
    ui.painter().line_segment(
        [center + direction * 10.0, center + direction * 17.0],
        Stroke::new(2.5, gold),
    );
    if *value != old {
        response.mark_changed();
    }
    response
}
pub(super) fn number(
    ui: &mut egui::Ui,
    value: &mut f64,
    minimum: f64,
    maximum: f64,
    unit: &str,
    integer: bool,
    fader: bool,
) -> egui::Response {
    let mut response = if fader {
        ui.add(egui::Slider::new(value, minimum..=maximum).show_value(false))
    } else {
        knob(
            ui,
            value,
            minimum,
            maximum,
            if integer {
                1.0
            } else {
                (maximum - minimum) / 1000.0
            },
        )
    };
    if integer {
        *value = value.round();
    }
    let exact = ui.add(
        egui::DragValue::new(value)
            .range(minimum..=maximum)
            .speed(if integer {
                1.0
            } else {
                (maximum - minimum) / 1000.0
            })
            .max_decimals(if integer { 0 } else { 4 })
            .suffix(unit),
    );
    response |= exact;
    response
}

/// Bounded visual input surface shared by parameter maps and audio displays.
pub(super) fn graph(ui: &mut egui::Ui, values: &[f32], height: f32) -> (Rect, egui::Response) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        Sense::click_and_drag(),
    );
    ui.painter().rect_filled(rect, 4.0, SURFACE);
    let plot = rect.shrink(8.0);
    for division in 1..4 {
        let y = plot.top() + plot.height() * division as f32 / 4.0;
        ui.painter().line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(0.5, BORDER),
        );
    }
    let count = values.len().min(128);
    let points: Vec<_> = values
        .iter()
        .take(count)
        .enumerate()
        .map(|(index, value)| {
            Pos2::new(
                plot.left() + plot.width() * index as f32 / (count.saturating_sub(1).max(1)) as f32,
                plot.bottom() - value.clamp(0.0, 1.0) * plot.height(),
            )
        })
        .collect();
    if points.len() > 1 {
        ui.painter().add(egui::Shape::line(
            points,
            Stroke::new(1.5, Color32::from_rgb(245, 198, 75)),
        ));
    }
    (plot, response)
}
