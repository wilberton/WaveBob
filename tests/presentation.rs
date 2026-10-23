#![cfg(feature = "gui")]

use wavebob::{Command, ErrorCode, Outcome, Request, desktop::DesktopHost, presentation::*};

fn command(host: &mut DesktopHost, command: Command) -> wavebob::Response {
    host.execute(Request::new("presentation-test", command))
}
fn layout(host: &mut DesktopHost, params: SetLayoutParams) -> wavebob::Response {
    command(host, Command::SetLayout(params))
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
            left_width: Some(f32::NAN),
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
