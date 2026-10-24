use super::*;

struct Harness {
    ctx: egui::Context,
    app: WaveBob,
    time: f64,
    size: Vec2,
    step: f64,
}

#[test]
fn waveform_envelope_keeps_a_visible_continuous_trace_for_flat_bins() {
    assert_flat_waveform_visible(2);
}
#[test]
fn waveform_individual_samples_keep_a_visible_continuous_trace() {
    assert_flat_waveform_visible(1);
}
fn assert_flat_waveform_visible(frames_per_bin: u64) {
    // At a few frames per bin, neighbouring samples can have identical values
    // (DC, held values, or locally flat audio). Their trace must remain visible.
    let ctx = egui::Context::default();
    let rect = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(128.0, 100.0));
    let values: Vec<_> = (0..64)
        .map(|i| Peak {
            start_frame: i * frames_per_bin,
            end_frame: (i + 1) * frames_per_bin,
            min: 0.5,
            max: 0.5,
        })
        .collect();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 160.0))),
            ..Default::default()
        },
        |ui| {
            visuals::peaks(ui.painter(), rect, &values, visuals::WAVE);
        },
    );
    output.textures_delta.clear();
    let primitives = ctx.tessellate(output.shapes, 1.0);
    let y = rect.center().y - 0.5 * rect.height() * 0.45;
    for x in 22..146 {
        let point = Pos2::new(x as f32 + 0.5, y);
        let covered = primitives
            .iter()
            .any(|primitive| match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    mesh.indices.as_chunks::<3>().0.iter().any(|triangle| {
                        let [a, b, c] = [
                            mesh.vertices[triangle[0] as usize],
                            mesh.vertices[triangle[1] as usize],
                            mesh.vertices[triangle[2] as usize],
                        ];
                        if ![a, b, c].iter().any(|vertex| vertex.color == visuals::WAVE) {
                            return false;
                        }
                        let cross = |a: Pos2, b: Pos2, c: Pos2| {
                            (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
                        };
                        let area = cross(a.pos, b.pos, c.pos);
                        if area.abs() < 0.0001 {
                            return false;
                        }
                        let edges = [
                            cross(a.pos, b.pos, point),
                            cross(b.pos, c.pos, point),
                            cross(c.pos, a.pos, point),
                        ];
                        edges.iter().all(|v| *v >= -0.0001) || edges.iter().all(|v| *v <= 0.0001)
                    })
                }
                _ => false,
            });
        assert!(
            covered,
            "Waveform trace disappears at column {x} for {frames_per_bin}-frame flat bins"
        );
    }
}

#[test]
fn arrangement_library_drop_moves_once_and_invalid_or_escape_drops_cancel() {
    use wavebob::arrangement::*;
    let (mut h, _directory) = audio_harness();
    h.app.dispatch(Command::AddTrack(AddTrackParams::default()));
    h.app.dispatch(Command::AddTrack(AddTrackParams::default()));
    h.app.set_mode(ViewMode::Multitrack);
    h.app
        .dispatch(Command::SetTimelineViewport(TimelineViewportParams {
            start_frame: 0,
            end_frame: 48_000,
            expected_presentation_revision: None,
        }));
    h.frame(vec![]);
    let source = h.text_rect("Gestures.wav").center();
    let track1 = h.text_rect("Track 1");
    let target = Pos2::new(600.0, track1.center().y + 35.0);
    let history = h.app.host.history().undo_count;
    h.drag(source, target);
    h.frame(vec![]);
    let arr = h.app.host.snapshot().arrangement;
    assert_eq!(arr.tracks[0].clips.len(), 1, "{:?}", h.app.error);
    assert_eq!(h.app.host.history().undo_count, history + 1);
    let clip = arr.tracks[0].clips[0].clone();
    let clip_text = text_shapes(&h.frame(vec![]))
        .into_iter()
        .find(|(rect, text)| text == "Gestures.wav" && rect.left() > 400.0 && rect.right() < 1240.0)
        .unwrap()
        .0;
    let from = clip_text.left_center() + Vec2::new(10.0, 25.0);
    let track2 = h.text_rect("Track 2");
    let to = Pos2::new(750.0, track2.center().y + 35.0);
    h.drag(from, to);
    h.frame(vec![]);
    let arr = h.app.host.snapshot().arrangement;
    assert!(arr.tracks[0].clips.is_empty());
    assert_eq!(arr.tracks[1].clips[0].id, clip.id);
    assert_eq!(h.app.host.history().undo_count, history + 2);
    let clip_text = text_shapes(&h.frame(vec![]))
        .into_iter()
        .find(|(rect, text)| text == "Gestures.wav" && rect.left() > 400.0 && rect.right() < 1240.0)
        .unwrap()
        .0;
    let from = clip_text.left_center() + Vec2::new(10.0, 25.0);
    let before = h.app.host.snapshot();
    h.drag(from, Pos2::new(20.0, 400.0));
    assert_eq!(h.app.host.snapshot(), before);
    h.frame(vec![
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.frame(vec![egui::Event::PointerMoved(target)]);
    assert_eq!(h.app.host.history().undo_count, history + 2);
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.frame(vec![egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert_eq!(h.app.host.snapshot(), before);
}

#[test]
fn clip_edge_trim_and_single_clip_effect_target_use_shared_jobs() {
    use wavebob::arrangement::*;
    let (mut h, _directory) = audio_harness();
    let doc = h.app.active_document().unwrap();
    h.app.dispatch(Command::AddTrack(AddTrackParams::default()));
    let track = h.app.host.snapshot().arrangement.tracks[0].id.clone();
    h.app.dispatch(Command::InsertClip(InsertClipParams {
        track_id: track,
        document_id: doc.id.clone(),
        revision_id: doc.revision_id.clone(),
        source_start: 0,
        source_end: doc.frames,
        position: 0,
    }));
    h.app.set_mode(ViewMode::Multitrack);
    h.app
        .dispatch(Command::SetTimelineViewport(TimelineViewportParams {
            start_frame: 0,
            end_frame: doc.frames * 2,
            expected_presentation_revision: None,
        }));
    let output = h.frame(vec![]);
    let text = text_shapes(&output)
        .into_iter()
        .find(|(r, t)| t == "Gestures.wav" && r.left() > 400.0 && r.right() < 1240.0)
        .unwrap()
        .0;
    assert!(
        text_shapes(&output)
            .iter()
            .any(|(_, text)| text == "Selected clip")
    );
    let timeline_left = text.left() - 8.0;
    let right = h.app.host.presentation().layout.effective_widths(1440.0)[1];
    let timeline_right = 1440.0 - right - 14.0;
    let edge = Pos2::new(
        timeline_left + (timeline_right - timeline_left) / 2.0 - 2.0,
        text.top() + 45.0,
    );
    let history = h.app.host.history().undo_count;
    h.drag(
        edge,
        edge - Vec2::new((timeline_right - timeline_left) / 4.0, 0.0),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h
        .app
        .host
        .snapshot()
        .jobs
        .iter()
        .any(|j| j.status == JobStatus::Running)
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    let clip = h.app.host.snapshot().arrangement.tracks[0].clips[0].clone();
    assert!(
        clip.source_end < doc.frames && clip.source_end > doc.frames / 3,
        "{:?}",
        h.app.error
    );
    assert_eq!(h.app.host.history().undo_count, history + 1);
    let owner = wavebob::processing::Owner::Clip(clip.id.clone());
    assert_eq!(h.app.stack_owner(), Some(owner.clone()));
    h.app.processing_action(
        &owner,
        wavebob::processing::Action::SetEnvelope {
            envelope: wavebob::processing::Envelope {
                gain_db: Some(0.0),
                fade_in_frames: 0,
                fade_out_frames: 128,
            },
        },
    );
    assert_eq!(
        h.app.host.snapshot().arrangement.tracks[0].clips[0]
            .processing
            .envelope
            .fade_out_frames,
        128
    );
    assert_eq!(
        h.app.active_document().unwrap().revision_id,
        doc.revision_id
    );
    h.app.navigate_arrangement(0, None, Vec::new());
    h.frame(vec![]);
    assert_eq!(h.app.stack_owner(), None);
    let arrangement = h.app.host.snapshot().arrangement;
    h.app.dispatch(Command::InsertClip(InsertClipParams {
        track_id: arrangement.tracks[0].id.clone(),
        document_id: doc.id,
        revision_id: doc.revision_id,
        source_start: 0,
        source_end: doc.frames,
        position: doc.frames * 3,
    }));
    let selected = h.app.host.snapshot().arrangement.tracks[0]
        .clips
        .iter()
        .map(|clip| clip.id.clone())
        .collect();
    h.app.navigate_arrangement(0, None, selected);
    h.frame(vec![]);
    assert_eq!(h.app.stack_owner(), None);
    assert!(
        text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text.contains("Select exactly one clip"))
    );
}

#[test]
fn crossfade_overlay_tracks_clip_drag_and_third_drop_has_visible_rejection() {
    use wavebob::arrangement::*;
    let (mut h, _dir) = audio_harness();
    let doc = h.app.active_document().unwrap();
    h.app.dispatch(Command::AddTrack(AddTrackParams::default()));
    let track = h.app.host.snapshot().arrangement.tracks[0].id.clone();
    for position in [0, 2400] {
        h.app.dispatch(Command::InsertClip(InsertClipParams {
            track_id: track.clone(),
            document_id: doc.id.clone(),
            revision_id: doc.revision_id.clone(),
            source_start: 0,
            source_end: doc.frames,
            position,
        }));
    }
    h.app.set_mode(ViewMode::Multitrack);
    h.app
        .dispatch(Command::SetTimelineViewport(TimelineViewportParams {
            start_frame: 0,
            end_frame: 14400,
            expected_presentation_revision: None,
        }));
    let frame = h.frame(vec![]);
    assert!(
        text_shapes(&frame)
            .iter()
            .any(|(_, text)| text == "Auto crossfade")
    );
    let before = h.app.host.snapshot();
    let row = h.text_rect("Track 1");
    let source = h.text_rect("Gestures.wav").center();
    let lane_left = 408.0;
    let lane_width = 818.0;
    h.drag(
        source,
        Pos2::new(
            lane_left + lane_width * 3000.0 / 14400.0,
            row.center().y + 35.0,
        ),
    );
    assert_eq!(h.app.host.snapshot(), before);
    assert!(h.app.error.as_ref().unwrap().contains("two simultaneous"));
    let frame = h.frame(vec![]);
    assert!(
        text_shapes(&frame)
            .iter()
            .any(|(_, text)| text.contains("two simultaneous"))
    );
    let second = text_shapes(&h.frame(vec![]))
        .into_iter()
        .filter(|(r, t)| t == "Gestures.wav" && r.left() > 400.0 && r.right() < 1240.0)
        .max_by(|a, b| a.0.left().total_cmp(&b.0.left()))
        .unwrap()
        .0;
    let from = second.left_center() + Vec2::new(10.0, 25.0);
    h.drag(from, from + Vec2::new(lane_width * 3000.0 / 14400.0, 0.0));
    h.frame(vec![]);
    assert!(
        h.app
            .host
            .snapshot()
            .arrangement
            .crossfades()
            .regions
            .is_empty(),
        "{:?}",
        h.app.error
    );
    assert!(
        !text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text == "Auto crossfade")
    );
}
#[test]
fn crossfade_endpoint_trims_covered_clip_and_preserves_other_clip() {
    use wavebob::arrangement::*;
    let (mut h, _dir) = audio_harness();
    let doc = h.app.active_document().unwrap();
    h.app.dispatch(Command::AddTrack(AddTrackParams::default()));
    let track = h.app.host.snapshot().arrangement.tracks[0].id.clone();
    for position in [0, 2400] {
        h.app.dispatch(Command::InsertClip(InsertClipParams {
            track_id: track.clone(),
            document_id: doc.id.clone(),
            revision_id: doc.revision_id.clone(),
            source_start: 0,
            source_end: doc.frames,
            position,
        }));
    }
    h.app.set_mode(ViewMode::Multitrack);
    h.app
        .dispatch(Command::SetTimelineViewport(TimelineViewportParams {
            start_frame: 0,
            end_frame: 14400,
            expected_presentation_revision: None,
        }));
    h.frame(vec![]);
    let original = h.app.host.snapshot().arrangement;
    let history = h.app.host.history().undo_count;
    let endpoint = h
        .frame(vec![])
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(shape) if shape.rect.size() == Vec2::new(6.0, 20.0) => {
                Some(shape.rect.center())
            }
            _ => None,
        })
        .expect("Visible crossfade endpoint handle");
    let hover = h.frame(vec![egui::Event::PointerMoved(endpoint)]);
    assert_eq!(
        hover.platform_output.cursor_icon,
        egui::CursorIcon::ResizeHorizontal
    );
    h.drag(
        endpoint,
        endpoint - Vec2::new(818.0 * 1200.0 / 14400.0, 0.0),
    );
    let wait = |h: &mut Harness| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h
            .app
            .host
            .snapshot()
            .jobs
            .iter()
            .any(|job| job.status == JobStatus::Running)
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
            h.frame(vec![]);
        }
        h.frame(vec![]);
    };
    wait(&mut h);
    let edited = h.app.host.snapshot().arrangement;
    assert_eq!(edited.tracks[0].clips[0].source_end, doc.frames - 1200);
    assert_eq!(edited.tracks[0].clips[1], original.tracks[0].clips[1]);
    assert_eq!(edited.crossfades().regions[0].end, doc.frames - 1200);
    assert_eq!(h.app.host.history().undo_count, history + 1);
    h.app.dispatch(Command::Undo(Default::default()));
    assert_eq!(h.app.host.snapshot().arrangement.tracks, original.tracks);
    h.app.dispatch(Command::Redo(Default::default()));
    assert_eq!(h.app.host.snapshot().arrangement.tracks, edited.tracks);
    // Dragging back beyond the available source clamps to the original end.
    let shortened = endpoint - Vec2::new(818.0 * 1200.0 / 14400.0, 0.0);
    h.frame(vec![egui::Event::PointerMoved(shortened)]);
    h.frame(vec![pointer_event(
        shortened,
        egui::PointerButton::Primary,
        true,
    )]);
    h.frame(vec![egui::Event::PointerMoved(endpoint)]);
    assert_eq!(h.app.host.snapshot().arrangement, edited);
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.frame(vec![pointer_event(
        endpoint,
        egui::PointerButton::Primary,
        false,
    )]);
    assert_eq!(h.app.host.snapshot().arrangement, edited);
    assert_eq!(h.app.host.history().undo_count, history + 1);
    h.drag(shortened, endpoint + Vec2::new(100.0, 0.0));
    wait(&mut h);
    assert_eq!(
        h.app.host.snapshot().arrangement.tracks[0].clips[0].source_end,
        doc.frames
    );
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
        self.input_frame(egui::RawInput {
            events,
            ..Default::default()
        })
    }
    fn input_frame(&mut self, mut input: egui::RawInput) -> egui::FullOutput {
        self.time += self.step;
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, self.size));
        input.time = Some(self.time);
        let mut output = self.ctx.run_ui(input, |ui| self.app.draw(ui));
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
fn multitrack_space_rewind_loop_track_mute_and_master_meter_use_shared_mix() {
    use wavebob::arrangement::*;
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
    let doc = h.app.active_document().unwrap();
    h.app.dispatch(Command::AddTrack(AddTrackParams::default()));
    let track = h.app.host.snapshot().arrangement.tracks[0].id.clone();
    h.app.dispatch(Command::InsertClip(InsertClipParams {
        track_id: track.clone(),
        document_id: doc.id,
        revision_id: doc.revision_id,
        source_start: 0,
        source_end: doc.frames,
        position: 0,
    }));
    h.app.set_mode(ViewMode::Multitrack);
    h.frame(vec![]);
    let tap = |h: &mut Harness, key| {
        for pressed in [true, false] {
            h.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    };
    tap(&mut h, egui::Key::Slash);
    tap(&mut h, egui::Key::Space);
    while h.app.host.mix().prepared_revision_id.as_ref()
        != Some(&h.app.host.snapshot().arrangement.revision_id)
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    let mut pcm = [[0.0; 2]; 128];
    output.render_into(&mut pcm);
    let frame = h.frame(vec![]);
    assert!(pcm.iter().flatten().any(|value| *value != 0.0));
    assert_eq!(h.app.host.snapshot().arrangement.cursor, 128);
    assert!(
        text_shapes(&frame)
            .iter()
            .any(|(_, text)| text.starts_with("Master "))
    );
    let mute = h.text_rect("M").center();
    h.click(mute);
    while h.app.host.mix().prepared_revision_id.as_ref()
        != Some(&h.app.host.snapshot().arrangement.revision_id)
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    output.render_into(&mut pcm);
    h.frame(vec![]);
    assert_eq!(pcm, [[0.0; 2]; 128]);
    tap(&mut h, egui::Key::Space);
    output.render_into(&mut []);
    h.frame(vec![]);
    assert!(!h.app.host.transport().applied.playing);
    tap(&mut h, egui::Key::Enter);
    output.render_into(&mut []);
    h.frame(vec![]);
    assert_eq!(h.app.host.snapshot().arrangement.cursor, 0);
    assert!(h.app.host.snapshot().loop_enabled);
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

#[derive(Debug)]
struct DroppedPath(std::path::PathBuf);
impl egui::DroppedFile for DroppedPath {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        panic!("File contents must be read by the background core importer")
    }
}
fn dropped(paths: impl IntoIterator<Item = std::path::PathBuf>) -> egui::RawInput {
    egui::RawInput {
        dropped_files: paths
            .into_iter()
            .map(|path| std::sync::Arc::new(DroppedPath(path)) as egui::DroppedFileHandle)
            .collect(),
        ..Default::default()
    }
}
fn finish_dropped_files(h: &mut Harness) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        h.frame(vec![]);
        assert!(
            h.app
                .host
                .snapshot()
                .jobs
                .iter()
                .filter(|job| job.status == JobStatus::Running)
                .count()
                <= 2
        );
        if h.app.file_imports.paths.is_empty()
            && !h
                .app
                .host
                .snapshot()
                .jobs
                .iter()
                .any(|job| job.status == JobStatus::Running)
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{:?}", h.app.error);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn native_file_drop_batches_import_once_without_placing_clips() {
    let (mut h, dir) = audio_harness();
    let source = dir.path().join("Gestures.wav");
    let mut paths = Vec::new();
    for name in ["One.WAV", "Two.wav", "音色 three.wav"] {
        let path = dir.path().join(name);
        std::fs::copy(&source, &path).unwrap();
        paths.push(path);
    }
    let mp3 = dir.path().join("Four.MP3");
    std::fs::copy("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3", &mp3).unwrap();
    paths.push(mp3);
    h.app.dispatch(Command::AddTrack(
        wavebob::arrangement::AddTrackParams::default(),
    ));
    h.app.set_mode(ViewMode::Multitrack);
    h.input_frame(dropped(paths));
    finish_dropped_files(&mut h);
    assert_eq!(
        h.app.host.snapshot().documents.len(),
        5,
        "{:?}",
        h.app.error
    );
    assert_eq!(h.app.host.presentation().mode, ViewMode::Waveform);
    assert!(h.app.host.snapshot().arrangement.tracks[0].clips.is_empty());
    assert!(
        h.app
            .host
            .snapshot()
            .documents
            .iter()
            .any(|doc| doc.encoding == "mp3")
    );
    let jobs = h.app.host.snapshot().jobs.len();
    for _ in 0..3 {
        h.frame(vec![]);
    }
    assert_eq!(h.app.host.snapshot().jobs.len(), jobs);
    assert!(h.app.error.is_none(), "{:?}", h.app.error);
}
#[test]
fn file_drop_errors_preserve_existing_audio_and_continue_the_batch() {
    let (mut h, dir) = audio_harness();
    let original = h.app.active_document().unwrap();
    h.input_frame(dropped([dir.path().join("notes.txt")]));
    assert!(h.app.error.as_ref().unwrap().contains("WAV or MP3"));
    assert_eq!(h.app.active_document().unwrap(), original);
    h.input_frame(dropped([
        dir.path().join("missing.wav"),
        dir.path().join("Gestures.wav"),
    ]));
    finish_dropped_files(&mut h);
    assert_eq!(h.app.host.snapshot().documents.len(), 2);
    assert!(
        h.app
            .host
            .snapshot()
            .jobs
            .iter()
            .any(|job| job.status == JobStatus::Failed)
    );
    assert!(h.app.error.is_some());
}
#[test]
fn file_drops_wait_for_dialogs_and_queued_imports_cancel_on_workspace_close() {
    let (mut h, dir) = dirty_audio_harness();
    let (_sender, receiver) = mpsc::channel();
    h.app.dialog = Some(receiver);
    h.input_frame(dropped([dir.path().join("Gestures.wav")]));
    assert_eq!(h.app.file_imports.paths.len(), 1);
    let count = h.app.host.snapshot().jobs.len();
    h.frame(vec![]);
    assert_eq!(h.app.host.snapshot().jobs.len(), count);
    h.app.dialog = None;
    finish_dropped_files(&mut h);
    assert_eq!(h.app.host.snapshot().documents.len(), 2);
    let (_sender, receiver) = mpsc::channel();
    h.app.dialog = Some(receiver);
    h.input_frame(dropped([dir.path().join("Gestures.wav")]));
    h.app.begin_close(saving::CloseTarget::Quit);
    assert!(h.app.file_imports.paths.is_empty());
    assert!(h.app.close_flow.is_some());
    h.app.close_flow = None;
    h.app.dialog = None;
    h.frame(vec![]);
    assert_eq!(h.app.host.snapshot().documents.len(), 2);
}
#[test]
fn file_drop_hover_is_visible_without_importing_and_clears_after_leaving() {
    let mut h = Harness::new();
    let initial = h.app.host.snapshot();
    let output = h.input_frame(egui::RawInput {
        hovered_files: vec![egui::HoveredFile {
            path: Some("Audio.wav".into()),
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(
        text_shapes(&output)
            .iter()
            .any(|(_, text)| text == "Drop audio to open")
    );
    assert_eq!(h.app.host.snapshot(), initial);
    assert!(
        !text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text == "Drop audio to open")
    );
}

#[test]
fn file_drop_waits_for_in_flight_capture_without_losing_the_event() {
    let (mut h, dir) = audio_harness();
    let runtime = tempfile::tempdir().unwrap();
    h.app
        .host
        .listen(runtime.path(), "drop-capture".into(), h.ctx.clone())
        .unwrap();
    let client =
        wavebob::ipc::Client::attach(runtime.path(), &h.app.host.snapshot().session_id).unwrap();
    let capture = std::thread::spawn(move || {
        client
            .call(r#"{"id":"capture","command":"capture.png","params":{"timeout_ms":750}}"#)
            .unwrap()
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !h.app.host.frozen() {
        h.frame(vec![]);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let before = h.app.host.snapshot();
    h.input_frame(dropped([dir.path().join("Gestures.wav")]));
    assert_eq!(h.app.file_imports.paths.len(), 1);
    assert_eq!(h.app.host.snapshot(), before);
    // This painter harness has no framebuffer readback. Let the capture time
    // out, then verify its deferred native event resumes through the core.
    while h.app.host.frozen() {
        h.frame(vec![]);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(matches!(
        capture.join().unwrap().outcome,
        Outcome::Error { .. }
    ));
    finish_dropped_files(&mut h);
    assert_eq!(h.app.host.snapshot().documents.len(), 2);
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

#[test]
fn reverse_and_shared_history_remain_edit_actions_with_the_current_selection() {
    let (mut h, _directory) = audio_harness();
    let document = h.app.active_document().unwrap();
    h.app.dispatch(Command::SetSelection(SelectionParams {
        document_id: document.id.clone(),
        start_frame: 100,
        end_frame: 4000,
    }));
    let edit = h.text_rect("Edit").center();
    h.click(edit);
    let reverse = h.text_rect("Reverse").center();
    h.click(reverse);
    finish_gui_jobs(&mut h);
    let edited = h.app.active_document().unwrap();
    assert_ne!(edited.revision_id, document.revision_id);
    assert!(edited.dirty);
    assert_eq!(h.app.host.history().undo_count, 1);
    let edit = h.text_rect("Edit").center();
    h.click(edit);
    let undo = h.text_rect("Undo    Ctrl/Cmd+Z").center();
    h.click(undo);
    assert_eq!(
        h.app.active_document().unwrap().revision_id,
        document.revision_id
    );
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    }]);
    assert_eq!(
        h.app.active_document().unwrap().revision_id,
        edited.revision_id
    );
    assert_eq!(
        h.app.active_document().unwrap().selection,
        Some(FrameRange {
            start: 100,
            end: 4000
        })
    );
}

#[test]
fn audio_clipboard_shortcuts_delete_all_paste_and_undo_without_os_text() {
    let (mut h, _d) = audio_harness();
    let doc = h.app.active_document().unwrap();
    h.app.dispatch(Command::SetSelection(SelectionParams {
        document_id: doc.id.clone(),
        start_frame: 1000,
        end_frame: 2000,
    }));
    let tap = |h: &mut Harness, key, modifiers| {
        for pressed in [true, false] {
            h.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }]);
        }
    };
    tap(&mut h, egui::Key::C, egui::Modifiers::CTRL);
    assert_eq!(h.app.host.clipboard().frames, 1000);
    assert!(!h.app.host.history().can_undo);
    h.app.dispatch(Command::SetViewport(SetViewportParams {
        document_id: doc.id.clone(),
        start_frame: 1000,
        end_frame: 3000,
        expected_presentation_revision: None,
    }));
    h.app.dispatch(Command::SelectAll(DocumentParams {
        document_id: doc.id.clone(),
    }));
    tap(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.active_document().unwrap().frames != 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    assert!(
        text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text.starts_with("Empty sample"))
    );
    tap(&mut h, egui::Key::V, egui::Modifiers::CTRL);
    while h.app.active_document().unwrap().frames == 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    assert_eq!(h.app.active_document().unwrap().frames, 1000);
    assert_eq!(h.app.host.history().undo_count, 2);
    tap(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(h.app.active_document().unwrap().frames, 0);
    tap(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(h.app.active_document().unwrap().frames, doc.frames);
    assert_eq!(
        h.app
            .host
            .presentation()
            .visible_interval(&h.app.active_document().unwrap()),
        FrameRange {
            start: 0,
            end: doc.frames
        }
    );
}

#[test]
fn text_clipboard_keys_delegate_to_the_platform_while_audio_keys_stay_raw() {
    let mut h = Harness::new();
    let event = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    };
    let mut raw = egui::RawInput {
        events: vec![event(egui::Key::V)],
        ..Default::default()
    };
    h.app.text_clipboard_input(&h.ctx, &mut raw);
    assert!(matches!(
        raw.events[0],
        egui::Event::Key {
            key: egui::Key::V,
            ..
        }
    ));
    let mut text = String::from("folder");
    let id = egui::Id::unique("clipboard-path");
    for _ in 0..2 {
        let mut output = h.ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(id))
                .request_focus();
        });
        output.textures_delta.clear();
    }
    assert!(h.ctx.text_edit_focused());
    let mut raw = egui::RawInput {
        events: vec![
            event(egui::Key::C),
            event(egui::Key::X),
            event(egui::Key::V),
        ],
        ..Default::default()
    };
    h.app.text_clipboard_input(&h.ctx, &mut raw);
    assert_eq!(raw.events.len(), 2);
    assert!(matches!(raw.events[0], egui::Event::Copy));
    assert!(matches!(raw.events[1], egui::Event::Cut));
    let mut output = h.ctx.run_ui(raw, |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).id(id));
    });
    assert!(output.viewport_output.values().any(|viewport| {
        viewport
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::RequestPaste))
    }));
    output.textures_delta.clear();
}

#[test]
fn compact_stack_has_scope_and_management_without_inline_effect_parameters() {
    let (mut h, _d) = audio_harness();
    let before = h.app.active_document().unwrap();
    let texts = text_shapes(&h.frame(vec![]));
    for label in [
        "EFFECTS",
        "Whole document",
        "＋ Add effect",
        "Bypass all",
        "Processed output",
    ] {
        assert!(texts.iter().any(|(_, text)| text == label), "{label}");
    }
    for old in [
        "Apply", "Fade in", "Fade out", "Gain", "Room", "Decay s", "Wet",
    ] {
        assert!(
            !texts.iter().any(|(_, text)| text == old),
            "Old inline control {old}"
        );
    }
    assert_eq!(h.app.active_document().unwrap(), before);
    assert!(!h.app.host.history().can_undo);
    h.size = Vec2::new(800.0, 520.0);
    h.frame(vec![]);
    assert!(h.text_rect("›").top() < 70.0);
}

fn dirty_audio_harness() -> (Harness, tempfile::TempDir) {
    let (mut h, d) = audio_harness();
    let edit = h.text_rect("Edit").center();
    h.click(edit);
    let reverse = h.text_rect("Reverse").center();
    h.click(reverse);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !h.app.active_document().unwrap().dirty {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    (h, d)
}
fn finish_gui_jobs(h: &mut Harness) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        h.frame(vec![]);
        if !h
            .app
            .host
            .snapshot()
            .jobs
            .iter()
            .any(|job| job.status == JobStatus::Running)
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{:?}", h.app.error);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn project_picker_save_preserves_source_and_project_close_saves_arrangement() {
    let (mut h, directory) = dirty_audio_harness();
    let source = directory.path().join("Gestures.wav");
    let original = std::fs::read(&source).unwrap();
    h.app.dispatch(Command::AddTrack(
        wavebob::arrangement::AddTrackParams::default(),
    ));
    let path = directory.path().join("Portable.wavebob");
    let (sender, receiver) = mpsc::channel();
    h.app.project_picker = Some(projecting::ProjectPicker {
        receiver,
        save: true,
        close_after: false,
        revision: h.app.host.snapshot().project.revision_id,
    });
    sender.send(Some(path.clone())).unwrap();
    finish_gui_jobs(&mut h);
    assert!(path.exists(), "{:?}", h.app.error);
    assert!(!h.app.host.snapshot().project.dirty);
    assert!(h.app.active_document().unwrap().dirty);
    assert_eq!(std::fs::read(source).unwrap(), original);
    h.app.dispatch(Command::AddTrack(
        wavebob::arrangement::AddTrackParams::default(),
    ));
    h.app.begin_close(saving::CloseTarget::Quit);
    h.frame(vec![]);
    let save = h.text_rect("Save project").center();
    h.click(save);
    finish_gui_jobs(&mut h);
    h.frame(vec![]);
    assert!(
        h.app.host.presentation().close_requested,
        "{:?}",
        h.app.error
    );
    assert!(!h.app.host.snapshot().project.dirty);
    assert!(h.app.active_document().unwrap().dirty);
}
#[test]
fn project_open_cancel_discard_and_failed_load_keep_workspace_safe() {
    let (mut h, directory) = dirty_audio_harness();
    let original = h.app.active_document().unwrap();
    let missing = directory.path().join("Missing.wavebob");
    h.app.begin_close(saving::CloseTarget::OpenProject(
        missing.to_string_lossy().into_owned(),
    ));
    h.frame(vec![]);
    let cancel = h.text_rect("Cancel").center();
    h.click(cancel);
    assert!(h.app.close_flow.is_none());
    assert_eq!(h.app.active_document().unwrap(), original);
    h.app.begin_close(saving::CloseTarget::OpenProject(
        missing.to_string_lossy().into_owned(),
    ));
    h.frame(vec![]);
    let discard = h.text_rect("Discard").center();
    h.click(discard);
    finish_gui_jobs(&mut h);
    assert_eq!(h.app.active_document().unwrap(), original);
    assert!(
        h.app
            .host
            .snapshot()
            .jobs
            .iter()
            .any(|job| job.status == JobStatus::Failed)
    );
    let (sender, receiver) = mpsc::channel();
    h.app.project_picker = Some(projecting::ProjectPicker {
        receiver,
        save: true,
        close_after: false,
        revision: h.app.host.snapshot().project.revision_id,
    });
    sender.send(None).unwrap();
    h.frame(vec![]);
    assert!(h.app.project_picker.is_none());
    assert_eq!(h.app.active_document().unwrap(), original);
}
#[test]
fn dirty_document_close_can_cancel_discard_or_save_before_closing() {
    for save in [false, true] {
        let (mut h, d) = dirty_audio_harness();
        let id = h.app.active_document().unwrap().id;
        let close = h.text_rect("×").center();
        h.click(close);
        assert!(h.app.close_flow.is_some());
        assert!(h.app.active_document().unwrap().dirty);
        let cancel = h.text_rect("Cancel").center();
        h.click(cancel);
        assert!(h.app.close_flow.is_none());
        assert_eq!(h.app.active_document().unwrap().id, id);
        h.click(close);
        let action = h.text_rect(if save { "Save" } else { "Discard" }).center();
        h.click(action);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !h.app.host.snapshot().documents.is_empty() {
            assert!(std::time::Instant::now() < deadline, "{:?}", h.app.error);
            std::thread::sleep(std::time::Duration::from_millis(2));
            h.frame(vec![]);
        }
        assert!(h.app.close_flow.is_none());
        if save {
            let mut reader = hound::WavReader::open(d.path().join("Gestures.wav")).unwrap();
            assert_eq!(reader.samples::<i16>().next().unwrap().unwrap(), 19600);
        }
    }
}
#[test]
fn gui_export_dialog_shows_lossy_settings_and_submits_mp3_through_the_core() {
    let (mut h, d) = audio_harness();
    let file = h.text_rect("File").center();
    h.click(file);
    let export = h.text_rect("Export audio…").center();
    h.click(export);
    assert!(h.app.output_dialog.is_some());
    let format = h.text_rect("WAV · 24-bit PCM").center();
    h.click(format);
    let mp3 = h.text_rect("MP3 · lossy").center();
    h.click(mp3);
    assert!(
        text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text.contains("MP3 uses lossy encoding"))
    );
    let path = d.path().join("UI export.mp3");
    let (sender, receiver) = mpsc::channel();
    h.app.save_picker = Some(receiver);
    sender.send(Some(path.clone())).unwrap();
    h.frame(vec![]);
    let export = h.text_rect("Export").center();
    h.click(export);
    assert!(h.app.output_dialog.is_none());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        h.frame(vec![]);
        if h.app
            .host
            .snapshot()
            .jobs
            .iter()
            .any(|job| job.kind == "Export" && job.status == JobStatus::Succeeded)
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{:?}", h.app.error);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(path.exists());
    assert!(!h.app.active_document().unwrap().dirty);
    assert!(!h.app.host.history().can_undo);
}

#[test]
fn multitrack_export_dialog_submits_selected_stereo_mp3_without_a_device() {
    use wavebob::arrangement::*;
    let (mut h, d) = audio_harness();
    let doc = h.app.active_document().unwrap();
    h.app.dispatch(Command::AddTrack(AddTrackParams::default()));
    let track = h.app.host.snapshot().arrangement.tracks[0].id.clone();
    for position in [0, 2400] {
        h.app.dispatch(Command::InsertClip(InsertClipParams {
            track_id: track.clone(),
            document_id: doc.id.clone(),
            revision_id: doc.revision_id.clone(),
            source_start: 0,
            source_end: doc.frames,
            position,
        }));
    }
    h.app.set_mode(ViewMode::Multitrack);
    h.app.navigate_arrangement(
        1200,
        Some(FrameRange {
            start: 1200,
            end: 6000,
        }),
        Vec::new(),
    );
    h.frame(vec![]);
    let file = h.text_rect("File").center();
    h.click(file);
    let export = h.text_rect("Export audio…").center();
    h.click(export);
    assert!(h.app.output_dialog.is_some());
    assert!(
        text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text == "Export arrangement")
    );
    let format = h.text_rect("WAV · 24-bit PCM").center();
    h.click(format);
    let mp3 = h.text_rect("MP3 · lossy").center();
    h.click(mp3);
    let path = d.path().join("UI mix.mp3");
    let (sender, receiver) = mpsc::channel();
    h.app.save_picker = Some(receiver);
    sender.send(Some(path.clone())).unwrap();
    h.frame(vec![]);
    let export = h.text_rect("Export").center();
    h.click(export);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h
        .app
        .host
        .snapshot()
        .jobs
        .iter()
        .any(|job| job.status == JobStatus::Running)
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    assert!(path.exists(), "{:?}", h.app.error);
    let summary = h
        .app
        .host
        .snapshot()
        .jobs
        .into_iter()
        .find(|job| job.kind == "Export mix")
        .unwrap();
    assert_eq!(summary.status, JobStatus::Succeeded);
    let Some(JobOutput::AudioExport(output)) = summary.output else {
        panic!("mix output summary")
    };
    assert_eq!(output.source_start_frame, 1200);
    assert_eq!(output.source_end_frame, 6000);
    assert_eq!(output.frames, 4800);
    assert_eq!(output.channels, 2);
    assert!(h.app.host.snapshot().arrangement.dirty);
}
#[test]
fn native_quit_intent_requires_a_dirty_work_choice_and_discard_is_explicit() {
    let (mut h, _d) = dirty_audio_harness();
    h.app.os_close_pending = true;
    h.frame(vec![]);
    assert!(h.app.close_flow.is_some());
    assert!(!h.app.host.presentation().close_requested);
    let cancel = h.text_rect("Cancel").center();
    h.click(cancel);
    assert!(!h.app.host.presentation().close_requested);
    h.app.os_close_pending = true;
    h.frame(vec![]);
    let discard = h.text_rect("Discard").center();
    h.click(discard);
    h.frame(vec![]);
    assert!(h.app.host.presentation().close_requested);
}

#[test]
fn closing_an_edited_mp3_confirms_encoding_and_offers_lossless_wav_save_as() {
    let mut h = Harness::new();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3");
    let original = std::fs::read(&path).unwrap();
    h.app.open_path(path.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.active_document().is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    let edit = h.text_rect("Edit").center();
    h.click(edit);
    let reverse = h.text_rect("Reverse").center();
    h.click(reverse);
    while !h.app.active_document().unwrap().dirty {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    let close = h.text_rect("×").center();
    h.click(close);
    let save = h.text_rect("Save").center();
    h.click(save);
    assert!(h.app.output_dialog.is_some());
    assert!(
        text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text.contains("MP3 uses lossy encoding"))
    );
    let format = h.text_rect("MP3 · lossy").center();
    h.click(format);
    let wav = h.text_rect("WAV · 24-bit PCM").center();
    h.click(wav);
    let d = tempfile::tempdir().unwrap();
    let output = d.path().join("Lossless edit.wav");
    let (sender, receiver) = mpsc::channel();
    h.app.save_picker = Some(receiver);
    sender.send(Some(output.clone())).unwrap();
    h.frame(vec![]);
    let save = h.text_rect("Save").center();
    h.click(save);
    while !h.app.host.snapshot().documents.is_empty() {
        assert!(std::time::Instant::now() < deadline, "{:?}", h.app.error);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    let reader = hound::WavReader::open(output).unwrap();
    assert_eq!(reader.spec().bits_per_sample, 24);
    assert_eq!(reader.duration(), 8192);
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn quit_during_an_edit_waits_and_prompts_instead_of_saving_without_consent() {
    let (mut h, d) = audio_harness();
    let doc = h.app.active_document().unwrap();
    let original = std::fs::read(d.path().join("Gestures.wav")).unwrap();
    h.app
        .dispatch(Command::ApplyEffect(wavebob::effects::ApplyEffectParams {
            target: wavebob::effects::EditTarget::Whole {
                document_id: doc.id,
                revision_id: doc.revision_id,
            },
            effect: wavebob::effects::Effect::Reverse,
        }));
    h.app.begin_close(saving::CloseTarget::Quit);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !h.app.active_document().unwrap().dirty {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.frame(vec![]);
    }
    h.frame(vec![]);
    assert!(!h.app.host.presentation().close_requested);
    assert!(h.app.close_flow.is_some());
    assert!(
        !h.app
            .host
            .snapshot()
            .jobs
            .iter()
            .any(|job| job.kind == "Save")
    );
    assert_eq!(
        std::fs::read(d.path().join("Gestures.wav")).unwrap(),
        original
    );
    assert!(
        text_shapes(&h.frame(vec![]))
            .iter()
            .any(|(_, text)| text == "Unsaved audio")
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
fn waveform_selection_edges_resize_with_opposite_boundary_fixed() {
    let (mut h, _directory) = audio_harness();
    let doc = h.app.active_document().unwrap();
    let rect = visuals::main_plot(canvas_rects(&mut h)[1]);
    let point = |frame: u64| {
        Pos2::new(
            rect.left() + rect.width() * frame as f32 / 4800.0,
            rect.center().y,
        )
    };
    h.app.dispatch(Command::SetSelection(SelectionParams {
        document_id: doc.id.clone(),
        start_frame: 1200,
        end_frame: 3600,
    }));
    h.frame(vec![]);
    let output = h.frame(vec![egui::Event::PointerMoved(point(1200))]);
    assert_eq!(
        output.platform_output.cursor_icon,
        egui::CursorIcon::ResizeHorizontal
    );
    for (from, to, start, end) in [
        (1200, 600, 600, 3600),
        (3600, 4200, 600, 4200),
        (600, 4500, 4200, 4500),
        (4500, 4800, 4200, 4800),
    ] {
        h.drag(point(from), point(to));
        assert_eq!(
            h.app.active_document().unwrap().selection,
            Some(FrameRange { start, end })
        );
    }
    let before = h.app.host.snapshot();
    let from = point(4200);
    let to = point(2400);
    h.frame(vec![egui::Event::PointerMoved(from)]);
    h.frame(vec![pointer_event(
        from,
        egui::PointerButton::Primary,
        true,
    )]);
    h.frame(vec![egui::Event::PointerMoved(to)]);
    assert_eq!(
        h.app.display_document().unwrap().selection,
        Some(FrameRange {
            start: 2400,
            end: 4800
        })
    );
    assert_eq!(h.app.host.snapshot(), before);
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.frame(vec![pointer_event(to, egui::PointerButton::Primary, false)]);
    assert_eq!(h.app.host.snapshot(), before);
    assert_eq!(
        h.app.active_document().unwrap().revision_id,
        doc.revision_id
    );
    assert!(!h.app.active_document().unwrap().dirty);
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

#[test]
fn native_knob_drag_and_fine_keyboard_emit_bounded_values() {
    let ctx = egui::Context::default();
    let mut value = 50.0;
    let mut render = |mut events: Vec<egui::Event>, modifiers| {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let mut center = Pos2::ZERO;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0))),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                let response = effect_controls::knob(ui, &mut value, 0.0, 100.0, 1.0);
                center = response.rect.center();
            },
        );
        output.textures_delta.clear();
        (center, value)
    };
    let (center, _) = render(vec![], egui::Modifiers::NONE);
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    render(
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
        egui::Modifiers::NONE,
    );
    let (_, dragged) = render(
        vec![egui::Event::PointerMoved(center - Vec2::new(0.0, 32.0))],
        egui::Modifiers::NONE,
    );
    assert!((dragged - 70.0).abs() < 0.001, "{dragged}");
    render(
        vec![pointer(center - Vec2::new(0.0, 32.0), false)],
        egui::Modifiers::NONE,
    );
    render(
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
        egui::Modifiers::NONE,
    );
    render(vec![pointer(center, false)], egui::Modifiers::NONE);
    let (_, fine) = render(
        vec![egui::Event::Key {
            key: egui::Key::ArrowUp,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }],
        egui::Modifiers::SHIFT,
    );
    assert!((fine - 70.1).abs() < 0.001, "{fine}");
}

#[test]
fn compact_rows_emit_stable_bypass_open_and_drag_reorder_actions() {
    use wavebob::processing::{Action, EffectKind, Instance, Processing};
    let ctx = egui::Context::default();
    let first = uuid::Uuid::new_v4().to_string();
    let second = uuid::Uuid::new_v4().to_string();
    let state = Processing {
        effects: vec![
            Instance {
                id: first.clone(),
                kind: EffectKind::SoftClipper,
                bypassed: false,
                parameters: Default::default(),
            },
            Instance {
                id: second.clone(),
                kind: EffectKind::Reverb,
                bypassed: false,
                parameters: Default::default(),
            },
        ],
        ..Default::default()
    };
    let mut drag = None;
    let mut time = 0.0;
    let mut render = |events| {
        time += 0.1;
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                time: Some(time),
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 240.0))),
                ..Default::default()
            },
            |ui| actions = effects::stack_rows(ui, &state, &mut drag, &mut Help::default()),
        );
        output.textures_delta.clear();
        (output, actions)
    };
    let (output, _) = render(Vec::new());
    let texts = text_shapes(&output);
    let first_label = texts
        .iter()
        .find(|(_, text)| text == "Soft Clipper")
        .unwrap()
        .0;
    let grips: Vec<_> = texts
        .iter()
        .filter(|(_, text)| text == "⠿")
        .map(|(rect, _)| rect.center())
        .collect();
    let opens: Vec<_> = texts
        .iter()
        .filter(|(_, text)| text == "↗")
        .map(|(rect, _)| rect.center())
        .collect();
    let checkbox = Pos2::new(first_label.left() - 18.0, first_label.center().y);
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    render(vec![
        egui::Event::PointerMoved(checkbox),
        pointer(checkbox, true),
    ]);
    let (_, actions) = render(vec![pointer(checkbox, false)]);
    assert!(actions.iter().any(|action|matches!(action,effects::StackEvent::Edit(Action::Bypass{effect_id,bypassed:true}) if effect_id == &first)),"{actions:?}");
    render(vec![
        egui::Event::PointerMoved(opens[0]),
        pointer(opens[0], true),
    ]);
    let (_, actions) = render(vec![pointer(opens[0], false)]);
    assert!(
        actions
            .iter()
            .any(|action| matches!(action,effects::StackEvent::Open(id) if id == &first)),
        "{actions:?}"
    );
    render(vec![
        egui::Event::PointerMoved(grips[0]),
        pointer(grips[0], true),
    ]);
    render(vec![egui::Event::PointerMoved(grips[1])]);
    let (_, actions) = render(vec![pointer(grips[1], false)]);
    assert!(actions.iter().any(|action|matches!(action,effects::StackEvent::Edit(Action::Move{effect_id,index:1}) if effect_id == &first)),"{actions:?}");
}
