use super::*;
use crate::app::effect_panel::Dial;
use crate::app::fast_distortion::{Control, transfer_points};
use wavebob::processing::{Action, EffectKind, Owner, ParameterValue};

struct Editor {
    h: Harness,
    _directory: tempfile::TempDir,
    owner: Owner,
    window: EditorWindow,
    native: effects::NativeEditor,
    ctx: egui::Context,
    time: f64,
    modifiers: egui::Modifiers,
}
impl Editor {
    fn new() -> Self {
        let (mut h, directory) = audio_harness();
        let owner = Owner::Document(h.app.active_document().unwrap().id);
        h.app.processing_action(
            &owner,
            Action::Add {
                kind: EffectKind::FastDistortion,
            },
        );
        let state = h.app.host.processing(owner.clone()).unwrap().committed;
        let window = EditorWindow {
            id: "distortion-input-test".into(),
            owner: owner.clone(),
            instance_id: state.effects[0].id.clone(),
            focus_generation: 1,
        };
        let ctx = egui::Context::default();
        theme(&ctx);
        Self {
            h,
            _directory: directory,
            owner,
            window,
            native: Default::default(),
            ctx,
            time: 0.0,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn frame(&mut self, mut events: Vec<egui::Event>) -> egui::FullOutput {
        events.insert(0, egui::Event::ModifiersChanged(self.modifiers));
        self.time += 0.1;
        let state = self.h.app.host.processing(self.owner.clone()).unwrap();
        let instance = &state.preview.as_ref().unwrap_or(&state.committed).effects[0];
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(850.0, 530.0))),
                time: Some(self.time),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                self.h
                    .app
                    .native_parameters(ui, &self.window, instance, &mut self.native)
            },
        );
        output.textures_delta.clear();
        output
    }
    fn pointer(&mut self, pos: Pos2, pressed: bool) {
        self.frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: self.modifiers,
        }]);
    }
    fn click(&mut self, pos: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        self.pointer(pos, true);
        self.pointer(pos, false);
    }
    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]);
    }
    fn parameter(&self, name: &str) -> ParameterValue {
        self.h
            .app
            .host
            .processing(self.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .parameters[name]
            .clone()
    }
    fn history(&self) -> usize {
        self.h.app.host.history().undo_count
    }
}

#[test]
fn distortion_exact_entry_uses_db_percent_and_reference_integer_steps() {
    let mut e = Editor::new();
    e.frame(vec![]);
    let before = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let text: Vec<_> = text_shapes(&e.frame(vec![]))
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    for label in [
        "Distortion",
        "Preamp",
        "Threshold",
        "Post gain",
        "Mix",
        "Type",
        "A",
        "B",
        "Bypass",
    ] {
        assert_eq!(
            text.iter().filter(|s| s.as_str() == label).count(),
            1,
            "{label}: {text:?}"
        );
    }
    assert_eq!(
        e.h.app.host.processing(e.owner.clone()).unwrap().committed,
        before
    );
    for (name, pos, input, expected) in [
        (
            "pre",
            Pos2::new(100.0, 194.0),
            "+6 dB",
            Control(0).parse("6").unwrap() as i32,
        ),
        (
            "post",
            Pos2::new(753.0, 194.0),
            "-6 dB",
            Control(4).parse("-6").unwrap() as i32,
        ),
        (
            "mix",
            Pos2::new(753.0, 393.0),
            "70%",
            Control(3).parse("70").unwrap() as i32,
        ),
        ("threshold", Pos2::new(100.0, 393.0), "7", 7),
    ] {
        e.frame(vec![]);
        e.click(pos);
        e.key(egui::Key::A, egui::Modifiers::CTRL);
        e.frame(vec![egui::Event::Text(input.into())]);
        let history = e.history();
        e.key(egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(
            e.parameter(name),
            ParameterValue::Integer(expected),
            "{name}"
        );
        assert_eq!(e.history(), history + 1, "{name} should commit once");
    }
    // Keyboard and reset on the outer ring use the very same shared dial.
    let pos = Pos2::new(150.0, 393.0);
    e.click(pos);
    e.key(egui::Key::ArrowRight, egui::Modifiers::NONE);
    assert_eq!(e.parameter("threshold"), ParameterValue::Integer(8));
    for _ in 0..2 {
        e.pointer(pos, true);
        e.pointer(pos, false);
    }
    assert_eq!(e.parameter("threshold"), ParameterValue::Integer(10));
}

#[test]
fn distortion_drag_preview_cancel_type_bypass_and_undo_preserve_processing() {
    let mut e = Editor::new();
    e.frame(vec![]);
    let original = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let history = e.history();
    let start = Pos2::new(145.0, 194.0);
    let end = start - Vec2::new(0.0, 32.0);
    e.frame(vec![egui::Event::PointerMoved(start)]);
    e.pointer(start, true);
    e.frame(vec![egui::Event::PointerMoved(end)]);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    assert_eq!(e.history(), history);
    e.pointer(end, false);
    assert_eq!(e.history(), history + 1);
    assert_ne!(e.parameter("pre"), ParameterValue::Integer(128));
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_eq!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects,
        original.effects
    );
    e.frame(vec![egui::Event::PointerMoved(start)]);
    e.pointer(start, true);
    e.frame(vec![egui::Event::PointerMoved(end)]);
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(end - Vec2::new(0.0, 15.0))]);
    e.pointer(end, false);
    let state = e.h.app.host.processing(e.owner.clone()).unwrap();
    assert!(state.preview.is_none());
    assert_eq!(state.committed.effects, original.effects);
    e.click(Pos2::new(470.0, 474.0));
    assert_eq!(e.parameter("type"), ParameterValue::Integer(1));
    assert_eq!(e.history(), history + 1);
    e.click(Pos2::new(414.0, 474.0));
    assert_eq!(e.parameter("type"), ParameterValue::Integer(0));
    assert_eq!(e.history(), history + 2);
    e.click(Pos2::new(740.0, 28.0));
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .bypassed
    );
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert!(
        !e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .bypassed
    );
}

#[test]
fn distortion_transfer_shows_headroom_and_exact_reference_values() {
    use wavebob::dsp::fast_distortion as dist;
    let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(380.0, 320.0));
    for kind in 0..=1 {
        let mut settings = dist::DEFAULTS;
        settings[2] = kind;
        let curve = dist::Curve::new(settings).unwrap();
        let (limit, points) = transfer_points(rect, &curve);
        assert!(limit >= curve.sample(1.0).abs());
        assert_eq!(points[256], rect.center());
        for (i, point) in points.iter().enumerate() {
            let output = (rect.center().y - point.y) * 2.0 / rect.height() * limit;
            assert!((output - curve.sample(-1.0 + i as f32 / 256.0)).abs() < 0.00001);
            assert!(rect.expand(0.001).contains(*point));
        }
    }
    assert_eq!(Control(3).parse("0%"), Some(0.0));
    assert_eq!(Control(3).parse("100%"), Some(128.0));
    assert_eq!(Control(4).parse("−∞ dB"), Some(0.0));
    assert_eq!(Control(0).parse("0 dB"), Some(128.0));
    assert!(Control(0).parse("NaN").is_none());
    // Validate inverse mapping independently of the parser implementation.
    let raw = Control(4).parse("-6 dB").unwrap() as i32;
    let actual = 20.0 * (dist::level(raw) as f64).log10();
    assert!((actual + 6.0).abs() < 0.15);
    let raw = Control(3).parse("70%").unwrap() as i32;
    assert!((dist::level(raw) - 0.7).abs() < 0.01);
}

#[test]
fn distortion_shift_drag_is_finer_and_type_supports_keyboard() {
    let mut increments = Vec::new();
    for fine in [false, true] {
        let mut e = Editor::new();
        e.modifiers.shift = fine;
        e.frame(vec![]);
        let start = Pos2::new(145.0, 194.0);
        let end = start - Vec2::new(0.0, 32.0);
        e.frame(vec![egui::Event::PointerMoved(start)]);
        e.pointer(start, true);
        e.frame(vec![egui::Event::PointerMoved(end)]);
        e.pointer(end, false);
        let ParameterValue::Integer(value) = e.parameter("pre") else {
            panic!()
        };
        increments.push(value - 128);
    }
    assert!(
        increments[1] > 0 && increments[1] < increments[0] / 2,
        "{increments:?}"
    );
    let mut e = Editor::new();
    e.frame(vec![]);
    e.click(Pos2::new(414.0, 474.0));
    let history = e.history();
    e.key(egui::Key::ArrowRight, egui::Modifiers::NONE);
    assert_eq!(e.parameter("type"), ParameterValue::Integer(1));
    assert_eq!(e.history(), history + 1);
}
