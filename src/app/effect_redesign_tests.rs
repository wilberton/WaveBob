//! Real egui gestures and processing transactions for the effect redesigns.
use super::*;
use crate::app::effect_panel::Dial;
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
    size: Vec2,
}
impl Editor {
    fn new(kind: EffectKind) -> Self {
        Self::with_session(kind, wavebob::Session::default())
    }
    fn with_session(kind: EffectKind, session: wavebob::Session) -> Self {
        let (mut h, directory) = audio_harness_with_session(session);
        let owner = Owner::Document(h.app.active_document().unwrap().id);
        h.app.processing_action(&owner, Action::Add { kind });
        let state = h.app.host.processing(owner.clone()).unwrap().committed;
        let window = EditorWindow {
            id: "redesign-input".into(),
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
            size: Vec2::new(850.0, 530.0),
        }
    }
    fn frame(&mut self, mut events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.1;
        events.insert(0, egui::Event::ModifiersChanged(self.modifiers));
        let state = self.h.app.host.processing(self.owner.clone()).unwrap();
        let instance = &state.preview.as_ref().unwrap_or(&state.committed).effects[0];
        let viewport = egui::ViewportId::from_hash_of(&self.window.id);
        let mut input = egui::RawInput {
            viewport_id: viewport,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            focused: true,
            ..Default::default()
        };
        input.viewports.insert(
            viewport,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        let mut output = self.ctx.run_ui(input, |ui| {
            self.h.app.shortcuts(ui.ctx());
            self.native.help = None;
            self.h
                .app
                .native_parameters(ui, &self.window, instance, &mut self.native)
        });
        output.textures_delta.clear();
        output
    }
    fn pointer(&mut self, p: Pos2, pressed: bool) {
        self.frame(vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: self.modifiers,
        }]);
    }
    fn click(&mut self, p: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.pointer(p, true);
        self.pointer(p, false);
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
    fn settle_output(&mut self, output: &mut wavebob::transport::ManualOutput) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            self.h.app.host.begin_frame(&self.ctx);
            output.render_into(&mut []);
            let transport = self.h.app.host.transport();
            assert!(
                transport.processing.preparation_error.is_none(),
                "{transport:?}"
            );
            if !transport.pending && !transport.processing.pending {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "{transport:?}");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    fn exact(&mut self, p: Pos2, text: &str) {
        self.click(p);
        self.key(egui::Key::A, egui::Modifiers::CTRL);
        self.frame(vec![egui::Event::Text(text.into())]);
        self.key(egui::Key::Enter, egui::Modifiers::NONE);
    }
    fn parameter(&self, id: &str) -> ParameterValue {
        self.h
            .app
            .host
            .processing(self.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .parameters[id]
            .clone()
    }
    fn history(&self) -> usize {
        self.h.app.host.history().undo_count
    }
    fn start_drag(&mut self, p: Pos2, delta: Vec2) {
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.pointer(p, true);
        self.frame(vec![egui::Event::PointerMoved(p + delta)]);
    }
    fn set(&mut self, id: &str, value: ParameterValue) {
        self.h.app.processing_action(
            &self.owner,
            Action::SetParameter {
                effect_id: self.window.instance_id.clone(),
                parameter_id: id.into(),
                value,
            },
        );
    }
    fn eq_node(&mut self, band: usize) -> Pos2 {
        let max_y = self.size.y * 0.6;
        text_shapes(&self.frame(vec![]))
            .into_iter()
            .find(|(rect, text)| text == &(band + 1).to_string() && rect.bottom() < max_y)
            .unwrap()
            .0
            .center()
    }
    fn popup_choice(&mut self, open: Pos2, label: &str) {
        self.click(open);
        let output = self.frame(vec![]);
        let target = text_shapes(&output)
            .into_iter()
            .rev()
            .find(|(_, text)| text == label)
            .unwrap_or_else(|| {
                panic!(
                    "Missing popup choice {label} at {open:?}: {:?}",
                    text_shapes(&output)
                )
            })
            .0
            .center();
        self.click(target);
    }
}

#[test]
fn effect_transport_shortcuts_work_after_adjusting_a_focused_dial() {
    let (session, mut output) = wavebob::Session::with_manual_output(48000).unwrap();
    let mut e = Editor::with_session(EffectKind::SoftClipper, session);
    e.frame(vec![]);
    e.click(Pos2::new(150.0, 257.0));
    assert!(e.ctx.memory(|m| m.focused().is_some()));
    assert!(!e.ctx.text_edit_focused());
    e.key(egui::Key::ArrowLeft, egui::Modifiers::NONE);
    e.key(egui::Key::Space, egui::Modifiers::NONE);
    assert!(
        e.h.app.host.transport().requested.playing,
        "Space must preview audio while a non-text effect control has focus: {:?}",
        e.h.app.error
    );
    e.settle_output(&mut output);
    let mut pcm = [[0.0; 2]; 128];
    output.render_into(&mut pcm);
    assert!(pcm.iter().flatten().any(|v| *v != 0.0));
    e.key(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(e.h.app.host.transport().requested.frame, Some(0));
    assert!(e.h.app.host.transport().requested.playing);
    assert!(
        !e.ctx.text_edit_focused(),
        "Enter rewinds instead of opening entry"
    );
    e.settle_output(&mut output);
    assert_eq!(e.h.app.host.transport().applied.frame, 0);
    assert!(e.h.app.host.transport().applied.playing);
    e.key(egui::Key::Slash, egui::Modifiers::NONE);
    assert!(e.h.app.host.snapshot().loop_enabled);
    output.render_into(&mut pcm);
    e.key(egui::Key::Comma, egui::Modifiers::NONE);
    e.settle_output(&mut output);
    assert_eq!(e.h.app.host.transport().applied.frame, 0);
    e.frame(vec![egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    e.key(egui::Key::Space, egui::Modifiers::NONE);
    assert!(!e.h.app.host.transport().requested.playing);
    e.settle_output(&mut output);
    assert!(!e.h.app.host.transport().applied.playing);
}

#[test]
fn effect_transport_shortcuts_preserve_an_active_parameter_preview() {
    let (session, _output) = wavebob::Session::with_manual_output(48000).unwrap();
    let mut e = Editor::with_session(EffectKind::SoftClipper, session);
    e.frame(vec![]);
    let start = Pos2::new(150.0, 257.0);
    e.click(start);
    let history = e.history();
    let moved = Vec2::new(0.0, -24.0);
    e.start_drag(start, moved);
    let preview = e.h.app.host.processing(e.owner.clone()).unwrap().preview;
    assert!(preview.is_some());
    for key in [egui::Key::Space, egui::Key::Enter] {
        e.key(key, egui::Modifiers::NONE);
        assert!(e.h.app.host.transport().requested.playing);
        assert_eq!(
            e.h.app.host.processing(e.owner.clone()).unwrap().preview,
            preview
        );
        assert_eq!(e.history(), history);
    }
    e.pointer(start + moved, false);
    assert_eq!(e.history(), history + 1);
}

#[test]
fn effect_numeric_entry_suppresses_transport_and_accepts_enter() {
    let (session, mut output) = wavebob::Session::with_manual_output(48000).unwrap();
    let mut e = Editor::with_session(EffectKind::SoftClipper, session);
    e.frame(vec![]);
    e.click(Pos2::new(150.0, 257.0));
    e.key(egui::Key::F2, egui::Modifiers::NONE);
    assert!(e.ctx.text_edit_focused());
    let transport = e.h.app.host.transport().requested;
    for key in [egui::Key::Space, egui::Key::Comma, egui::Key::Slash] {
        e.key(key, egui::Modifiers::NONE);
        assert_eq!(e.h.app.host.transport().requested, transport);
    }
    assert!(!e.h.app.host.snapshot().loop_enabled);
    e.key(egui::Key::A, egui::Modifiers::CTRL);
    e.frame(vec![egui::Event::Text("-6 dB".into())]);
    e.key(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(!e.ctx.text_edit_focused());
    assert_eq!(e.h.app.host.transport().requested, transport);
    assert_eq!(
        e.parameter("threshold"),
        ParameterValue::Integer(soft_clipper::Control(0).parse("-6 dB").unwrap() as i32)
    );
    e.settle_output(&mut output);
}

#[test]
fn soft_clipper_dials_entry_preview_cancel_undo_and_bypass() {
    use crate::app::soft_clipper::Control;
    let mut e = Editor::new(EffectKind::SoftClipper);
    e.frame(vec![]);
    let before = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let source_revision = e.h.app.active_document().unwrap().revision_id;
    let output = e.frame(vec![]);
    let labels: Vec<_> = text_shapes(&output).into_iter().map(|(_, s)| s).collect();
    for label in ["Soft Clipper", "Threshold", "Post gain", "Bypass"] {
        assert_eq!(
            labels.iter().filter(|s| s.as_str() == label).count(),
            1,
            "{labels:?}"
        );
    }
    assert_eq!(
        before,
        e.h.app.host.processing(e.owner.clone()).unwrap().committed
    );
    let history = e.history();
    let start = Pos2::new(150.0, 257.0);
    let delta = Vec2::new(0.0, -24.0);
    e.start_drag(start, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    assert_eq!(e.history(), history);
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(start + delta * 2.0)]);
    e.pointer(start + delta * 2.0, false);
    assert_eq!(e.parameter("threshold"), ParameterValue::Integer(100));
    assert_eq!(e.history(), history);
    e.start_drag(start, delta);
    e.pointer(start + delta, false);
    assert_eq!(e.history(), history + 1);
    let changed = e.parameter("threshold");
    assert_ne!(changed, ParameterValue::Integer(100));
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_eq!(e.parameter("threshold"), ParameterValue::Integer(100));
    e.h.app.dispatch(Command::Redo(EmptyParams {}));
    assert_eq!(e.parameter("threshold"), changed);
    let count = e.history();
    e.exact(Pos2::new(100.0, 257.0), "-6 dB");
    assert_eq!(
        e.parameter("threshold"),
        ParameterValue::Integer(Control(0).parse("-6").unwrap() as i32)
    );
    assert_eq!(e.history(), count + 1);
    e.exact(Pos2::new(753.0, 257.0), "-inf");
    assert_eq!(e.parameter("post"), ParameterValue::Integer(0));
    e.exact(Pos2::new(753.0, 257.0), "60 dB");
    assert_eq!(e.parameter("post"), ParameterValue::Integer(160));
    e.click(Pos2::new(803.0, 257.0));
    e.key(egui::Key::ArrowLeft, egui::Modifiers::NONE);
    assert_eq!(e.parameter("post"), ParameterValue::Integer(159));
    for _ in 0..2 {
        e.pointer(Pos2::new(803.0, 257.0), true);
        e.pointer(Pos2::new(803.0, 257.0), false);
    }
    assert_eq!(e.parameter("post"), ParameterValue::Integer(128));
    let count = e.history();
    e.click(Pos2::new(742.0, 29.0));
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .bypassed
    );
    assert_eq!(e.history(), count + 1);
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
    e.set("threshold", ParameterValue::Integer(100));
    e.start_drag(start, delta);
    e.pointer(start + delta, false);
    let ParameterValue::Integer(coarse) = e.parameter("threshold") else {
        panic!()
    };
    e.set("threshold", ParameterValue::Integer(100));
    e.modifiers = egui::Modifiers::SHIFT;
    e.start_drag(start, delta);
    e.pointer(start + delta, false);
    e.modifiers = egui::Modifiers::NONE;
    let ParameterValue::Integer(fine) = e.parameter("threshold") else {
        panic!()
    };
    assert!((fine - 100).abs() < (coarse - 100).abs());
    e.set("threshold", ParameterValue::Integer(1));
    e.frame(vec![]);
    e.click(start);
    e.key(egui::Key::ArrowLeft, egui::Modifiers::NONE);
    assert_eq!(e.parameter("threshold"), ParameterValue::Integer(1));
    for _ in 0..2 {
        e.pointer(start, true);
        e.pointer(start, false);
    }
    assert_eq!(e.parameter("threshold"), ParameterValue::Integer(100));
    assert_eq!(
        e.h.app.active_document().unwrap().revision_id,
        source_revision
    );
}

#[test]
fn soft_clipper_graph_matches_dsp_headroom_and_unit_endpoints() {
    use crate::app::soft_clipper::{Control, transfer_points};
    let plot = Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(400.0, 300.0));
    for threshold in [1, 64, 100, 127] {
        for post in [0, 128, 160] {
            let (limit, points) = transfer_points(plot, threshold, post);
            assert!(limit >= 1.0);
            for (i, p) in points.iter().enumerate() {
                let x = -2.0 + i as f32 / 128.0;
                let y = (plot.center().y - p.y) / plot.height() * 2.0 * limit;
                assert!((y - wavebob::dsp::soft_clipper::curve(x, threshold, post)).abs() < 1e-5);
                assert!(plot.expand(0.001).contains(*p));
            }
        }
    }
    for raw in [1, 64, 100, 127] {
        let db = 20.0 * (raw as f64 / 128.0).log10();
        assert_eq!(Control(0).parse(&format!("{db:.6} dB")), Some(raw as f64));
    }
    for raw in [1, 64, 128, 160] {
        let db = 20.0 * (wavebob::dsp::soft_clipper::post_gain(raw) as f64).log10();
        assert_eq!(Control(1).parse(&format!("{db:.6}")), Some(raw as f64));
    }
    assert_eq!(Control(1).parse("−∞ dB"), Some(0.0));
    assert_eq!(Control(0).parse("NaN"), None);
}

#[test]
fn eq_graph_frequency_gain_and_escape_restore_the_visible_selected_band() {
    let mut e = Editor::new(EffectKind::ParametricEq);
    e.frame(vec![]);
    let before = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let history = e.history();
    let start = e.eq_node(1);
    let delta = Vec2::new(55.0, -22.0);
    e.start_drag(start, delta);
    let state = e.h.app.host.processing(e.owner.clone()).unwrap();
    let preview = &state.preview.as_ref().unwrap().effects[0];
    assert_ne!(
        preview.parameters["freq2"],
        before.effects[0].parameters["freq2"]
    );
    assert_ne!(
        preview.parameters["level2"],
        before.effects[0].parameters["level2"]
    );
    assert_eq!(e.native.eq_band, 1);
    assert_eq!(e.history(), history);
    let output = e.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(
        text_shapes(&output)
            .iter()
            .any(|(_, text)| text == "Frequency"),
        "Escape must redraw restored controls, not a blank panel"
    );
    let ParameterValue::Integer(original_freq) = before.effects[0].parameters["freq2"] else {
        panic!()
    };
    let expected = crate::app::parametric_eq::Control {
        slot: 0,
        band: 1,
        shape: 6,
    }
    .text(wavebob::dsp::parametric_eq::frequency_hz(original_freq));
    assert!(
        text_shapes(&output)
            .iter()
            .any(|(_, text)| text == &expected)
    );
    e.frame(vec![egui::Event::PointerMoved(start + delta * 2.0)]);
    e.pointer(start + delta * 2.0, false);
    assert_eq!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects,
        before.effects
    );
    assert_eq!(e.history(), history);
    let start = e.eq_node(1);
    e.start_drag(start, Vec2::new(40.0, 0.0));
    e.pointer(start + Vec2::new(40.0, 0.0), false);
    assert_eq!(e.history(), history + 1);
    assert_eq!(
        e.parameter("level2"),
        before.effects[0].parameters["level2"]
    );
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_eq!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects,
        before.effects
    );
    e.h.app.dispatch(Command::Redo(EmptyParams {}));
    let freq = e.parameter("freq4");
    let start = e.eq_node(3);
    e.start_drag(start, Vec2::new(0.0, 24.0));
    e.pointer(start + Vec2::new(0.0, 24.0), false);
    assert_eq!(e.parameter("freq4"), freq);
    assert_ne!(e.parameter("level4"), ParameterValue::Integer(0));
    e.exact(Pos2::new(100.0, 450.0), "2 kHz");
    assert_eq!(
        e.parameter("freq4"),
        ParameterValue::Integer(wavebob::dsp::parametric_eq::frequency_raw(2000.0))
    );
}

#[test]
fn eq_shift_drag_edits_only_reference_bandwidth_and_wheel_is_one_tick() {
    let mut e = Editor::new(EffectKind::ParametricEq);
    e.frame(vec![]);
    let before = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let history = e.history();
    let start = e.eq_node(4);
    e.modifiers = egui::Modifiers::SHIFT;
    let delta = Vec2::new(45.0, -18.0);
    e.start_drag(start, delta);
    e.pointer(start + delta, false);
    e.modifiers = egui::Modifiers::NONE;
    assert_eq!(e.native.eq_band, 4);
    assert_eq!(e.parameter("freq5"), before.effects[0].parameters["freq5"]);
    assert_eq!(
        e.parameter("level5"),
        before.effects[0].parameters["level5"]
    );
    assert_ne!(
        e.parameter("width5"),
        before.effects[0].parameters["width5"]
    );
    assert_eq!(e.history(), history + 1);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_eq!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects,
        before.effects
    );
    let point = e.eq_node(4);
    e.frame(vec![egui::Event::PointerMoved(point)]);
    e.frame(vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: Vec2::new(0.0, 30.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    }]);
    let value = e.parameter("width5");
    assert_ne!(value, before.effects[0].parameters["width5"]);
    let count = e.history();
    for _ in 0..3 {
        e.frame(vec![]);
    }
    assert_eq!(e.parameter("width5"), value);
    assert_eq!(e.history(), count);
}

#[test]
fn eq_coincident_nodes_stay_distinct_selectable_and_scaled() {
    for (size, ppp) in [
        (Vec2::new(850.0, 530.0), 1.0),
        (Vec2::new(680.0, 424.0), 1.5),
    ] {
        let mut e = Editor::new(EffectKind::ParametricEq);
        e.size = size;
        e.ctx.set_pixels_per_point(ppp);
        for band in 1..=7 {
            e.set(&format!("freq{band}"), ParameterValue::Integer(32768));
            e.set(&format!("level{band}"), ParameterValue::Integer(0));
        }
        e.frame(vec![]);
        let before = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
        let history = e.history();
        let centers: Vec<_> = (0..7).map(|i| e.eq_node(i)).collect();
        for a in 0..7 {
            for b in a + 1..7 {
                assert!(centers[a].distance(centers[b]) > 15.0, "{centers:?}");
            }
        }
        for (band, p) in centers.into_iter().enumerate() {
            e.click(p);
            assert_eq!(e.native.eq_band, band);
        }
        assert_eq!(e.history(), history);
        assert_eq!(
            e.h.app
                .host
                .processing(e.owner.clone())
                .unwrap()
                .committed
                .effects,
            before.effects
        );
    }
}

#[test]
fn eq_all_43_parameters_remain_reachable_and_response_uses_real_units() {
    use wavebob::dsp::parametric_eq as eq;
    let mut e = Editor::new(EffectKind::ParametricEq);
    e.frame(vec![]);
    assert_eq!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .parameters
            .len(),
        43
    );
    for band in 0..7 {
        let history = e.history();
        e.click(Pos2::new(97.0 + band as f32 * 50.0, 342.0));
        assert_eq!(e.native.eq_band, band);
        assert_eq!(e.history(), history);
        e.exact(Pos2::new(100.0, 450.0), "2 kHz");
        e.exact(Pos2::new(263.0, 450.0), "3.25 dB");
        e.exact(Pos2::new(426.0, 450.0), "50%");
        e.click(Pos2::new(535.0, 343.0));
        e.popup_choice(Pos2::new(585.0, 416.0), "Band stop");
        e.popup_choice(Pos2::new(585.0, 480.0), "Gentle 4");
        for (group, value) in [
            ("freq", ParameterValue::Integer(eq::frequency_raw(2000.0))),
            ("level", ParameterValue::Integer(325)),
            ("width", ParameterValue::Integer(32768)),
            ("enabled", ParameterValue::Boolean(false)),
            ("type", ParameterValue::Integer(4)),
            ("order", ParameterValue::Integer(1)),
        ] {
            assert_eq!(
                e.parameter(&eq::parameter_id(group, band)),
                value,
                "band {band} {group}"
            );
        }
    }
    e.exact(Pos2::new(754.0, 450.0), "-2.5 dB");
    assert_eq!(e.parameter("main"), ParameterValue::Integer(-250));
    e.set("enabled4", ParameterValue::Boolean(true));
    e.set("type4", ParameterValue::Integer(6));
    e.set("enabled1", ParameterValue::Boolean(true));
    e.set("type1", ParameterValue::Integer(5));
    let state = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let instance = &state.effects[0];
    let plot = Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(700.0, 230.0));
    for rate in [8000, 48000] {
        let response = eq::Response::new(instance, rate);
        let points = crate::app::parametric_eq::response_points(instance, rate, plot);
        for p in points {
            let hz = 20.0 * 1000.0_f64.powf(((p.x - plot.left()) / plot.width()) as f64);
            assert!(hz < rate as f64 * 0.5);
            let db = (plot.center().y - p.y) as f64 / plot.height() as f64 * 36.0;
            assert!((db - response.db(hz)).abs() < 0.01);
        }
    }
}

#[test]
fn eq_focused_switch_keeps_transport_shortcuts_available() {
    let (session, _output) = wavebob::Session::with_manual_output(48000).unwrap();
    let mut e = Editor::with_session(EffectKind::ParametricEq, session);
    e.frame(vec![]);
    e.click(Pos2::new(535.0, 343.0));
    assert_eq!(e.parameter("enabled4"), ParameterValue::Boolean(false));
    e.key(egui::Key::Space, egui::Modifiers::NONE);
    assert_eq!(e.parameter("enabled4"), ParameterValue::Boolean(false));
    assert!(e.h.app.host.transport().requested.playing);
}

#[test]
fn delay_pages_controls_and_reference_units_preserve_transactions() {
    use crate::app::delay::Control;
    use wavebob::dsp::delay;
    let mut e = Editor::new(EffectKind::Delay);
    let initial = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let history = e.history();
    for (page, x) in [(1, 370.0), (2, 479.0), (3, 586.0), (0, 262.0)] {
        e.click(Pos2::new(x, 75.0));
        assert_eq!(e.native.effect_page, page);
        assert_eq!(e.history(), history);
        assert_eq!(
            e.h.app.host.processing(e.owner.clone()).unwrap().committed,
            initial
        );
    }
    e.exact(Pos2::new(100.0, 188.0), "0:36");
    assert_eq!(e.parameter("time"), ParameterValue::Integer(36));
    e.click(Pos2::new(380.0, 372.0));
    assert_eq!(e.parameter("tempo_sync"), ParameterValue::Boolean(false));
    e.exact(Pos2::new(100.0, 188.0), "375 ms");
    let values = delay::values(
        &e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0],
    );
    let ms = delay::value_text(4, &values, 120.0)
        .trim_end_matches(" ms")
        .parse::<f64>()
        .unwrap();
    assert!((ms - 375.0).abs() < 2.0, "{ms}");
    e.exact(Pos2::new(100.0, 399.0), "125%");
    assert_eq!(e.parameter("feedback"), ParameterValue::Integer(6000));
    e.exact(Pos2::new(753.0, 188.0), "35%");
    assert_eq!(e.parameter("wet"), ParameterValue::Integer(2100));
    assert_eq!(e.parameter("dry"), ParameterValue::Integer(6000));
    e.exact(Pos2::new(753.0, 399.0), "27%");
    assert_eq!(e.parameter("dry"), ParameterValue::Integer(1620));
    e.exact(Pos2::new(348.0, 470.0), "65%");
    assert_eq!(e.parameter("spread"), ParameterValue::Integer(3900));
    e.exact(Pos2::new(505.0, 470.0), "-25%");
    assert_eq!(e.parameter("tone"), ParameterValue::Integer(-750));
    e.click(Pos2::new(570.0, 372.0));
    assert_eq!(e.parameter("keep_pitch"), ParameterValue::Boolean(true));
    e.click(Pos2::new(277.0, 323.0));
    assert!(e.native.effect_details);
    for (index, p) in [
        (0, Pos2::new(257.0, 154.0)),
        (5, Pos2::new(425.0, 154.0)),
        (6, Pos2::new(592.0, 154.0)),
        (16, Pos2::new(425.0, 266.0)),
        (17, Pos2::new(592.0, 266.0)),
    ] {
        e.exact(p, "raw 123");
        assert_eq!(e.parameter(delay::IDS[index]), ParameterValue::Integer(123));
    }
    e.popup_choice(Pos2::new(259.0, 266.0), "Band pass");
    assert_eq!(e.parameter("filter"), ParameterValue::Integer(2));
    e.popup_choice(Pos2::new(263.0, 374.0), "Off");
    assert_eq!(e.parameter("type"), ParameterValue::Integer(3));
    for (_page, x, indices) in [
        (1, 370.0, vec![8, 9, 10]),
        (2, 479.0, vec![11, 12]),
        (3, 586.0, vec![18, 19, 20, 21, 22]),
    ] {
        e.click(Pos2::new(x, 75.0));
        for (slot, index) in indices.iter().enumerate() {
            let p = if indices.len() <= 3 {
                Pos2::new(286.0 + slot as f32 * 139.0, 304.0)
            } else {
                Pos2::new(
                    270.0 + (slot % 3) as f32 * 148.0,
                    217.0 + (slot / 3) as f32 * 220.0,
                )
            };
            e.exact(p, "raw 124");
            assert_eq!(
                e.parameter(delay::IDS[*index]),
                ParameterValue::Integer(124)
            );
        }
    }
    e.popup_choice(Pos2::new(588.0, 433.0), "Limit");
    assert_eq!(e.parameter("distortion_type"), ParameterValue::Integer(0));
    let values = delay::DEFAULTS;
    for index in [
        0, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    ] {
        let c = Control {
            index,
            values,
            bpm: 120.0,
        };
        let (lo, hi, _) = c.range();
        assert_eq!(c.parse(&format!("raw {lo}")), Some(lo));
        assert_eq!(c.parse(&format!("raw {hi}")), Some(hi));
        assert_eq!(c.reset(), delay::DEFAULTS[index] as f64);
    }
}

#[test]
fn delay_echoes_are_bounded_model_timing_routing_and_feedback_settings() {
    use wavebob::dsp::delay;
    let mut v = delay::DEFAULTS;
    v[1] = 2;
    v[5] = 3000;
    v[14] = 3000;
    let a = crate::app::delay::echoes(&v, 120.0);
    let b = crate::app::delay::echoes(&v, 60.0);
    assert!(a.len() <= 8);
    assert!(a[0].1 > 0.0 && a[1].1 < 0.0);
    assert!((a[1].1.abs() - 0.625).abs() < 1e-6);
    assert!((b[0].0 - 2.0 * a[0].0).abs() < 1e-6);
    v[1] = 3;
    assert!(crate::app::delay::echoes(&v, 120.0).is_empty());
    v[1] = 1;
    v[14] = 0;
    assert_eq!(crate::app::delay::echoes(&v, 120.0).len(), 2);
}

#[test]
fn delay_dial_preview_fine_keyboard_reset_escape_and_undo() {
    let mut e = Editor::new(EffectKind::Delay);
    e.frame(vec![]);
    let history = e.history();
    let start = Pos2::new(145.0, 399.0);
    let delta = Vec2::new(0.0, -20.0);
    e.start_drag(start, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(start + delta * 2.0)]);
    e.pointer(start + delta * 2.0, false);
    assert_eq!(e.parameter("feedback"), ParameterValue::Integer(3000));
    assert_eq!(e.history(), history);
    e.start_drag(start, delta);
    e.pointer(start + delta, false);
    assert_eq!(e.history(), history + 1);
    let normal = e.parameter("feedback");
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    e.modifiers = egui::Modifiers::SHIFT;
    e.start_drag(start, delta);
    e.pointer(start + delta, false);
    e.modifiers = egui::Modifiers::NONE;
    let fine = e.parameter("feedback");
    assert_ne!(normal, fine);
    e.click(start);
    e.key(egui::Key::ArrowUp, egui::Modifiers::NONE);
    for _ in 0..2 {
        e.pointer(start, true);
        e.pointer(start, false);
    }
    assert_eq!(e.parameter("feedback"), ParameterValue::Integer(3000));
}

#[test]
fn chorus_all_controls_raw_regions_and_reference_units_are_preserved() {
    use crate::app::chorus::Control;
    use wavebob::dsp::chorus;
    let mut e = Editor::new(EffectKind::Chorus);
    let history = e.history();
    let initial = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let output = e.frame(vec![]);
    assert_eq!(e.history(), history);
    assert_eq!(
        e.h.app.host.processing(e.owner.clone()).unwrap().committed,
        initial
    );
    assert_eq!(e.parameter("wave1"), ParameterValue::Integer(23));
    assert!(text_shapes(&output).iter().any(|(_, s)| s == "Arc"));
    for (id, p, text, raw) in [
        ("delay", Pos2::new(100.0, 181.0), "30 ms", 1024),
        ("depth", Pos2::new(100.0, 387.0), "5 ms", 5000),
        ("stereo", Pos2::new(753.0, 181.0), "179.9°", 1024),
        ("cutoff", Pos2::new(753.0, 369.0), "raw 5000", 5000),
        ("speed1", Pos2::new(270.0, 380.0), "5 Hz", 5000),
        ("speed2", Pos2::new(425.0, 380.0), "raw 1", 1),
        ("speed3", Pos2::new(580.0, 380.0), "0.8 Hz", 2000),
    ] {
        e.exact(p, text);
        assert_eq!(e.parameter(id), ParameterValue::Integer(raw), "{id}");
    }
    for voice in 0..3 {
        for (raw, label) in [
            (342, "Sharp arc"),
            (683, "Sines"),
            (1024, "Sharp sines"),
            (0, "Arc"),
        ] {
            e.popup_choice(Pos2::new(270.0 + voice as f32 * 155.0, 452.0), label);
            assert_eq!(
                e.parameter(chorus::IDS[6 + voice]),
                ParameterValue::Integer(raw)
            );
        }
    }
    e.click(Pos2::new(791.0, 475.0));
    assert_eq!(e.parameter("cross"), ParameterValue::Integer(100));
    e.click(Pos2::new(550.0, 495.0));
    assert_eq!(e.parameter("wet"), ParameterValue::Integer(1024));
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_eq!(e.parameter("wet"), ParameterValue::Integer(0));
    for index in [0, 1, 2, 3, 4, 5, 10] {
        let c = Control(index);
        let (lo, hi, _) = c.range();
        assert_eq!(c.parse(&format!("raw {lo}")), Some(lo));
        assert_eq!(c.parse(&format!("raw {hi}")), Some(hi));
        assert_eq!(c.reset(), chorus::DEFAULTS[index] as f64);
    }
}

#[test]
fn chorus_wave_selector_has_exact_raw_entry_reset_and_keeps_default_region() {
    let mut e = Editor::new(EffectKind::Chorus);
    e.frame(vec![]);
    let history = e.history();
    let open = |e: &mut Editor| {
        let p = Pos2::new(270.0, 452.0);
        e.frame(vec![egui::Event::PointerMoved(p)]);
        for pressed in [true, false] {
            e.frame(vec![egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    };
    open(&mut e);
    let output = e.frame(vec![]);
    assert_eq!(e.history(), history);
    let raw = text_shapes(&output)
        .into_iter()
        .find(|(_, s)| s == "23")
        .unwrap()
        .0
        .center();
    e.exact(raw, "270");
    assert_eq!(
        e.parameter("wave1"),
        ParameterValue::Integer(270),
        "{:?} {:?}",
        e.h.app.host.processing(e.owner.clone()),
        text_shapes(&e.frame(vec![]))
    );
    let output = e.frame(vec![]);
    let reset = text_shapes(&output)
        .into_iter()
        .find(|(_, s)| s == "Reset")
        .unwrap()
        .0
        .center();
    e.click(reset);
    assert_eq!(e.parameter("wave1"), ParameterValue::Integer(23));
    assert_eq!(e.history(), history + 2);
}

#[test]
fn chorus_trace_uses_bounded_rate_wave_depth_and_static_phase_settings() {
    use wavebob::dsp::chorus;
    let mut v = chorus::DEFAULTS;
    let initial = crate::app::chorus::trace(&v, 0);
    assert_eq!(initial.len(), 385);
    assert_eq!(initial, crate::app::chorus::trace(&v, 0));
    assert!(
        initial
            .iter()
            .all(|(t, y)| *t >= 0.0 && *t <= 2.0 && y.abs() <= 2.25)
    );
    v[3] = 5000;
    assert_ne!(initial, crate::app::chorus::trace(&v, 0));
    let fast = crate::app::chorus::trace(&v, 0);
    v[6] = 1024;
    assert_ne!(fast, crate::app::chorus::trace(&v, 0));
    v[1] = 0;
    assert!(
        crate::app::chorus::trace(&v, 0)
            .iter()
            .all(|(_, y)| *y == 0.0)
    );
}

#[test]
fn chorus_dial_preview_escape_undo_keyboard_reset_and_bypass() {
    let mut e = Editor::new(EffectKind::Chorus);
    e.frame(vec![]);
    let history = e.history();
    let p = Pos2::new(158.0, 181.0);
    let delta = Vec2::new(0.0, -24.0);
    e.start_drag(p, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(p + delta * 2.0)]);
    e.pointer(p + delta * 2.0, false);
    assert_eq!(e.parameter("delay"), ParameterValue::Integer(512));
    assert_eq!(e.history(), history);
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    assert_eq!(e.history(), history + 1);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_eq!(e.parameter("delay"), ParameterValue::Integer(512));
    e.click(p);
    e.key(egui::Key::ArrowUp, egui::Modifiers::NONE);
    assert_eq!(e.parameter("delay"), ParameterValue::Integer(513));
    for _ in 0..2 {
        e.pointer(p, true);
        e.pointer(p, false);
    }
    assert_eq!(e.parameter("delay"), ParameterValue::Integer(512));
    e.click(Pos2::new(742.0, 29.0));
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .bypassed
    );
}

#[test]
fn limiter_pages_compare_and_complete_controls_preserve_processing() {
    use wavebob::dsp::limiter;
    let mut e = Editor::new(EffectKind::Limiter);
    e.frame(vec![]);
    let initial = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    let history = e.history();
    e.click(Pos2::new(286.0, 75.0));
    e.click(Pos2::new(210.0, 75.0));
    e.click(Pos2::new(585.0, 75.0));
    assert_eq!(e.history(), history);
    assert_eq!(
        e.h.app.host.processing(e.owner.clone()).unwrap().committed,
        initial
    );
    e.exact(Pos2::new(82.0, 183.0), "3 dB");
    assert_ne!(e.parameter("gain"), ParameterValue::Integer(1000));
    e.click(Pos2::new(648.0, 75.0));
    assert_eq!(e.parameter("gain"), ParameterValue::Integer(1000));
    assert_eq!(e.history(), history + 2);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_ne!(e.parameter("gain"), ParameterValue::Integer(1000));
    for (index, p) in [
        (0, Pos2::new(82.0, 183.0)),
        (1, Pos2::new(82.0, 374.0)),
        (2, Pos2::new(768.0, 183.0)),
        (7, Pos2::new(768.0, 374.0)),
        (3, Pos2::new(270.0, 412.0)),
        (5, Pos2::new(430.0, 412.0)),
        (16, Pos2::new(590.0, 412.0)),
        (4, Pos2::new(200.0, 503.0)),
        (6, Pos2::new(300.0, 503.0)),
        (15, Pos2::new(402.0, 503.0)),
        (17, Pos2::new(501.0, 503.0)),
    ] {
        let raw = if matches!(index, 4 | 6) { 7 } else { 123 };
        e.exact(p, &format!("raw {raw}"));
        assert_eq!(
            e.parameter(limiter::IDS[index]),
            ParameterValue::Integer(raw),
            "limit {index}"
        );
    }
    e.click(Pos2::new(600.0, 502.0));
    assert_eq!(e.parameter("exact_ceiling"), ParameterValue::Boolean(true));
    e.click(Pos2::new(286.0, 75.0));
    for (index, p) in [
        (8, Pos2::new(270.0, 412.0)),
        (9, Pos2::new(430.0, 412.0)),
        (10, Pos2::new(590.0, 412.0)),
        (11, Pos2::new(200.0, 503.0)),
        (12, Pos2::new(300.0, 503.0)),
        (14, Pos2::new(402.0, 503.0)),
        (13, Pos2::new(501.0, 503.0)),
    ] {
        let raw = if index == 13 { 8 } else { 321 };
        e.exact(p, &format!("raw {raw}"));
        assert_eq!(
            e.parameter(limiter::IDS[index]),
            ParameterValue::Integer(raw),
            "comp {index}"
        );
    }
    let history = e.history();
    e.click(Pos2::new(367.0, 75.0));
    let output = e.frame(vec![]);
    assert!(
        text_shapes(&output)
            .iter()
            .any(|(_, s)| s == "Noise threshold")
    );
    assert_eq!(e.history(), history);
}

#[test]
fn limiter_reference_conversions_and_preview_escape_undo_are_exact() {
    use crate::app::limiter::Control;
    use wavebob::dsp::limiter;
    for index in 0..18 {
        let c = Control(index);
        let (a, b, _) = c.range();
        assert_eq!(c.parse(&format!("raw {a}")), Some(a));
        assert_eq!(c.parse(&format!("raw {b}")), Some(b));
        assert_eq!(c.reset(), limiter::DEFAULTS[index] as f64);
    }
    assert_eq!(Control(0).parse("0 dB").unwrap().round(), 1000.0);
    assert_eq!(Control(2).parse("-inf"), Some(0.0));
    assert_eq!(Control(9).parse("20:1").unwrap().round(), 1000.0);
    assert_eq!(Control(9).parse("1:4").unwrap().round(), -1000.0);
    assert_eq!(Control(10).parse("-100%"), Some(-1000.0));
    let mut e = Editor::new(EffectKind::Limiter);
    e.frame(vec![]);
    let history = e.history();
    let p = Pos2::new(133.0, 183.0);
    let delta = Vec2::new(0.0, -23.0);
    e.start_drag(p, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(p + delta * 2.0)]);
    e.pointer(p + delta * 2.0, false);
    assert_eq!(e.parameter("gain"), ParameterValue::Integer(1000));
    assert_eq!(e.history(), history);
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    assert_eq!(e.history(), history + 1);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    assert_eq!(e.parameter("gain"), ParameterValue::Integer(1000));
}

#[test]
fn limiter_layers_and_history_span_are_presentation_only_and_fields_reset() {
    let mut e = Editor::new(EffectKind::Limiter);
    e.frame(vec![]);
    let history = e.history();
    let state = e.h.app.host.processing(e.owner.clone()).unwrap().committed;
    for x in [205.0, 270.0, 346.0, 440.0] {
        e.click(Pos2::new(x, 317.0));
    }
    e.exact(Pos2::new(620.0, 316.0), "1.0");
    let out = e.frame(vec![]);
    assert!(text_shapes(&out).iter().any(|(_, s)| s == "-1.00 s"));
    assert_eq!(e.history(), history);
    assert_eq!(
        e.h.app.host.processing(e.owner.clone()).unwrap().committed,
        state
    );
    e.exact(Pos2::new(200.0, 503.0), "7");
    assert_eq!(e.parameter("att_curve"), ParameterValue::Integer(7));
    for _ in 0..2 {
        e.pointer(Pos2::new(200.0, 503.0), true);
        e.pointer(Pos2::new(200.0, 503.0), false);
    }
    e.frame(vec![]);
    assert_eq!(e.parameter("att_curve"), ParameterValue::Integer(3));
}

#[test]
fn balance_controls_units_fold_matrix_mute_and_source_are_preserved() {
    use crate::app::balance::{Control, matrix};
    let mut e = Editor::new(EffectKind::Balance);
    let history = e.history();
    let source = e.h.app.active_document().unwrap().revision_id;
    e.frame(vec![]);
    assert_eq!(e.history(), history);
    assert_eq!(Control(0).parse("Centred"), Some(0.0));
    assert_eq!(Control(0).parse("100% L"), Some(-128.0));
    assert_eq!(Control(0).parse("100% right"), Some(128.0));
    assert_eq!(Control(0).parse("-25%"), Some(-32.0));
    assert_eq!(Control(1).parse("0 dB"), Some(256.0));
    assert_eq!(Control(1).parse("mute"), Some(0.0));
    for (index, lo, hi) in [(0, -128, 128), (1, 0, 320)] {
        let c = Control(index);
        assert_eq!(c.parse(&format!("raw {lo}")), Some(lo as f64));
        assert_eq!(c.parse(&format!("raw {hi}")), Some(hi as f64));
    }
    e.exact(Pos2::new(100.0, 268.0), "100% L");
    assert_eq!(e.parameter("balance"), ParameterValue::Integer(-128));
    let left = matrix([-128, 256]);
    assert!((left[0][0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    assert_eq!(left[0][0], left[0][1]);
    assert!(left[1].iter().all(|g| g.abs() < 1e-6));
    e.exact(Pos2::new(100.0, 268.0), "100% R");
    assert_eq!(e.parameter("balance"), ParameterValue::Integer(128));
    let right = matrix([128, 256]);
    assert!(right[0].iter().all(|g| g.abs() < 1e-6));
    assert!((right[1][0] - right[1][1]).abs() < 1e-6);
    e.exact(Pos2::new(753.0, 268.0), "-6 dB");
    assert_eq!(
        e.parameter("volume"),
        ParameterValue::Integer(Control(1).parse("-6 dB").unwrap() as i32)
    );
    e.exact(Pos2::new(753.0, 268.0), "-inf");
    assert_eq!(e.parameter("volume"), ParameterValue::Integer(0));
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(380.0, 250.0));
    let silent = crate::app::routing::scene(rect, matrix([128, 0]), 1.0, false);
    assert!(silent.marks.iter().all(|m| m.emission.is_none()));
    let active = crate::app::routing::scene(rect, matrix([0, 256]), 1.0, false);
    assert!(active.marks.iter().any(|m| m.emission.is_some()));
    assert!(active.marks.len() < 256);
    let center = matrix([0, 256]);
    assert_eq!([center[0][0], center[1][1]], [1.0, 1.0]);
    assert!(center[0][1].abs() < 1e-6 && center[1][0].abs() < 1e-6);
    assert_eq!(e.h.app.active_document().unwrap().revision_id, source);
}

#[test]
fn balance_preview_fine_keyboard_reset_cancel_undo_bypass_and_scaled_input() {
    let mut e = Editor::new(EffectKind::Balance);
    e.frame(vec![]);
    let history = e.history();
    let p = Pos2::new(166.0, 268.0);
    let delta = Vec2::new(0.0, -24.0);
    e.start_drag(p, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(p + delta * 2.0)]);
    e.pointer(p + delta * 2.0, false);
    assert_eq!(e.parameter("balance"), ParameterValue::Integer(0));
    assert_eq!(e.history(), history);
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    let ordinary = e.parameter("balance");
    assert_eq!(e.history(), history + 1);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    e.modifiers = egui::Modifiers::SHIFT;
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    e.modifiers = egui::Modifiers::NONE;
    assert_ne!(e.parameter("balance"), ordinary);
    e.click(p);
    e.key(egui::Key::ArrowRight, egui::Modifiers::NONE);
    for _ in 0..2 {
        e.pointer(p, true);
        e.pointer(p, false);
    }
    assert_eq!(e.parameter("balance"), ParameterValue::Integer(0));
    e.size = Vec2::new(680.0, 424.0);
    e.ctx.set_pixels_per_point(1.5);
    e.frame(vec![]);
    e.exact(Pos2::new(80.0, 214.4), "50% L");
    assert_eq!(e.parameter("balance"), ParameterValue::Integer(-64));
    let history = e.history();
    e.click(Pos2::new(742.0 * 0.8, 29.0 * 0.8));
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .bypassed
    );
    assert_eq!(e.history(), history + 1);
}

#[test]
fn enhancer_all_six_controls_directions_units_and_selectors_preserve_source() {
    use crate::app::stereo_enhancer::Control;
    let mut e = Editor::new(EffectKind::StereoEnhancer);
    let history = e.history();
    let source = e.h.app.active_document().unwrap().revision_id;
    e.frame(vec![]);
    assert_eq!(e.history(), history);
    e.exact(Pos2::new(100.0, 188.0), "25% separated");
    assert_eq!(e.parameter("separation"), ParameterValue::Integer(-32));
    e.exact(Pos2::new(100.0, 188.0), "50%");
    assert_eq!(e.parameter("separation"), ParameterValue::Integer(-64));
    e.exact(Pos2::new(100.0, 188.0), "75% merged");
    assert_eq!(e.parameter("separation"), ParameterValue::Integer(96));
    e.exact(Pos2::new(753.0, 188.0), "0.5 ms Left");
    let left = Control {
        index: 3,
        direction: 0,
    }
    .parse("0.5 ms Left")
    .unwrap()
    .round() as i32;
    assert_eq!(e.parameter("phase"), ParameterValue::Integer(left));
    assert!(left < 0);
    e.exact(Pos2::new(753.0, 188.0), "1 ms");
    assert!(matches!(e.parameter("phase"),ParameterValue::Integer(v) if v<0));
    e.exact(Pos2::new(753.0, 188.0), "500 ms Right");
    assert_eq!(e.parameter("phase"), ParameterValue::Integer(512));
    e.exact(Pos2::new(100.0, 389.0), "100% L");
    assert_eq!(e.parameter("pan"), ParameterValue::Integer(-128));
    e.exact(Pos2::new(753.0, 389.0), "mute");
    assert_eq!(e.parameter("volume"), ParameterValue::Integer(0));
    for (p, id, raw) in [
        (Pos2::new(276.0, 495.0), "placement", 0),
        (Pos2::new(348.0, 495.0), "placement", 1),
        (Pos2::new(531.0, 495.0), "invert", 1),
        (Pos2::new(592.0, 495.0), "invert", 2),
        (Pos2::new(469.0, 495.0), "invert", 0),
    ] {
        let count = e.history();
        e.click(p);
        assert_eq!(e.parameter(id), ParameterValue::Integer(raw));
        assert_eq!(e.history(), count + 1);
    }
    for (index, lo, hi) in [(0, -128, 128), (1, 0, 320), (2, -96, 96), (3, -512, 512)] {
        let c = Control {
            index,
            direction: 0,
        };
        assert_eq!(c.parse(&format!("raw {lo}")), Some(lo as f64));
        assert_eq!(c.parse(&format!("raw {hi}")), Some(hi as f64));
    }
    assert_eq!(e.h.app.active_document().unwrap().revision_id, source);
}

#[test]
fn enhancer_preview_fine_keyboard_reset_cancel_and_scaled_second_line() {
    let mut e = Editor::new(EffectKind::StereoEnhancer);
    e.frame(vec![]);
    let history = e.history();
    let p = Pos2::new(162.0, 188.0);
    let delta = Vec2::new(0.0, -24.0);
    e.start_drag(p, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(p + delta * 2.0)]);
    e.pointer(p + delta * 2.0, false);
    assert_eq!(e.parameter("separation"), ParameterValue::Integer(0));
    assert_eq!(e.history(), history);
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    let ordinary = e.parameter("separation");
    assert_eq!(e.history(), history + 1);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    e.modifiers = egui::Modifiers::SHIFT;
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    e.modifiers = egui::Modifiers::NONE;
    assert_ne!(e.parameter("separation"), ordinary);
    e.click(p);
    e.key(egui::Key::ArrowUp, egui::Modifiers::NONE);
    for _ in 0..2 {
        e.pointer(p, true);
        e.pointer(p, false);
    }
    assert_eq!(e.parameter("separation"), ParameterValue::Integer(0));
    e.size = Vec2::new(680.0, 424.0);
    e.ctx.set_pixels_per_point(1.5);
    e.frame(vec![]);
    e.exact(Pos2::new(80.0, 188.0 * 0.8), "75% separated");
    assert_eq!(e.parameter("separation"), ParameterValue::Integer(-96));
    let out = e.frame(vec![]);
    let texts = text_shapes(&out);
    let value = texts.iter().find(|(_, s)| s == "75.0%").unwrap().0;
    let sub = texts.iter().find(|(_, s)| s == "separated").unwrap().0;
    assert!(value.bottom() < sub.top());
}

#[test]
fn enhancer_field_is_bounded_static_settings_and_clears_emission_on_mute() {
    use crate::app::stereo_enhancer::field;
    use wavebob::dsp::stereo_enhancer as dsp;
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(422.0, 365.0));
    let mut v = dsp::DEFAULTS;
    let silent = field(
        rect,
        {
            let mut v = v;
            v[1] = 0;
            v
        },
        1.0,
        false,
    );
    assert!(silent.marks.iter().all(|m| m.emission.is_none()));
    let normal = field(rect, v, 1.0, false);
    assert!(normal.marks.len() < 1024);
    v[0] = 128;
    let right = field(rect, v, 1.0, false);
    let paths: Vec<_> = right
        .marks
        .iter()
        .filter_map(|m| {
            if m.emission.is_some() {
                match &m.shape {
                    egui::Shape::Path(p) => Some(p),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();
    assert_eq!(paths.len(), 1);
    assert!(
        paths[0]
            .points
            .iter()
            .all(|p| p.x >= rect.center().x - 0.001)
    );
    v = dsp::DEFAULTS;
    v[2] = -96;
    v[3] = 512;
    v[4] = 0;
    let pre = field(rect, v, 1.0, false);
    v[4] = 1;
    let post = field(rect, v, 1.0, false);
    assert_ne!(
        format!("{:?}", pre.marks[pre.marks.len() - 1].shape),
        format!("{:?}", post.marks[post.marks.len() - 1].shape)
    );
    v[5] = 1;
    let inverted = field(rect, v, 1.0, false);
    assert!(inverted.marks.len() > post.marks.len());
    assert!(inverted.marks.len() < 1024);
}

#[test]
fn enhancer_tessellated_field_stays_within_its_surface() {
    let ctx = egui::Context::default();
    theme(&ctx);
    let mut initialized = ctx.run_ui(Default::default(), |_| {});
    initialized.textures_delta.clear();
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(422.0, 365.0));
    for values in [
        wavebob::dsp::stereo_enhancer::DEFAULTS,
        [0, 320, -96, 512, 1, 0],
        [0, 320, -96, -512, 0, 1],
    ] {
        let scene = crate::app::stereo_enhancer::field(rect, values, 1.0, false);
        let clipped = scene
            .marks
            .into_iter()
            .map(|m| egui::epaint::ClippedShape {
                clip_rect: rect,
                shape: m.shape,
                transform_after_tessellation: Default::default(),
            })
            .collect();
        for primitive in ctx.tessellate(clipped, 1.0) {
            if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                for vertex in mesh.vertices {
                    assert!(
                        vertex.pos.is_finite() && rect.expand(4.0).contains(vertex.pos),
                        "{:?}",
                        vertex.pos
                    );
                }
            }
        }
    }
}

#[test]
fn shaper_signed_matrix_exact_neutral_and_channel_units_preserve_source() {
    use crate::app::stereo_shaper::Control;
    let mut e = Editor::new(EffectKind::StereoShaper);
    e.frame(vec![]);
    let source = e.h.app.active_document().unwrap().revision_id;
    for (id, p) in [
        ("left", Pos2::new(100.0, 188.0)),
        ("rl", Pos2::new(100.0, 389.0)),
        ("right", Pos2::new(753.0, 188.0)),
        ("lr", Pos2::new(753.0, 389.0)),
    ] {
        for (text, raw) in [
            ("-200%", -25600),
            ("0%", 0),
            ("+100%", 12800),
            ("200%", 25600),
            ("raw -1", -1),
        ] {
            let history = e.history();
            e.exact(p, text);
            assert_eq!(e.parameter(id), ParameterValue::Integer(raw));
            assert_eq!(e.history(), history + 1);
        }
    }
    let c = Control {
        index: 0,
        direction: 0,
    };
    assert_eq!(c.neutral(), Some(0.0));
    assert_eq!(c.fraction(0.0), 0.5);
    assert_eq!(c.text(-1.0), "-0.008%");
    for (p, id, text, raw) in [
        (Pos2::new(337.0, 389.0), "delay", "500 ms Left", -4096),
        (Pos2::new(337.0, 389.0), "delay", "500 ms Right", 4096),
        (Pos2::new(514.0, 389.0), "phase", "10 Hz Left", -4096),
        (Pos2::new(514.0, 389.0), "phase", "10 Hz Right", 4096),
        (Pos2::new(514.0, 389.0), "phase", "Off", 0),
    ] {
        e.exact(p, text);
        assert_eq!(e.parameter(id), ParameterValue::Integer(raw));
    }
    e.exact(Pos2::new(514.0, 389.0), "1 kHz Left");
    let expected = Control {
        index: 5,
        direction: 0,
    }
    .parse("1 kHz Left")
    .unwrap() as i32;
    assert!(expected < 0);
    assert!((wavebob::dsp::stereo_shaper::phase_hz(expected) - 1000.0).abs() < 2.0);
    assert_eq!(e.parameter("phase"), ParameterValue::Integer(expected));
    e.exact(Pos2::new(514.0, 389.0), "2 kHz");
    assert!(matches!(e.parameter("phase"),ParameterValue::Integer(v) if v<0));
    for (x, raw) in [(353.0, -1), (425.0, 0), (497.0, 1)] {
        e.click(Pos2::new(x, 496.0));
        assert_eq!(e.parameter("position"), ParameterValue::Integer(raw));
    }
    assert_eq!(e.h.app.active_document().unwrap().revision_id, source);
}

#[test]
fn shaper_dial_preview_cancel_fine_reset_undo_and_scaled_input() {
    let mut e = Editor::new(EffectKind::StereoShaper);
    e.frame(vec![]);
    let p = Pos2::new(162.0, 188.0);
    let delta = Vec2::new(0.0, -24.0);
    let history = e.history();
    e.start_drag(p, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(p + delta * 2.0)]);
    e.pointer(p + delta * 2.0, false);
    assert_eq!(e.parameter("left"), ParameterValue::Integer(12800));
    assert_eq!(e.history(), history);
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    let ordinary = e.parameter("left");
    assert_eq!(e.history(), history + 1);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    e.modifiers = egui::Modifiers::SHIFT;
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    e.modifiers = egui::Modifiers::NONE;
    assert_ne!(e.parameter("left"), ordinary);
    e.click(p);
    e.key(egui::Key::ArrowLeft, egui::Modifiers::NONE);
    for _ in 0..2 {
        e.pointer(p, true);
        e.pointer(p, false);
    }
    assert_eq!(e.parameter("left"), ParameterValue::Integer(12800));
    e.size = Vec2::new(680.0, 424.0);
    e.ctx.set_pixels_per_point(1.5);
    e.frame(vec![]);
    e.exact(Pos2::new(80.0, 188.0 * 0.8), "-100%");
    assert_eq!(e.parameter("left"), ParameterValue::Integer(-12800));
    e.click(Pos2::new(742.0 * 0.8, 29.0 * 0.8));
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .bypassed
    );
}

#[test]
fn shaper_routing_tracks_nonlinear_signed_matrix_without_extra_buses() {
    use crate::app::stereo_shaper::matrix;
    use wavebob::dsp::stereo_shaper as dsp;
    let mut v = dsp::DEFAULTS;
    let m = matrix(v);
    assert!((m[0][0] - 1.0).abs() < 1e-6);
    assert_eq!(m[0][1], 0.0);
    assert_eq!(m[1][0], 0.0);
    v[..4].copy_from_slice(&[12800, 0, 0, 12800]);
    let swap = matrix(v);
    assert_eq!(swap[0][0], 0.0);
    assert!((swap[0][1] - 1.0).abs() < 1e-6);
    assert!((swap[1][0] - 1.0).abs() < 1e-6);
    v[..4].copy_from_slice(&[-25600, -12800, 25600, 3200]);
    let m = matrix(v);
    assert!(m[0][0] < 0.0 && m[0][1] < -3.99 && m[1][0] > 0.0 && m[1][1] > 3.99);
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(380.0, 218.0));
    let scene = routing::scene(rect, m, 1.0, false);
    assert!(scene.marks.len() < 512);
    assert!(scene.marks.iter().any(|m| m.emission.is_some()));
    let silent = routing::scene(rect, [[0.0; 2]; 2], 1.0, false);
    assert!(silent.marks.iter().all(|m| m.emission.is_none()));
}

#[test]
fn transient_all_nine_controls_use_reference_units_and_independent_switches() {
    use crate::app::transient_processor::Control;
    let mut e = Editor::new(EffectKind::TransientProcessor);
    let before = e.history();
    let source = e.h.app.active_document().unwrap().revision_id;
    e.frame(vec![]);
    assert_eq!(e.history(), before);
    for (id, p, text, raw) in [
        ("attack", Pos2::new(100.0, 188.0), "-60%", 0),
        ("attack", Pos2::new(100.0, 188.0), "40%", 160),
        ("release", Pos2::new(100.0, 389.0), "-100%", 0),
        ("release", Pos2::new(100.0, 389.0), "60%", 160),
        ("drive", Pos2::new(753.0, 188.0), "0 dB", 16),
        ("gain", Pos2::new(753.0, 389.0), "-9 dB", 0),
        ("gain", Pos2::new(753.0, 389.0), "9 dB", 160),
        ("split", Pos2::new(297.0, 455.0), "5 Hz", 0),
        ("split", Pos2::new(297.0, 455.0), "1 kHz", 160),
        ("balance", Pos2::new(444.0, 455.0), "100% low", 0),
        ("balance", Pos2::new(444.0, 455.0), "100% high", 160),
        ("balance", Pos2::new(444.0, 455.0), "Centred", 80),
    ] {
        let n = e.history();
        e.exact(p, text);
        assert_eq!(e.parameter(id), ParameterValue::Integer(raw));
        assert_eq!(e.history(), n + 1);
    }
    assert_eq!(Control(0).neutral(), Some(80.0));
    assert_eq!(Control(0).text(80.0), "0%");
    assert!(!Control(2).text(0.0).contains("ms"));
    for (x, raw) in [(245.0, 0), (314.0, 1), (383.0, 2)] {
        e.click(Pos2::new(x, 351.0));
        assert_eq!(e.parameter("attack_shape"), ParameterValue::Integer(raw));
        assert_eq!(e.parameter("release_shape"), ParameterValue::Integer(1));
    }
    for (x, raw) in [(473.0, 0), (611.0, 2), (542.0, 1)] {
        e.click(Pos2::new(x, 351.0));
        assert_eq!(e.parameter("release_shape"), ParameterValue::Integer(raw));
        assert_eq!(e.parameter("attack_shape"), ParameterValue::Integer(2));
    }
    e.click(Pos2::new(572.0, 469.0));
    assert_eq!(e.parameter("on"), ParameterValue::Integer(0));
    e.click(Pos2::new(742.0, 29.0));
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .committed
            .effects[0]
            .bypassed
    );
    assert_eq!(e.parameter("on"), ParameterValue::Integer(0));
    e.click(Pos2::new(617.0, 469.0));
    assert_eq!(e.parameter("on"), ParameterValue::Integer(1));
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
    assert_eq!(e.parameter("on"), ParameterValue::Integer(0));
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
    assert_eq!(e.h.app.active_document().unwrap().revision_id, source);
}

#[test]
fn transient_shared_gestures_cancel_commit_and_scaled_layout() {
    let mut e = Editor::new(EffectKind::TransientProcessor);
    e.frame(vec![]);
    let p = Pos2::new(162.0, 188.0);
    let delta = Vec2::new(0.0, -24.0);
    let history = e.history();
    e.start_drag(p, delta);
    assert!(
        e.h.app
            .host
            .processing(e.owner.clone())
            .unwrap()
            .preview
            .is_some()
    );
    e.key(egui::Key::Escape, egui::Modifiers::NONE);
    e.frame(vec![egui::Event::PointerMoved(p + delta * 2.0)]);
    e.pointer(p + delta * 2.0, false);
    assert_eq!(e.parameter("attack"), ParameterValue::Integer(80));
    assert_eq!(e.history(), history);
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    let ordinary = e.parameter("attack");
    assert_eq!(e.history(), history + 1);
    e.h.app.dispatch(Command::Undo(EmptyParams {}));
    e.modifiers = egui::Modifiers::SHIFT;
    e.start_drag(p, delta);
    e.pointer(p + delta, false);
    e.modifiers = egui::Modifiers::NONE;
    assert_ne!(e.parameter("attack"), ordinary);
    e.click(p);
    e.key(egui::Key::ArrowRight, egui::Modifiers::NONE);
    for _ in 0..2 {
        e.pointer(p, true);
        e.pointer(p, false);
    }
    assert_eq!(e.parameter("attack"), ParameterValue::Integer(80));
    e.size = Vec2::new(680.0, 424.0);
    e.ctx.set_pixels_per_point(1.5);
    e.frame(vec![]);
    e.exact(Pos2::new(297.0 * 0.8, 455.0 * 0.8), "raw 160");
    assert_eq!(e.parameter("split"), ParameterValue::Integer(160));
    let output = e.frame(vec![]);
    let text = text_shapes(&output);
    for label in [
        "Transient Processor",
        "Attack shape",
        "Release shape",
        "Split frequency",
        "Split balance",
        "Processing",
    ] {
        assert!(text.iter().any(|(_, s)| s == label), "{label}");
    }
}

#[test]
fn transient_envelope_is_deterministic_parameter_driven_and_inactive_without_emission() {
    use crate::app::transient_processor::{envelopes, scene};
    use wavebob::dsp::transient_processor as dsp;
    let mut v = dsp::DEFAULTS;
    let initial = envelopes(v);
    assert_eq!(initial, envelopes(v));
    assert_eq!(initial.len(), 401);
    v[0] = 160;
    v[2] = 160;
    let boosted = envelopes(v);
    assert!(boosted[40][1] > initial[40][1]);
    assert!(boosted[220][1] > initial[220][1]);
    v[0] = 0;
    v[2] = 0;
    let cut = envelopes(v);
    assert!(cut[40][1] < initial[40][1]);
    assert!(cut[220][1] < initial[220][1]);
    v[0] = 160;
    v[2] = 160;
    v[4] = 0;
    v[5] = 0;
    let sharp = envelopes(v);
    v[4] = 2;
    v[5] = 2;
    assert_ne!(sharp, envelopes(v));
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(414.0, 193.0));
    let active = scene(rect, v, 1.0, false);
    assert!(active.marks.len() < 1024);
    assert!(active.marks.iter().any(|m| m.emission.is_some()));
    v[8] = 0;
    let off = envelopes(v);
    assert!(off.iter().all(|p| p[0] == p[1]));
    assert!(
        scene(rect, v, 1.0, false)
            .marks
            .iter()
            .all(|m| m.emission.is_none())
    );
    for d in [initial, boosted, cut, sharp, off] {
        assert!(d.iter().flatten().all(|v| v.is_finite() && *v >= 0.0));
    }
}
