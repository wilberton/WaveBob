use super::*;

struct Harness {
    ctx: egui::Context,
    app: WaveBob,
    time: f64,
    size: Vec2,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        theme(&ctx);
        let mut harness = Self {
            ctx,
            app: WaveBob::default(),
            time: 0.0,
            size: Vec2::new(1440.0, 900.0),
        };
        harness.frame(Vec::new());
        harness.frame(Vec::new());
        harness
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.1;
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| self.app.draw(ui),
        );
        output.textures_delta.clear();
        output
    }
    fn click(&mut self, pos: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        self.frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(Vec::new());
    }
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(from)]);
        self.frame(vec![egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(vec![egui::Event::PointerMoved(to)]);
        self.frame(vec![egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    fn key(&mut self, modifiers: egui::Modifiers, repeat: bool) {
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Backtick,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }]);
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Backtick,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }]);
    }
    fn text_rect(&mut self, wanted: &str) -> Rect {
        let output = self.frame(Vec::new());
        text_shapes(&output)
            .into_iter()
            .find(|(_, text)| text == wanted)
            .unwrap_or_else(|| panic!("Missing visible text {wanted}"))
            .0
    }
}

fn text_shapes(output: &egui::FullOutput) -> Vec<(Rect, String)> {
    fn collect(shape: &egui::Shape, out: &mut Vec<(Rect, String)>) {
        match shape {
            egui::Shape::Text(text) => out.push((
                Rect::from_min_size(text.pos, text.galley.size()),
                text.galley.text().to_owned(),
            )),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut result);
    }
    result
}

#[test]
fn chevrons_restore_independently_and_backtick_preserves_widths_in_both_modes() {
    for mode in [ViewMode::Waveform, ViewMode::Multitrack] {
        let mut h = Harness::new();
        h.app.set_mode(mode);
        h.app.layout(SetLayoutParams {
            left_width: Some(300.0),
            right_width: Some(270.0),
            ..Default::default()
        });
        let left = h.text_rect("‹").center();
        h.click(left);
        assert!(!h.app.host.presentation().layout.left.visible);
        assert!(h.app.host.presentation().layout.right.visible);
        h.click(Pos2::new(12.0, 66.0));
        assert!(h.app.host.presentation().layout.left.visible);
        // Use the right chevron's visible text rather than widget internals.
        let right = h.text_rect("›").center();
        h.click(right);
        assert!(!h.app.host.presentation().layout.right.visible);
        h.key(egui::Modifiers::NONE, false);
        assert!(!h.app.host.presentation().layout.left.visible);
        assert!(!h.app.host.presentation().layout.right.visible);
        h.key(egui::Modifiers::NONE, false);
        let layout = h.app.host.presentation().layout;
        assert!(layout.left.visible && layout.right.visible);
        assert_eq!((layout.left.width, layout.right.width), (300.0, 270.0));
        assert_eq!(h.app.host.snapshot().revision, 0);
    }
}

#[test]
fn pointer_drags_resize_both_panels_and_the_library_browser_split() {
    let mut h = Harness::new();
    h.drag(Pos2::new(237.0, 400.0), Pos2::new(315.0, 400.0));
    assert!(h.app.host.presentation().layout.left.width > 300.0);
    h.drag(Pos2::new(1243.0, 400.0), Pos2::new(1140.0, 400.0));
    assert!(h.app.host.presentation().layout.right.width > 290.0);
    let top = 47.0;
    let height = 810.0; // Visible column between header and status, inset by 10.
    h.drag(
        Pos2::new(100.0, top + height * 0.18),
        Pos2::new(100.0, top + height * 0.44),
    );
    assert!(h.app.host.presentation().layout.library_fraction > 0.40);
    assert_eq!(h.app.host.snapshot().revision, 0);
}

#[test]
fn hover_and_keyboard_focus_help_clear_without_tooltip_popups() {
    let mut h = Harness::new();
    let play = Pos2::new(634.0, 844.0);
    h.frame(vec![egui::Event::PointerMoved(play)]);
    h.time += 3.0;
    let output = h.frame(Vec::new());
    let texts = text_shapes(&output);
    let help = texts
        .iter()
        .find(|(_, text)| text.starts_with("Playback is unavailable"))
        .expect("Disabled control must explain why");
    assert!(
        help.0.top() > 860.0,
        "Help belongs to the bottom status bar"
    );
    assert!(egui::Tooltip::seconds_since_last_tooltip(&h.ctx).is_infinite());
    let output = h.frame(vec![egui::Event::PointerGone]);
    assert!(
        !text_shapes(&output)
            .iter()
            .any(|(_, text)| text.starts_with("Playback is unavailable"))
    );
    // Tab gives keyboard focus to a visible interactive control.
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let output = h.frame(Vec::new());
    assert!(
        text_shapes(&output)
            .iter()
            .any(|(rect, text)| rect.top() > 860.0
                && (text.contains("waveform editing")
                    || text.contains("Open a WAV")
                    || text.contains("arrangement view")))
    );
    // Backtick still works with a button focused, and removes its stale help.
    h.key(egui::Modifiers::NONE, false);
    assert!(!h.app.host.presentation().layout.left.visible);
}

#[test]
fn text_input_modals_modified_and_repeat_keys_suppress_global_shortcuts() {
    let mut h = Harness::new();
    h.key(egui::Modifiers::SHIFT, false);
    assert!(h.app.host.presentation().layout.left.visible);
    h.key(egui::Modifiers::CTRL, false);
    assert!(h.app.host.presentation().layout.left.visible);
    let key_event = |pressed, repeat| egui::Event::Key {
        key: egui::Key::Backtick,
        physical_key: None,
        pressed,
        repeat,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame(vec![key_event(true, false)]);
    assert!(!h.app.host.presentation().layout.left.visible);
    h.frame(vec![key_event(true, true)]);
    assert!(!h.app.host.presentation().layout.left.visible);
    h.frame(vec![key_event(false, false)]);
    h.key(egui::Modifiers::NONE, false);
    assert!(h.app.host.presentation().layout.left.visible);
    let mut text = String::new();
    let input_id = egui::Id::unique("test-text");
    let mut output = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).id(input_id))
            .request_focus();
    });
    output.textures_delta.clear();
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Backtick,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(input_id));
            h.app.shortcuts(ui.ctx());
        },
    );
    output.textures_delta.clear();
    assert!(h.app.host.presentation().layout.left.visible);
    h.ctx.memory_mut(|memory| memory.surrender_focus(input_id));
    let mut output = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::Modal::new(egui::Id::unique("test-modal")).show(ui.ctx(), |ui| {
            ui.label("Modal owns keyboard");
        });
    });
    output.textures_delta.clear();
    h.key(egui::Modifiers::NONE, false);
    assert!(h.app.host.presentation().layout.left.visible);
}

#[test]
fn empty_readouts_and_persistent_errors_survive_hover_and_narrow_layouts() {
    let mut h = Harness::new();
    h.app.error = Some("Example persistent failure".into());
    for mode in [ViewMode::Waveform, ViewMode::Multitrack] {
        h.app.set_mode(mode);
        h.app.error = Some("Example persistent failure".into());
        h.size = Vec2::new(800.0, 600.0);
        let output = h.frame(vec![egui::Event::PointerGone]);
        let texts = text_shapes(&output);
        assert!(
            texts
                .iter()
                .any(|(_, text)| text == "Example persistent failure")
        );
        let readout = if mode == ViewMode::Waveform {
            "No document · Cursor — · No selection"
        } else {
            "Empty arrangement · Selection: none · Selected clips: 0"
        };
        let rect = texts
            .iter()
            .find(|(_, text)| text == readout)
            .expect("Actual empty-state readout must remain visible")
            .0;
        assert!(rect.min.x >= 0.0 && rect.max.x <= 800.0 && rect.max.y <= 600.0);
        let widths = h.app.host.presentation().layout.effective_widths(800.0);
        assert!(800.0 - widths.iter().sum::<f32>() >= MIN_CENTER_WIDTH - 0.01);
    }
}

#[test]
fn gui_loop_action_and_headless_command_produce_the_same_state() {
    let mut app = WaveBob::default();
    let mut headless = wavebob::Session::default();
    for _ in 0..3 {
        app.toggle_loop();
        wavebob::protocol::dispatch_line(
            &mut headless,
            r#"{"id":"headless","command":"transport.loop.toggle"}"#,
        );
        assert_eq!(
            app.host.snapshot().loop_enabled,
            headless.snapshot().loop_enabled
        );
        assert_eq!(app.host.snapshot().revision, headless.snapshot().revision);
    }
}

#[test]
fn open_action_real_channels_document_switching_and_failed_jobs_use_shared_state() {
    let directory = tempfile::tempdir().unwrap();
    for (name, channels, value) in [("Stereo.wav", 2, 16384_i16), ("Mono.wav", 1, -8192_i16)] {
        let mut writer = hound::WavWriter::create(
            directory.path().join(name),
            hound::WavSpec {
                channels,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..4800 {
            for channel in 0..channels {
                writer
                    .write_sample(if channel == 0 { value } else { -value })
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
    }
    let mut h = Harness::new();
    let open = h.text_rect("Open audio…").center();
    h.click(open);
    assert!(
        h.app.open_requested,
        "The visible Open action requests a native file dialog"
    );
    h.app.open_requested = false;
    for name in ["Stereo.wav", "Mono.wav"] {
        h.app.open_path(directory.path().join(name));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.app.active_document().is_none_or(|doc| doc.name != name) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
            h.frame(Vec::new());
        }
    }
    assert_eq!(h.app.waveform.as_ref().unwrap().channels.len(), 1);
    assert_eq!(h.app.waveform.as_ref().unwrap().channels[0][0].min, -0.25);
    h.app.set_mode(ViewMode::Multitrack);
    let stereo = h.text_rect("Stereo.wav").center();
    h.click(stereo);
    assert_eq!(h.app.host.presentation().mode, ViewMode::Waveform);
    let wave = h.app.waveform.as_ref().unwrap();
    assert_eq!(wave.channels.len(), 2);
    assert_eq!(wave.channels[0][0].max, 0.5);
    assert_eq!(wave.channels[1][0].min, -0.5);
    assert_eq!(h.app.active_document().unwrap().frames, 4800);
    h.app.open_path(directory.path().join("missing.wav"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.error.is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(Vec::new());
    }
    assert_eq!(h.app.host.snapshot().documents.len(), 2);
    assert_eq!(h.app.active_document().unwrap().name, "Stereo.wav");
    let close = h.text_rect("×").center();
    h.click(close);
    assert_eq!(h.app.host.snapshot().documents.len(), 1);
    assert_eq!(h.app.active_document().unwrap().name, "Mono.wav");
}
