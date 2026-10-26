//! Logical-pixel scenes. Effects own meaning; this module owns drawing and light.
//! A frame owns all submitted data until deferred painting has finished.
#![allow(dead_code)] // Plot helpers and review seams also serve the remaining effect editors.
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2};
mod gpu;
pub(super) use gpu::Renderer;

const MAX_SHAPES: usize = 16_384;
const MAX_POINTS: usize = 65_536;
const MAX_REGIONS: usize = 32;
pub(super) fn revision(value: &impl serde::Serialize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(value).unwrap_or_default().hash(&mut h);
    h.finish()
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Emission {
    color: Color32,
    intensity: f32,
}
impl Emission {
    pub fn new(color: Color32, intensity: f32) -> Self {
        Self {
            color,
            intensity: if intensity.is_finite() {
                intensity.clamp(0.0, 32.0)
            } else {
                0.0
            },
        }
    }
}

#[derive(Clone)]
pub(super) struct Mark {
    pub shape: Shape,
    pub emission: Option<Emission>,
}

/// Background/ordinary marks precede bloom; sharp marks and handles follow it.
#[derive(Clone, Default)]
pub(super) struct Scene {
    pub(super) marks: Vec<Mark>,
    pub(super) background: Vec<Shape>,
    points: usize,
}
impl Scene {
    fn push(&mut self, shape: Shape, emission: Option<Emission>, points: usize) {
        if self.marks.len() < MAX_SHAPES && self.points + points <= MAX_POINTS {
            self.points += points;
            self.marks.push(Mark { shape, emission });
        }
    }
    pub fn disc(&mut self, center: Pos2, radius: f32, color: Color32, emission: Option<Emission>) {
        if valid_point(center) && radius.is_finite() && radius >= 0.001 {
            self.push(
                Shape::circle_filled(center, radius.min(1024.0), color),
                emission,
                16,
            );
        }
    }
    pub fn points(
        &mut self,
        points: impl IntoIterator<Item = Pos2>,
        radius: f32,
        color: Color32,
        emission: Option<Emission>,
    ) {
        for point in points.into_iter().take(MAX_POINTS / 16) {
            self.disc(point, radius, color, emission);
        }
    }
    pub fn path(
        &mut self,
        points: impl IntoIterator<Item = Pos2>,
        stroke: Stroke,
        emission: Option<Emission>,
    ) {
        let points: Vec<_> = points.into_iter().take(MAX_POINTS).collect();
        if points.len() >= 2
            && points.iter().all(|p| valid_point(*p))
            && stroke.width.is_finite()
            && stroke.width >= 0.001
        {
            let len = points.len();
            self.push(
                Shape::line(points, Stroke::new(stroke.width.min(128.0), stroke.color)),
                emission,
                len,
            );
        }
    }
    /// Convex fills only: callers keep domain-specific polygon construction.
    pub fn fill(
        &mut self,
        points: impl IntoIterator<Item = Pos2>,
        color: Color32,
        emission: Option<Emission>,
    ) {
        let points: Vec<_> = points.into_iter().take(MAX_POINTS).collect();
        if points.len() >= 3 && points.iter().all(|p| valid_point(*p)) {
            let len = points.len();
            self.push(
                Shape::convex_polygon(points, color, Stroke::NONE),
                emission,
                len,
            );
        }
    }
    pub fn grid(&mut self, rect: Rect, divisions: [usize; 2], stroke: Stroke) {
        if !valid_rect(rect) {
            return;
        }
        for (axis, count) in divisions.into_iter().enumerate() {
            let count = count.min(128);
            for i in 0..=count {
                let t = i as f32 / count.max(1) as f32;
                let line = if axis == 0 {
                    [
                        Pos2::new(rect.left() + t * rect.width(), rect.top()),
                        Pos2::new(rect.left() + t * rect.width(), rect.bottom()),
                    ]
                } else {
                    [
                        Pos2::new(rect.left(), rect.top() + t * rect.height()),
                        Pos2::new(rect.right(), rect.top() + t * rect.height()),
                    ]
                };
                self.background.push(Shape::line_segment(line, stroke));
            }
        }
    }
    pub fn dotted_axis(
        &mut self,
        a: Pos2,
        b: Pos2,
        spacing: f32,
        radius: f32,
        color: Color32,
        emission: Option<Emission>,
    ) {
        if !a.is_finite() || !b.is_finite() || !spacing.is_finite() || spacing <= 0.0 {
            return;
        }
        let steps = ((b - a).length() / spacing).ceil().clamp(1.0, 4096.0) as usize;
        self.points(
            (0..=steps).map(|i| a.lerp(b, i as f32 / steps as f32)),
            radius,
            color,
            emission,
        );
    }
}

fn valid_rect(rect: Rect) -> bool {
    rect.is_finite() && rect.is_positive()
}
fn valid_point(p: Pos2) -> bool {
    p.is_finite() && p.x.abs() <= 1_000_000.0 && p.y.abs() <= 1_000_000.0
}

#[derive(Clone)]
pub(super) struct Region {
    pub id: egui::Id,
    pub revision: u64,
    pub clip: Rect,
    pub glow_clip: Rect,
    pub scene: Scene,
}
/// One collector per native viewport/frame. Submission never renders or blurs.
pub(super) struct Frame {
    pub viewport: egui::ViewportId,
    pub bounds: Rect,
    pub pixels_per_point: f32,
    pub regions: Vec<Region>,
    slot: Option<PaintSlots>,
}
impl Frame {
    pub fn new(viewport: egui::ViewportId, bounds: Rect, pixels_per_point: f32) -> Self {
        Self {
            viewport,
            bounds,
            pixels_per_point,
            regions: Vec::new(),
            slot: None,
        }
    }
    /// Reserve scene ordering at the first plot, before surrounding controls.
    pub fn place(&mut self, painter: &egui::Painter) {
        if self.slot.is_none() {
            let background = (0..MAX_REGIONS).map(|_| painter.add(Shape::Noop)).collect();
            let bloom = painter.add(Shape::Noop);
            let foreground = (0..MAX_REGIONS).map(|_| painter.add(Shape::Noop)).collect();
            self.slot = Some(PaintSlots {
                painter: painter.clone(),
                background,
                bloom,
                foreground,
            });
        }
    }
    pub fn submit(
        &mut self,
        id: egui::Id,
        revision: u64,
        clip: Rect,
        glow_clip: Rect,
        scene: Scene,
    ) -> Result<(), &'static str> {
        if self.regions.len() >= MAX_REGIONS {
            return Err("Visualization region limit exceeded");
        }
        if !valid_rect(self.bounds) || !valid_rect(clip) || !valid_rect(glow_clip) {
            return Ok(());
        }
        let clip = clip.intersect(self.bounds);
        let glow_clip = glow_clip.intersect(self.bounds);
        if !clip.is_positive() || !glow_clip.is_positive() {
            return Ok(());
        }
        if self.regions.iter().any(|r| r.id == id) {
            return Err("Duplicate visualization region");
        }
        self.regions.push(Region {
            id,
            revision,
            clip,
            glow_clip,
            scene,
        });
        Ok(())
    }
}
struct PaintSlots {
    painter: egui::Painter,
    background: Vec<egui::layers::ShapeIdx>,
    bloom: egui::layers::ShapeIdx,
    foreground: Vec<egui::layers::ShapeIdx>,
}

/// Bidirectional mapping also defines sharp geometry hit targets, never bloom.
#[derive(Clone, Copy)]
pub(super) struct Axis {
    min: f64,
    max: f64,
    logarithmic: bool,
}
impl Axis {
    pub fn linear(min: f64, max: f64) -> Option<Self> {
        Self::new(min, max, false)
    }
    pub fn logarithmic(min: f64, max: f64) -> Option<Self> {
        Self::new(min, max, true)
    }
    fn new(min: f64, max: f64, logarithmic: bool) -> Option<Self> {
        (min.is_finite() && max.is_finite() && max > min && (!logarithmic || min > 0.0)).then_some(
            Self {
                min,
                max,
                logarithmic,
            },
        )
    }
    pub fn fraction(self, value: f64) -> f32 {
        let value = if value.is_finite() {
            value.clamp(self.min, self.max)
        } else {
            self.min
        };
        if self.logarithmic {
            ((value / self.min).ln() / (self.max / self.min).ln()) as f32
        } else {
            ((value - self.min) / (self.max - self.min)) as f32
        }
    }
    pub fn value(self, fraction: f32) -> f64 {
        let t = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0) as f64
        } else {
            0.0
        };
        if self.logarithmic {
            self.min * (self.max / self.min).powf(t)
        } else {
            self.min + t * (self.max - self.min)
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Plot {
    pub rect: Rect,
    pub x: Axis,
    pub y: Axis,
}
impl Plot {
    pub fn point(self, x: f64, y: f64) -> Pos2 {
        self.rect.left_bottom()
            + Vec2::new(
                self.x.fraction(x) * self.rect.width(),
                -self.y.fraction(y) * self.rect.height(),
            )
    }
    pub fn value(self, p: Pos2) -> [f64; 2] {
        [
            self.x.value((p.x - self.rect.left()) / self.rect.width()),
            self.y
                .value((self.rect.bottom() - p.y) / self.rect.height()),
        ]
    }
    pub fn hit(self, pointer: Pos2, nodes: &[[f64; 2]], radius: f32) -> Option<usize> {
        nodes
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let d = self.point(p[0], p[1]).distance(pointer);
                (d <= radius).then_some((i, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|p| p.0)
    }
}

#[cfg(test)]
mod tests;
