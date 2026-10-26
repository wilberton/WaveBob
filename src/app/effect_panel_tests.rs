//! Real pointer input against the shared knob, including its centre value.
use super::*;
use crate::app::effect_panel::{Dial, dial};
use crate::app::reverb::Control;

struct Knob<C> {
    ctx: egui::Context,
    control: C,
    value: f64,
    time: f64,
    id: egui::Id,
    scale: f32,
    modifiers: egui::Modifiers,
    changed: bool,
    rendered_fraction: f64,
}
impl<C: Dial> Knob<C> {
    fn new(control: C, fraction: f64) -> Self {
        let ctx = egui::Context::default();
        theme(&ctx);
        let mut knob = Self {
            ctx,
            control,
            value: control.at_fraction(fraction),
            time: 0.0,
            id: egui::Id::NULL,
            scale: 1.0,
            modifiers: egui::Modifiers::NONE,
            changed: false,
            rendered_fraction: 0.0,
        };
        knob.frame(vec![]);
        knob
    }
    fn frame(&mut self, mut events: Vec<egui::Event>) {
        self.time += 0.1;
        events.insert(0, egui::Event::ModifiersChanged(self.modifiers));
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(600.0))),
                time: Some(self.time),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                let response = dial(
                    ui,
                    Pos2::new(300.0, 300.0),
                    46.0 * self.scale,
                    self.scale,
                    self.control,
                    &mut self.value,
                );
                self.id = response.id;
                self.changed = response.changed();
            },
        );
        output.textures_delta.clear();
        self.rendered_fraction = output
            .shapes
            .iter()
            .find_map(|clipped| {
                if let egui::Shape::LineSegment { points, stroke } = &clipped.shape
                    && stroke.color == crate::app::effect_panel::ACCENT
                    && (stroke.width - 6.4 * self.scale).abs() < 0.001
                {
                    let direction = points[1] - Pos2::new(300.0, 300.0);
                    let angle = direction.y.atan2(direction.x) as f64;
                    return Some(
                        (angle - std::f64::consts::PI * 0.75).rem_euclid(std::f64::consts::TAU)
                            / (std::f64::consts::PI * 1.5),
                    );
                }
                None
            })
            .unwrap();
    }
    fn begin(&mut self, position: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(position)]);
        self.frame(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    fn release(&mut self, position: Pos2) {
        self.frame(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    fn move_to(&mut self, position: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(position)]);
    }
    fn fraction(&self) -> f64 {
        self.control.fraction(self.value)
    }
}

#[test]
fn knob_value_double_click_resets_and_escape_discards_typed_draft() {
    let mut k = Knob::new(Control::Width, 0.1);
    let p = Pos2::new(300.0, 300.0);
    k.begin(p);
    k.release(p);
    k.begin(p);
    k.release(p);
    assert_eq!(
        k.value, 100.0,
        "Double-clicking the value must reset as well as double-clicking the ring"
    );
    assert!(
        k.ctx
            .data(|d| d.get_temp::<String>(k.id.with("entry")))
            .is_none()
    );
    // Let the double-click interval expire, then edit and cancel without preview.
    for _ in 0..5 {
        k.frame(vec![]);
    }
    k.begin(p);
    k.release(p);
    k.frame(vec![egui::Event::Text("75".into())]);
    assert!(!k.changed);
    k.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert_eq!(k.value, 100.0);
    assert!(!k.changed);
    assert!(
        k.ctx
            .data(|d| d.get_temp::<String>(k.id.with("entry")))
            .is_none()
    );
}

#[test]
fn knob_exact_entry_accepts_only_on_enter() {
    let mut k = Knob::new(Control::Width, 0.5);
    let p = Pos2::new(300.0, 300.0);
    k.begin(p);
    k.frame(vec![egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let entry = k.id.with("entry");
    assert!(
        k.ctx.data(|d| d.get_temp::<String>(entry)).is_some(),
        "A click must open exact entry"
    );
    k.frame(vec![egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    }]);
    k.frame(vec![egui::Event::Text("75".into())]);
    assert_eq!(k.value, 100.0);
    assert!(!k.changed, "Draft text must not start an audio preview");
    let text = k.ctx.data(|d| d.get_temp::<String>(entry));
    assert_eq!(text.as_deref(), Some("75"));
    k.frame(vec![egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert_eq!(k.value, 75.0);
}

#[test]
fn knob_horizontal_motion_over_the_value_never_edits() {
    let mut k = Knob::new(Control::Width, 0.5);
    k.begin(Pos2::new(300.0, 300.0));
    k.move_to(Pos2::new(340.0, 300.0));
    assert_eq!(
        k.fraction(),
        0.5,
        "The value and ring must share vertical-only dragging"
    );
}

#[test]
fn knob_multiframe_drag_is_linear_and_idle_frames_do_not_rewind() {
    let mut k = Knob::new(Control::Width, 0.0);
    let start = Pos2::new(345.0, 300.0);
    k.begin(start);
    for step in 1..=3 {
        k.move_to(start - Vec2::new(0.0, 30.0 * step as f32));
        assert!(
            (k.fraction() - step as f64 / 6.0).abs() < 1e-6,
            "90 points should advance half a sweep; step {step}, fraction {}",
            k.fraction()
        );
        let value = k.value;
        k.frame(vec![]);
        assert_eq!(
            k.value, value,
            "Holding still must retain the accumulated position"
        );
    }
}

#[test]
fn knob_value_and_ring_use_the_same_normalized_travel() {
    fn check<C: Dial>(control: C) {
        for offset in [Vec2::ZERO, Vec2::new(45.0, 0.0), Vec2::new(0.0, -68.0)] {
            let mut k = Knob::new(control, 0.0);
            let start = Pos2::new(300.0, 300.0) + offset;
            k.begin(start);
            k.move_to(start - Vec2::new(0.0, 90.0));
            assert!(
                (k.fraction() - 0.5).abs() < 1e-6,
                "{} at {offset:?}: {}",
                control.label(),
                k.fraction()
            );
            k.move_to(start - Vec2::new(0.0, 180.0));
            assert!((k.fraction() - 1.0).abs() < 1e-6);
        }
    }
    check(Control::Width);
    check(Control::Decay);
    check(Control::Damping);
    check(crate::app::fast_distortion::Control(1));
    check(crate::app::fast_distortion::Control(0));
    for i in 0..2 {
        check(crate::app::soft_clipper::Control(i));
        check(crate::app::balance::Control(i));
    }
    for slot in 0..4 {
        check(crate::app::parametric_eq::Control {
            slot,
            band: 0,
            shape: 0,
        });
    }
    for i in [0, 1, 2, 3, 4, 5, 10] {
        check(crate::app::chorus::Control(i));
    }
    for index in [
        0, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    ] {
        check(crate::app::delay::Control {
            index,
            values: wavebob::dsp::delay::DEFAULTS,
            bpm: 120.0,
        });
    }
    for i in [0, 1, 2, 3, 5, 7, 8, 9, 10, 15, 16, 17] {
        check(crate::app::limiter::Control(i));
    }
    for index in 0..4 {
        check(crate::app::stereo_enhancer::Control {
            index,
            direction: 1,
        });
    }
    for index in 0..6 {
        check(crate::app::stereo_shaper::Control {
            index,
            direction: 1,
        });
    }
    for i in [0, 1, 2, 3, 6, 7] {
        check(crate::app::transient_processor::Control(i));
    }
}

#[test]
fn knob_travel_is_independent_of_layout_scale_dpi_and_raw_quantization() {
    for scale in [0.7, 0.75, 1.0, 1.5] {
        for dpi in [1.0, 1.5] {
            let mut k = Knob::new(crate::app::fast_distortion::Control(1), 0.0);
            k.scale = scale;
            k.ctx.set_pixels_per_point(dpi);
            k.frame(vec![]);
            let start = Pos2::new(300.0, 300.0);
            k.begin(start);
            for y in 1..=180 {
                k.move_to(start - Vec2::new(0.0, y as f32));
                // The actual effect adapter publishes integer parameters each frame.
                k.value = k.value.round();
                k.frame(vec![]);
                if k.ctx
                    .data(|d| d.get_temp::<f64>(k.id.with("drag-fraction")))
                    .is_some()
                {
                    let expected = y as f64 / 180.0;
                    assert!(
                        (k.rendered_fraction - expected).abs() < 1e-5,
                        "The rendered dial must stay linear between raw steps: {expected} vs {}",
                        k.rendered_fraction
                    );
                }
            }
            assert_eq!(k.value, 10.0, "scale={scale}, dpi={dpi}");
            let accumulated = k
                .ctx
                .data(|d| d.get_temp::<f64>(k.id.with("drag-fraction")))
                .unwrap();
            assert!(
                (accumulated - 1.0).abs() < 1e-6,
                "The initial drag threshold must not lose travel"
            );
            k.modifiers.shift = true;
            k.move_to(start - Vec2::new(0.0, 90.0));
            assert!(
                (k.fraction() - 0.95).abs() < 1e-6,
                "scale={scale}, dpi={dpi}, fraction={}",
                k.fraction()
            );
            k.modifiers.shift = false;
            k.move_to(start - Vec2::new(0.0, 45.0));
            assert!(
                (k.fraction() - 0.7).abs() < 1e-6,
                "Changing fine mode must not jump to a different origin"
            );
        }
    }
}
