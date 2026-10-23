//! Drawing primitives shared by the editor views. All audio comes from read models.
use super::*;

pub const WAVE: Color32 = Color32::from_rgb(88, 154, 241);
pub const TEAL: Color32 = Color32::from_rgb(75, 167, 177);
pub const GOLD: Color32 = Color32::from_rgb(205, 169, 72);

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Start,
    Loop,
    Minus,
    Plus,
    Fit,
}

pub fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(
                if matches!(icon, Icon::Play) {
                    46.0
                } else {
                    34.0
                },
                28.0,
            ),
            Sense::click(),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
        let painter = ui.painter_at(rect);
        let fill = if selected {
            BLUE
        } else if response.hovered() {
            Color32::from_rgb(40, 49, 58)
        } else {
            Color32::from_rgb(25, 32, 39)
        };
        painter.rect(
            rect,
            4.0,
            fill,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Inside,
        );
        let color = if ui.is_enabled() {
            Color32::from_rgb(216, 226, 240)
        } else {
            MUTED
        };
        let c = rect.center();
        let stroke = Stroke::new(1.5, color);
        match icon {
            Icon::Play => {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        c + Vec2::new(-4.0, -7.0),
                        c + Vec2::new(7.0, 0.0),
                        c + Vec2::new(-4.0, 7.0),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
            Icon::Start => {
                painter.line_segment(
                    [c + Vec2::new(-7.0, -6.0), c + Vec2::new(-7.0, 6.0)],
                    Stroke::new(2.0, color),
                );
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        c + Vec2::new(5.0, -6.0),
                        c + Vec2::new(-4.0, 0.0),
                        c + Vec2::new(5.0, 6.0),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
            Icon::Loop => {
                let r = Rect::from_center_size(c, Vec2::new(15.0, 10.0));
                painter.line_segment([r.left_center(), r.left_top()], stroke);
                painter.line_segment([r.left_top(), r.right_top()], stroke);
                painter.line_segment([r.right_center(), r.right_bottom()], stroke);
                painter.line_segment([r.right_bottom(), r.left_bottom()], stroke);
                for (tip, delta) in [(r.right_top(), -3.0), (r.left_bottom(), 3.0)] {
                    painter.line_segment([tip + Vec2::new(delta, -3.0), tip], stroke);
                    painter.line_segment([tip + Vec2::new(delta, 3.0), tip], stroke);
                }
            }
            Icon::Minus | Icon::Plus => {
                painter.circle_stroke(c - Vec2::splat(2.0), 5.5, stroke);
                painter.line_segment([c + Vec2::new(2.0, 2.0), c + Vec2::new(7.0, 7.0)], stroke);
                painter.hline(
                    (c.x - 5.0)..=(c.x + 1.0),
                    c.y - 2.0,
                    Stroke::new(1.0, color),
                );
                if matches!(icon, Icon::Plus) {
                    painter.vline(
                        c.x - 2.0,
                        (c.y - 5.0)..=(c.y + 1.0),
                        Stroke::new(1.0, color),
                    );
                }
            }
            Icon::Fit => {
                for (x, dx) in [(-6.0, 3.0), (6.0, -3.0)] {
                    for (y, dy) in [(-6.0, 3.0), (6.0, -3.0)] {
                        let p = c + Vec2::new(x, y);
                        painter.line_segment([p, p + Vec2::new(dx, 0.0)], stroke);
                        painter.line_segment([p, p + Vec2::new(0.0, dy)], stroke);
                    }
                }
            }
        }
        response
    })
    .inner
}

pub fn file_icon(painter: &egui::Painter, center: Pos2, folder: bool, color: Color32) {
    if folder {
        painter.rect_filled(
            Rect::from_center_size(center + Vec2::new(0.0, 1.0), Vec2::new(13.0, 9.0)),
            1.0,
            color,
        );
        painter.rect_filled(
            Rect::from_min_size(center - Vec2::new(6.5, 6.0), Vec2::new(6.0, 4.0)),
            1.0,
            color,
        );
    } else {
        for (x, height) in [(-6.0, 3.0), (-3.0, 6.0), (0.0, 9.0), (3.0, 5.0), (6.0, 2.0)] {
            painter.line_segment(
                [
                    center + Vec2::new(x, -height / 2.0),
                    center + Vec2::new(x, height / 2.0),
                ],
                Stroke::new(1.1, color),
            );
        }
    }
}

pub fn file_row(
    ui: &mut egui::Ui,
    name: &str,
    selected: bool,
    folder: bool,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 24.0),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        if selected || response.hovered() {
            painter.rect_filled(
                rect,
                3.0,
                if selected {
                    Color32::from_rgb(21, 74, 143)
                } else {
                    Color32::from_rgb(31, 40, 49)
                },
            );
        }
        let color = if enabled {
            Color32::from_rgb(207, 218, 235)
        } else {
            MUTED
        };
        file_icon(
            &painter,
            Pos2::new(rect.left() + 12.0, rect.center().y),
            folder,
            color,
        );
        let text = egui::WidgetText::from(name).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            rect.width() - 38.0,
            egui::TextStyle::Body,
        );
        painter.galley(
            Pos2::new(rect.left() + 28.0, rect.center().y - text.size().y / 2.0),
            text,
            color,
        );
        response
    })
    .inner
}

pub fn ruler(painter: &egui::Painter, rect: Rect, start: f64, end: f64) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(20, 26, 32));
    let span = (end - start).max(0.000001);
    let target = span / (rect.width() / 85.0).max(1.0) as f64;
    let decade = 10.0_f64.powf(target.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|factor| factor * decade)
        .find(|step| *step >= target)
        .unwrap();
    let first = (start / step).floor() as i64;
    for tick in first..=(end / step).ceil() as i64 {
        let seconds = tick as f64 * step;
        let x = rect.left() + ((seconds - start) / span) as f32 * rect.width();
        if x < rect.left() || x > rect.right() {
            continue;
        }
        let label = if step >= 1.0 {
            format!("{}:{:02}", seconds as u64 / 60, seconds as u64 % 60)
        } else {
            format!("{seconds:.2}")
        };
        painter.text(
            Pos2::new(x + 4.0, rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(10.0),
            MUTED,
        );
        painter.vline(
            x,
            (rect.bottom() - 5.0)..=rect.bottom(),
            Stroke::new(1.0, MUTED),
        );
        for minor in 1..5 {
            let mx = x + minor as f32 * step as f32 / span as f32 * rect.width() / 5.0;
            if mx <= rect.right() {
                painter.vline(
                    mx,
                    (rect.bottom() - 2.0)..=rect.bottom(),
                    Stroke::new(1.0, BORDER),
                );
            }
        }
    }
    painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, BORDER));
}

pub fn grid(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(17, 23, 29));
    for i in 0..=50 {
        let x = rect.left() + rect.width() * i as f32 / 50.0;
        painter.vline(
            x,
            rect.y_range(),
            Stroke::new(
                1.0,
                if i % 5 == 0 {
                    Color32::from_rgb(35, 44, 54)
                } else {
                    Color32::from_rgb(23, 31, 39)
                },
            ),
        );
    }
}

pub fn peaks(painter: &egui::Painter, rect: Rect, values: &[Peak], color: Color32) {
    let individual = !values.is_empty()
        && values
            .iter()
            .all(|peak| peak.end_frame - peak.start_frame == 1);
    let mut previous = None;
    for (index, peak) in values.iter().enumerate() {
        let x = rect.left()
            + (index as f32 + if individual { 0.0 } else { 0.5 }) * rect.width()
                / values.len() as f32;
        let y = |value: f64| rect.center().y - value as f32 * rect.height() * 0.45;
        if individual {
            let point = Pos2::new(x, y(peak.min));
            if let Some(previous) = previous {
                painter.line_segment([previous, point], Stroke::new(1.0, color));
            }
            if rect.width() / values.len() as f32 > 4.0 {
                painter.circle_filled(point, 2.0, color);
            }
            previous = Some(point);
            continue;
        }
        painter.line_segment(
            [Pos2::new(x, y(peak.min)), Pos2::new(x, y(peak.max))],
            Stroke::new((rect.width() / values.len() as f32).max(1.0), color),
        );
    }
    painter.hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(0.5, color.gamma_multiply(0.35)),
    );
}

pub fn waveform_canvas(
    ui: &mut egui::Ui,
    height: f32,
    wave: &WaveformData,
    rate: u32,
    overview: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let plot = if overview {
        rect.shrink(1.0)
    } else {
        Rect::from_min_max(rect.min + Vec2::new(1.0, 22.0), rect.max - Vec2::splat(1.0))
    };
    grid(&painter, plot);
    if overview {
        peaks(
            &painter,
            plot,
            &wave.channels[0],
            Color32::from_rgb(105, 132, 166),
        );
        painter.rect_filled(plot, 0.0, Color32::from_rgba_unmultiplied(0, 122, 255, 24));
        painter.rect_stroke(plot, 1.0, Stroke::new(1.0, BLUE), egui::StrokeKind::Inside);
        for x in [plot.left() + 2.0, plot.right() - 2.0] {
            let handle =
                Rect::from_center_size(Pos2::new(x, plot.center().y), Vec2::new(7.0, 28.0));
            painter.rect_filled(handle, 2.0, BLUE);
            painter.vline(
                x,
                (plot.center().y - 7.0)..=(plot.center().y + 7.0),
                Stroke::new(1.0, Color32::from_rgb(163, 210, 255)),
            );
        }
    } else {
        ruler(
            &painter,
            Rect::from_min_max(rect.min, Pos2::new(rect.right(), plot.top())),
            wave.start_frame as f64 / rate as f64,
            wave.end_frame as f64 / rate as f64,
        );
        for (channel, values) in wave.channels.iter().enumerate() {
            let channel_rect = Rect::from_min_size(
                plot.min
                    + Vec2::new(
                        0.0,
                        plot.height() * channel as f32 / wave.channels.len() as f32,
                    ),
                Vec2::new(plot.width(), plot.height() / wave.channels.len() as f32),
            );
            peaks(
                &painter,
                Rect::from_min_max(channel_rect.min + Vec2::new(14.0, 0.0), channel_rect.max),
                values,
                WAVE,
            );
            painter.hline(
                channel_rect.x_range(),
                channel_rect.bottom(),
                Stroke::new(1.0, BORDER),
            );
            painter.text(
                channel_rect.left_center() + Vec2::new(4.0, -12.0),
                egui::Align2::LEFT_CENTER,
                if wave.channels.len() == 1 {
                    "M"
                } else if channel == 0 {
                    "L"
                } else {
                    "R"
                },
                egui::FontId::proportional(10.0),
                MUTED,
            );
        }
        painter.vline(
            plot.left() + 1.0,
            plot.y_range(),
            Stroke::new(1.0, Color32::from_rgb(220, 230, 244)),
        );
    }
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, Color32::from_rgb(91, 104, 119)),
        egui::StrokeKind::Inside,
    );
    response
}

pub fn demo_clip(painter: &egui::Painter, rect: Rect, name: &str, color: Color32, seed: usize) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    painter.rect(
        rect,
        3.0,
        color.gamma_multiply(0.22),
        Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.min + Vec2::new(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(11.0),
        Color32::from_rgb(230, 235, 241),
    );
    let waveform = rect
        .shrink2(Vec2::new(2.0, 16.0))
        .translate(Vec2::new(0.0, 9.0));
    for i in 0..waveform.width() as usize {
        let phase = i as f32 * 0.31 + seed as f32 * 1.7;
        let envelope = (0.1 + (phase * 0.02).sin().abs() * 0.28) * (0.3 + phase.sin().abs() * 0.7);
        let x = waveform.left() + i as f32;
        painter.line_segment(
            [
                Pos2::new(x, waveform.center().y - envelope * waveform.height()),
                Pos2::new(x, waveform.center().y + envelope * waveform.height()),
            ],
            Stroke::new(1.0, color),
        );
    }
}
