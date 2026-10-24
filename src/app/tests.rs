use super::*;

struct Harness {
    ctx: egui::Context,
    app: WaveBob,
    time: f64,
    size: Vec2,
    step: f64,
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
            step: 0.1,
        };
        harness.frame(Vec::new());
        harness.frame(Vec::new());
        harness
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += self.step;
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
fn transport_shortcuts_drive_the_shared_output_and_mode_switch_pauses() {
    let (mut h, directory) = audio_harness();
    let (session, mut output) = wavebob::Session::with_manual_output(48000).unwrap();
    h.app.host = DesktopHost::with_session(session);
    h.app.open_path(directory.path().join("Gestures.wav"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.active_document().is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    let tap = |h: &mut Harness, key| {
        h.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        h.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    };
    tap(&mut h, egui::Key::Space);
    assert!(h.app.host.transport().requested.playing);
    output.render_into(&mut [[0.0; 2]; 128]);
    h.frame(vec![]);
    assert_eq!(h.app.active_document().unwrap().cursor, 128);
    tap(&mut h, egui::Key::Space);
    output.render_into(&mut []);
    h.frame(vec![]);
    assert!(!h.app.host.transport().applied.playing);
    assert_eq!(h.app.active_document().unwrap().cursor, 128);
    tap(&mut h, egui::Key::Comma);
    output.render_into(&mut []);
    h.frame(vec![]);
    assert_eq!(h.app.active_document().unwrap().cursor, 0);
    tap(&mut h, egui::Key::Space);
    h.app.set_mode(ViewMode::Multitrack);
    output.render_into(&mut []);
    h.frame(vec![]);
    assert!(!h.app.host.transport().applied.playing);
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

fn audio_harness() -> (Harness, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Gestures.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for frame in 0..4800 {
        let sample = ((frame % 100) as i16 - 50) * 400;
        writer.write_sample(sample).unwrap();
        writer.write_sample(-sample).unwrap();
    }
    writer.finalize().unwrap();
    let mut h = Harness::new();
    h.app.open_path(path);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.active_document().is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(Vec::new());
    }
    h.frame(Vec::new());
    (h, directory)
}

#[test]
fn browser_single_click_selects_and_double_click_opens_without_audition_race() {
    let (mut h, directory) = audio_harness();
    h.app.dispatch(Command::CloseDocument(CloseParams {
        document_id: h.app.active_document().unwrap().id,
        discard_changes: false,
    }));
    h.app.browser_path = directory.path().to_string_lossy().into_owned();
    h.app
        .dispatch(Command::BrowseFolder(wavebob::browser::PathParams {
            path: h.app.browser_path.clone(),
        }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.host.browser().loading {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    h.step = 0.02;
    let pos = h.text_rect("Gestures.wav").center();
    h.click(pos);
    assert!(h.app.host.snapshot().documents.is_empty());
    assert!(h.app.host.browser().selected_path.is_some());
    assert!(!h.app.host.browser().audition.loading);
    h.click(pos);
    while h.app.host.snapshot().documents.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    assert_eq!(h.app.active_document().unwrap().name, "Gestures.wav");
    assert!(!h.app.host.browser().audition.playing);
}
#[test]
fn browser_preview_button_is_enabled_for_selected_audio_and_uses_status_help() {
    let (mut h, directory) = audio_harness();
    let (session, mut output) = wavebob::Session::with_manual_output(48000).unwrap();
    h.app.host = DesktopHost::with_session(session);
    h.app
        .dispatch(Command::BrowseFolder(wavebob::browser::PathParams {
            path: directory.path().to_string_lossy().into_owned(),
        }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.host.browser().loading {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    let row = h.text_rect("Gestures.wav").center();
    h.click(row);
    fn find(shapes: &egui::Shape) -> Option<Rect> {
        match shapes {
            egui::Shape::Rect(shape)
                if (shape.rect.width() - 46.0).abs() < 0.1
                    && (shape.rect.height() - 28.0).abs() < 0.1
                    && shape.rect.left() < 80.0 =>
            {
                Some(shape.rect)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(find),
            _ => None,
        }
    }
    let frame = h.frame(vec![]);
    let rect = frame
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape))
        .unwrap();
    h.click(rect.center());
    while h.app.host.browser().audition.loading {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    assert!(h.app.host.browser().audition.playing);
    assert!(h.app.host.snapshot().documents.is_empty());
    let mut pcm = [[0.0; 2]; 8];
    output.render_into(&mut pcm);
    assert!(pcm.iter().flatten().any(|sample| *sample != 0.0));
    let frame = h.frame(vec![egui::Event::PointerMoved(rect.center())]);
    assert!(
        text_shapes(&frame)
            .iter()
            .any(|(_, text)| text.starts_with("Stop preview"))
    );
}

fn canvas_rects(h: &mut Harness) -> [Rect; 2] {
    fn collect(shape: &egui::Shape, out: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Rect(shape)
                if shape.stroke.color == Color32::from_rgb(91, 104, 119)
                    && shape.rect.width() > 300.0
                    && shape.rect.height() > 60.0 =>
            {
                out.push(shape.rect)
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let output = h.frame(Vec::new());
    let mut rects = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut rects);
    }
    rects.sort_by(|a, b| a.top().total_cmp(&b.top()));
    assert_eq!(rects.len(), 2);
    [rects[0], rects[1]]
}
fn pointer_event(pos: Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn waveform_click_reversed_drag_preview_and_escape_have_exact_shared_semantics() {
    let (mut h, _directory) = audio_harness();
    let rect = visuals::main_plot(canvas_rects(&mut h)[1]);
    let point = |fraction: f32| Pos2::new(rect.left() + rect.width() * fraction, rect.center().y);
    let revision = h.app.active_document().unwrap().revision_id;
    let from = point(0.75);
    let to = point(0.25);
    h.frame(vec![egui::Event::PointerMoved(from)]);
    h.frame(vec![pointer_event(
        from,
        egui::PointerButton::Primary,
        true,
    )]);
    h.frame(vec![egui::Event::PointerMoved(to)]);
    assert!(
        h.app.active_document().unwrap().selection.is_none(),
        "Drag previews must not commit early"
    );
    assert_eq!(
        h.app.display_document().unwrap().selection,
        Some(FrameRange {
            start: 1200,
            end: 3600
        })
    );
    h.frame(vec![pointer_event(to, egui::PointerButton::Primary, false)]);
    h.frame(Vec::new());
    let document = h.app.active_document().unwrap();
    assert_eq!(
        document.selection,
        Some(FrameRange {
            start: 1200,
            end: 3600
        })
    );
    assert_eq!(document.cursor, 1200);
    assert_eq!(document.revision_id, revision);
    assert!(!document.dirty);
    let output = h.frame(Vec::new());
    assert!(
        text_shapes(&output)
            .iter()
            .any(|(rect, text)| rect.top() > 840.0
                && text.contains("Selection 1200–3600")
                && text.contains("2400 frames / 00:00.050000"))
    );
    h.click(point(0.5));
    assert_eq!(h.app.active_document().unwrap().cursor, 2400);
    assert!(h.app.active_document().unwrap().selection.is_none());
    let before = h.app.host.snapshot();
    h.frame(vec![egui::Event::PointerMoved(from)]);
    h.frame(vec![pointer_event(
        from,
        egui::PointerButton::Primary,
        true,
    )]);
    h.frame(vec![egui::Event::PointerMoved(to)]);
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.frame(vec![pointer_event(to, egui::PointerButton::Primary, false)]);
    assert_eq!(
        h.app.host.snapshot(),
        before,
        "Escape cancels the uncommitted selection"
    );
}

#[test]
fn overview_body_and_both_edges_pan_zoom_and_keep_selection_independent() {
    let (mut h, _directory) = audio_harness();
    let doc = h.app.active_document().unwrap();
    h.app.dispatch(Command::SetSelection(SelectionParams {
        document_id: doc.id.clone(),
        start_frame: 1000,
        end_frame: 2000,
    }));
    h.app.dispatch(Command::SetViewport(SetViewportParams {
        document_id: doc.id.clone(),
        start_frame: 1200,
        end_frame: 3600,
        expected_presentation_revision: None,
    }));
    let core = h.app.host.snapshot();
    let overview = canvas_rects(&mut h)[0].shrink(1.0);
    let point = |frame: u64| {
        Pos2::new(
            overview.left() + overview.width() * frame as f32 / 4800.0,
            overview.center().y,
        )
    };
    h.drag(point(2400), point(2880));
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 1680,
            end: 4080
        }
    );
    h.drag(point(1680), point(720));
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 720,
            end: 4080
        }
    );
    h.drag(point(4080), point(4320));
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 720,
            end: 4320
        }
    );
    h.drag(point(720), point(4800));
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 4319,
            end: 4320
        }
    );
    h.frame(Vec::new());
    assert_eq!(
        h.app.detail.as_ref().unwrap().end_frame - h.app.detail.as_ref().unwrap().start_frame,
        1
    );
    assert_eq!(
        h.app.host.snapshot(),
        core,
        "Navigation must preserve selection and all core state"
    );
}

#[test]
fn wheel_pinch_middle_drag_and_toolbar_zoom_anchor_and_clamp_the_view() {
    let (mut h, _directory) = audio_harness();
    let doc = h.app.active_document().unwrap();
    let set = |h: &mut Harness| {
        h.app.dispatch(Command::SetViewport(SetViewportParams {
            document_id: doc.id.clone(),
            start_frame: 1200,
            end_frame: 3600,
            expected_presentation_revision: None,
        }))
    };
    set(&mut h);
    let [overview, main] = canvas_rects(&mut h);
    let plot = visuals::main_plot(main);
    let anchor = Pos2::new(plot.left() + plot.width() * 0.25, plot.center().y);
    h.frame(vec![
        egui::Event::PointerMoved(anchor),
        egui::Event::Zoom(2.0),
    ]);
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 1500,
            end: 2700
        }
    );
    set(&mut h);
    h.frame(Vec::new());
    let from = plot.center();
    let to = from + Vec2::new(plot.width() * 0.1, 0.0);
    h.frame(vec![egui::Event::PointerMoved(from)]);
    h.frame(vec![pointer_event(from, egui::PointerButton::Middle, true)]);
    h.frame(vec![egui::Event::PointerMoved(to)]);
    h.frame(vec![pointer_event(to, egui::PointerButton::Middle, false)]);
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 960,
            end: 3360
        }
    );
    set(&mut h);
    h.frame(Vec::new());
    let wheel = |modifiers, delta| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers,
    };
    h.frame(vec![
        egui::Event::PointerMoved(anchor),
        wheel(egui::Modifiers::CTRL, Vec2::new(0.0, 60.0)),
    ]);
    let zoomed = h.app.host.presentation().visible_interval(&doc);
    assert!(zoomed.end - zoomed.start < 2400);
    assert!(
        (zoomed.start as f64 + 0.25 * (zoomed.end - zoomed.start) as f64 - 1800.0).abs() <= 1.0
    );
    // Finish wheel smoothing before testing plain scrolling as a separate gesture.
    for _ in 0..15 {
        h.frame(Vec::new());
    }
    set(&mut h);
    h.frame(Vec::new());
    h.frame(vec![
        egui::Event::PointerMoved(anchor),
        wheel(egui::Modifiers::NONE, Vec2::new(0.0, -80.0)),
    ]);
    let panned = h.app.host.presentation().visible_interval(&doc);
    assert_eq!(panned.end - panned.start, 2400);
    assert!(panned.start > 1200);
    for _ in 0..15 {
        h.frame(Vec::new());
    }
    h.click(Pos2::new(main.right() - 22.0, overview.top() - 22.0));
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 0,
            end: 4800
        }
    );
    h.click(Pos2::new(main.right() - 60.0, overview.top() - 22.0));
    assert_eq!(
        h.app.host.presentation().visible_interval(&doc),
        FrameRange {
            start: 1200,
            end: 3600
        }
    );
    assert!(h.app.active_document().unwrap().selection.is_none());
    assert!(!h.app.active_document().unwrap().dirty);
}

#[test]
fn selection_keyboard_shortcuts_respect_text_focus_and_narrow_status_help() {
    let (mut h, _directory) = audio_harness();
    let event = egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    };
    h.frame(vec![event.clone()]);
    assert_eq!(
        h.app.active_document().unwrap().selection,
        Some(FrameRange {
            start: 0,
            end: 4800
        })
    );
    h.frame(vec![egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.size = Vec2::new(800.0, 600.0);
    let main = visuals::main_plot(canvas_rects(&mut h)[1]);
    let output = h.frame(vec![egui::Event::PointerMoved(main.center())]);
    let texts = text_shapes(&output);
    let readout = texts
        .iter()
        .find(|(_, text)| text.contains("Selection 0–4800"))
        .unwrap();
    assert!(readout.0.right() <= 800.0 && readout.0.bottom() <= 600.0);
    let help = texts
        .iter()
        .find(|(_, text)| text.contains("Click to place the cursor"))
        .unwrap();
    assert!(help.0.top() > 550.0);
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(h.app.active_document().unwrap().selection.is_none());
    let before = h.app.host.snapshot();
    let mut text = String::from("Typed text");
    let id = egui::Id::unique("selection-text-focus");
    let mut output = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).id(id))
            .request_focus();
    });
    output.textures_delta.clear();
    let mut output = h.ctx.run_ui(
        egui::RawInput {
            events: vec![event],
            ..Default::default()
        },
        |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(id));
            h.app.shortcuts(ui.ctx());
        },
    );
    output.textures_delta.clear();
    assert_eq!(h.app.host.snapshot(), before);
}
