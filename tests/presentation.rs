#![cfg(feature = "gui")]

use wavebob::{Command, ErrorCode, Outcome, Request, desktop::DesktopHost, presentation::*};

fn command(host: &mut DesktopHost, command: Command) -> wavebob::Response {
    host.execute(Request::new("presentation-test", command))
}
fn layout(host: &mut DesktopHost, params: SetLayoutParams) -> wavebob::Response {
    command(host, Command::SetLayout(params))
}

#[test]
fn effect_parameters_are_revision_guarded_presentation_without_audio_mutation() {
    use wavebob::effects::Effect;
    let mut host = DesktopHost::default();
    let core = host.snapshot();
    let effect = Effect::Reverb {
        room_size: 0.5,
        decay_seconds: 1.5,
        wet: 0.25,
    };
    assert!(matches!(
        command(
            &mut host,
            Command::SetEffect(SetEffectParams {
                effect: effect.clone(),
                expected_presentation_revision: Some(0)
            })
        )
        .outcome,
        Outcome::Ok { .. }
    ));
    assert_eq!(host.presentation().effect, effect);
    assert_eq!(host.presentation().revision, 1);
    assert_eq!(host.snapshot(), core);
    let view = host.presentation();
    for (effect, expected, code) in [
        (Effect::Gain { db: 3.0 }, Some(0), ErrorCode::StaleRevision),
        (
            Effect::Reverb {
                room_size: 2.0,
                decay_seconds: 1.5,
                wet: 0.25,
            },
            Some(1),
            ErrorCode::InvalidParams,
        ),
    ] {
        assert!(
            matches!(command(&mut host,Command::SetEffect(SetEffectParams{effect,expected_presentation_revision:expected})).outcome,Outcome::Error{error} if error.code==code)
        );
        assert_eq!(host.presentation(), view);
        assert_eq!(host.snapshot(), core);
    }
    command(
        &mut host,
        Command::SetEffect(SetEffectParams {
            effect,
            expected_presentation_revision: Some(1),
        }),
    );
    assert_eq!(host.presentation(), view);
}

#[test]
fn viewport_commands_validate_bounds_and_preserve_core_selection_and_audio() {
    use wavebob::documents::*;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("viewport.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..100 {
        writer.write_sample(1_i16).unwrap();
    }
    writer.finalize().unwrap();
    let mut host = DesktopHost::default();
    command(
        &mut host,
        Command::OpenDocument(OpenParams {
            path: path.to_str().unwrap().into(),
        }),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = eframe::egui::Context::default();
    while host.snapshot().documents.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        host.begin_frame(&ctx);
    }
    let doc = host.snapshot().documents[0].clone();
    command(
        &mut host,
        Command::SetSelection(SelectionParams {
            document_id: doc.id.clone(),
            start_frame: 20,
            end_frame: 40,
        }),
    );
    let core = host.snapshot();
    let set = |start, end, expected| {
        Command::SetViewport(SetViewportParams {
            document_id: doc.id.clone(),
            start_frame: start,
            end_frame: end,
            expected_presentation_revision: expected,
        })
    };
    assert!(matches!(
        command(&mut host, set(10, 60, Some(0))).outcome,
        Outcome::Ok { .. }
    ));
    let expected = host.presentation();
    assert_eq!(
        expected.visible_interval(&doc),
        FrameRange { start: 10, end: 60 }
    );
    assert_eq!(host.snapshot(), core);
    command(&mut host, set(10, 60, Some(1)));
    assert_eq!(host.presentation(), expected);
    for input in [
        set(10, 10, None),
        set(70, 20, None),
        set(0, 101, None),
        set(10, 60, Some(0)),
    ] {
        assert!(matches!(
            command(&mut host, input).outcome,
            Outcome::Error { .. }
        ));
        assert_eq!(host.presentation(), expected);
        assert_eq!(host.snapshot(), core);
    }
    command(&mut host, set(99, 100, None));
    assert_eq!(
        host.presentation().visible_interval(&doc),
        FrameRange {
            start: 99,
            end: 100
        }
    );
    command(
        &mut host,
        Command::CloseDocument(CloseParams {
            document_id: doc.id.clone(),
            discard_changes: false,
        }),
    );
    host.begin_frame(&ctx);
    assert!(host.presentation().viewports.is_empty());
}

#[test]
fn pan_and_pointer_anchored_zoom_preserve_bounds_and_independent_selection() {
    use wavebob::documents::FrameRange;
    let view = FrameRange { start: 20, end: 60 };
    assert_eq!(
        pan(&view, 200.0, 100),
        FrameRange {
            start: 60,
            end: 100
        }
    );
    assert_eq!(pan(&view, -200.0, 100), FrameRange { start: 0, end: 40 });
    assert_eq!(
        zoom(&view, 2.0, 0.25, 100),
        FrameRange { start: 25, end: 45 }
    );
    assert_eq!(
        zoom(&view, 0.001, 0.5, 100),
        FrameRange { start: 0, end: 100 }
    );
    for total in [1, 2, 100] {
        for factor in [0.001, 0.5, 1.0, 2.0, 10000.0] {
            for anchor in [0.0, 0.25, 0.5, 1.0] {
                let view = FrameRange {
                    start: 0,
                    end: total,
                };
                let next = zoom(&view, factor, anchor, total);
                assert!(next.start < next.end && next.end <= total);
                assert_eq!(pan(&next, 2000.0, total).end, total);
            }
        }
    }
}

#[test]
fn panel_commands_are_atomic_revision_guarded_and_keep_core_state_unchanged() {
    let mut host = DesktopHost::default();
    let original = host.snapshot();
    let configured = layout(
        &mut host,
        SetLayoutParams {
            left_width: Some(320.0),
            right_width: Some(290.0),
            library_fraction: Some(0.4),
            expected_presentation_revision: Some(0),
            ..Default::default()
        },
    );
    assert!(matches!(configured.outcome, Outcome::Ok { .. }));
    let expected = host.presentation();
    for params in [
        SetLayoutParams {
            left_width: Some(179.0),
            right_visible: Some(false),
            ..Default::default()
        },
        SetLayoutParams {
            left_width: Some(f64::NAN),
            ..Default::default()
        },
        SetLayoutParams {
            library_fraction: Some(0.9),
            ..Default::default()
        },
        SetLayoutParams {
            left_visible: Some(false),
            expected_presentation_revision: Some(0),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            layout(&mut host, params).outcome,
            Outcome::Error { .. }
        ));
        assert_eq!(host.presentation(), expected);
    }
    layout(
        &mut host,
        SetLayoutParams {
            left_width: Some(320.0),
            ..Default::default()
        },
    );
    assert_eq!(
        host.presentation().revision,
        1,
        "No-op layouts do not create revisions"
    );
    layout(
        &mut host,
        SetLayoutParams {
            left_visible: Some(false),
            ..Default::default()
        },
    );
    command(
        &mut host,
        Command::TogglePanels(PresentationRevisionParams::default()),
    );
    let hidden = host.presentation().layout;
    assert!(!hidden.left.visible && !hidden.right.visible);
    command(
        &mut host,
        Command::TogglePanels(PresentationRevisionParams::default()),
    );
    assert_eq!(host.presentation().layout, expected.layout);
    assert_eq!(host.snapshot(), original);
    let stale = host.execute(
        Request::new(
            "stale-core",
            Command::SetLayout(SetLayoutParams {
                left_visible: Some(false),
                ..Default::default()
            }),
        )
        .expecting_revision(99),
    );
    assert!(matches!(stale.outcome,Outcome::Error {error} if error.code==ErrorCode::StaleRevision));
}

#[test]
fn narrow_windows_preserve_the_center_and_remember_expanded_sizes() {
    let mut layout = PanelLayout::default();
    layout.left.width = 420.0;
    layout.right.width = 360.0;
    assert_eq!(layout.effective_widths(1440.0), [420.0, 360.0]);
    for width in [800.0, 900.0, 1000.0] {
        let widths = layout.effective_widths(width);
        assert!(widths.iter().all(|width| *width >= MIN_PANEL_WIDTH));
        assert!(width - widths.iter().sum::<f32>() >= MIN_CENTER_WIDTH - 0.01);
    }
    layout.left.visible = false;
    assert_eq!(layout.effective_widths(800.0)[0], COLLAPSED_WIDTH);
    assert_eq!(layout.left.width, 420.0);
    layout.left.visible = true;
    assert_eq!(layout.effective_widths(1440.0), [420.0, 360.0]);
}

fn wait_preferences(host: &mut DesktopHost, ctx: &eframe::egui::Context) -> PreferenceStatus {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        host.begin_frame(ctx);
        let status = host.presentation().preferences;
        if !matches!(status, PreferenceStatus::Saving) {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Preference worker did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn preferences_restore_layout_after_restart_and_report_load_or_save_failures() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    std::fs::write(&path, "invalid-json").unwrap();
    let ctx = eframe::egui::Context::default();
    let mut host = DesktopHost::default();
    host.configure_preferences(path.clone(), ctx.clone());
    assert!(matches!(
        host.presentation().preferences,
        PreferenceStatus::Error { .. }
    ));
    command(
        &mut host,
        Command::SetMode(SetModeParams {
            mode: ViewMode::Multitrack,
            expected_presentation_revision: None,
        }),
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "invalid-json",
        "Changing mode must not overwrite a broken preference file"
    );
    layout(
        &mut host,
        SetLayoutParams {
            left_width: Some(310.0),
            right_width: Some(280.0),
            right_visible: Some(false),
            library_fraction: Some(0.42),
            ..Default::default()
        },
    );
    assert_eq!(wait_preferences(&mut host, &ctx), PreferenceStatus::Saved);
    let expected = host.presentation().layout;
    drop(host);
    let mut restored = DesktopHost::default();
    restored.configure_preferences(path, ctx.clone());
    assert_eq!(restored.presentation().layout, expected);
    assert_eq!(restored.presentation().revision, 0);
    assert_eq!(restored.snapshot().revision, 0);
    assert_eq!(restored.presentation().mode, ViewMode::Waveform);
    let blocker = directory.path().join("not-a-directory");
    std::fs::write(&blocker, "blocked").unwrap();
    restored.configure_preferences(blocker.join("layout.json"), ctx.clone());
    layout(
        &mut restored,
        SetLayoutParams {
            left_width: Some(350.0),
            ..Default::default()
        },
    );
    assert!(matches!(
        wait_preferences(&mut restored, &ctx),
        PreferenceStatus::Error { .. }
    ));
    assert_eq!(
        restored.presentation().layout.left.width,
        350.0,
        "A failed preference save must leave the live layout usable"
    );
}
