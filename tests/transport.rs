use serde_json::{Value, json};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use wavebob::{Session, protocol, transport::ManualOutput};

struct Allocator;
thread_local! {static CHECK:Cell<bool>=const{Cell::new(false)};static ALLOCS:Cell<usize>=const{Cell::new(0)};static FREES:Cell<usize>=const{Cell::new(0)};}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        CHECK.with(|check| {
            if check.get() {
                ALLOCS.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CHECK.with(|check| {
            if check.get() {
                FREES.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        CHECK.with(|check| {
            if check.get() {
                ALLOCS.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.realloc(pointer, layout, size) }
    }
}
fn send(session: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"playback-test","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(value: Value) -> Value {
    assert_eq!(value["status"], "ok", "{value}");
    value["result"].clone()
}
fn state(session: &mut Session) -> Value {
    ok(send(session, "transport.get", json!({})))
}
fn fixture(
    rate: u32,
    channels: u16,
    values: &[i16],
) -> (Session, ManualOutput, tempfile::TempDir, String) {
    fixture_at_rate(rate, rate, channels, values)
}
fn fixture_at_rate(
    rate: u32,
    output_rate: u32,
    channels: u16,
    values: &[i16],
) -> (Session, ManualOutput, tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("transport.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for sample in values {
        writer.write_sample(*sample).unwrap();
    }
    writer.finalize().unwrap();
    let (mut session, output) = Session::with_manual_output(output_rate).unwrap();
    let opened = ok(send(&mut session, "documents.open", json!({"path":path})));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let id = loop {
        let job = ok(send(
            &mut session,
            "jobs.get",
            json!({"job_id":opened["id"]}),
        ));
        if job["status"] != "running" {
            assert_eq!(job["status"], "succeeded");
            break job["result"]["id"].as_str().unwrap().to_string();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    (session, output, directory, id)
}
fn mono(values: &[f32]) -> Vec<[f32; 2]> {
    values.iter().map(|value| [*value; 2]).collect()
}

#[test]
fn playback_pause_seek_end_and_restart_distinguish_requested_from_applied() {
    let (mut session, mut output, _directory, id) =
        fixture(1000, 1, &[0, 4096, 8192, 12288, 16384, 20480, 24576, 28672]);
    let before = session.snapshot().documents[0].revision_id.clone();
    let play = ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    assert_eq!(play["requested"]["playing"], true);
    assert_eq!(play["applied"]["playing"], false);
    assert_eq!(play["pending"], true);
    let revision = session.snapshot().revision;
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    assert_eq!(session.snapshot().revision, revision);
    let mut first = [[0.0; 2]; 3];
    output.render_into(&mut first);
    assert_eq!(first.to_vec(), mono(&[0.0, 0.125, 0.25]));
    let applied = state(&mut session);
    assert_eq!(applied["applied"]["frame"], 3);
    assert_eq!(applied["pending"], false);
    let pause = ok(send(&mut session, "transport.pause", json!({})));
    assert_eq!(pause["requested"]["playing"], false);
    assert_eq!(pause["applied"]["playing"], true);
    assert_eq!(pause["pending"], true);
    let mut silence = [[1.0; 2]; 2];
    output.render_into(&mut silence);
    assert_eq!(silence, [[0.0; 2]; 2]);
    assert_eq!(state(&mut session)["applied"]["frame"], 3);
    ok(send(
        &mut session,
        "transport.rewind",
        json!({"document_id":id}),
    ));
    output.render_into(&mut []);
    assert_eq!(state(&mut session)["applied"]["frame"], 0);
    assert_eq!(state(&mut session)["applied"]["playing"], false);
    ok(send(
        &mut session,
        "transport.seek",
        json!({"document_id":id,"frame":4}),
    ));
    output.render_into(&mut []);
    state(&mut session);
    ok(send(
        &mut session,
        "transport.play_pause",
        json!({"document_id":id}),
    ));
    let mut tail = [[0.0; 2]; 6];
    output.render_into(&mut tail);
    assert_eq!(tail.to_vec(), mono(&[0.5, 0.625, 0.75, 0.875, 0.0, 0.0]));
    let ended = state(&mut session);
    assert_eq!(ended["applied"]["frame"], 8);
    assert_eq!(ended["requested"]["playing"], false);
    ok(send(
        &mut session,
        "transport.play_pause",
        json!({"document_id":id}),
    ));
    let mut restart = [[0.0; 2]; 2];
    output.render_into(&mut restart);
    assert_eq!(restart.to_vec(), mono(&[0.0, 0.125]));
    let document = ok(send(
        &mut session,
        "documents.get",
        json!({"document_id":id}),
    ));
    assert_eq!(document["revision_id"], before);
    assert_eq!(document["dirty"], false);
}

#[test]
fn selection_and_whole_loops_have_exact_boundaries_and_one_frame_is_defined() {
    let (mut session, mut output, _directory, id) =
        fixture(1000, 1, &[0, 4096, 8192, 12288, 16384, 20480, 24576, 28672]);
    ok(send(
        &mut session,
        "documents.selection.set",
        json!({"document_id":id,"start_frame":2,"end_frame":5}),
    ));
    ok(send(
        &mut session,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut samples = [[0.0; 2]; 8];
    output.render_into(&mut samples);
    assert_eq!(
        samples.to_vec(),
        mono(&[0.25, 0.375, 0.5, 0.25, 0.375, 0.5, 0.25, 0.375])
    );
    assert_eq!(state(&mut session)["applied"]["frame"], 4);
    ok(send(
        &mut session,
        "transport.rewind",
        json!({"document_id":id}),
    ));
    let mut rewound = [[0.0; 2]; 4];
    output.render_into(&mut rewound);
    assert_eq!(rewound.to_vec(), mono(&[0.25, 0.375, 0.5, 0.25]));
    assert_eq!(
        ok(send(
            &mut session,
            "documents.get",
            json!({"document_id":id})
        ))["selection"],
        json!({"start":2,"end":5})
    );
    ok(send(
        &mut session,
        "transport.loop.set",
        json!({"enabled":false}),
    ));
    let mut end = [[0.0; 2]; 8];
    output.render_into(&mut end);
    assert_eq!(
        end.to_vec(),
        mono(&[0.375, 0.5, 0.625, 0.75, 0.875, 0.0, 0.0, 0.0])
    );
    state(&mut session);
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut all = [[0.0; 2]; 8];
    output.render_into(&mut all);
    assert_eq!(
        all.to_vec(),
        mono(&[0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875])
    );
    state(&mut session);
    ok(send(
        &mut session,
        "documents.selection.set",
        json!({"document_id":id,"start_frame":4,"end_frame":5}),
    ));
    ok(send(
        &mut session,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut one = [[0.0; 2]; 6];
    output.render_into(&mut one);
    assert_eq!(one, [[0.5; 2]; 6]);
    assert_eq!(state(&mut session)["applied"]["frame"], 4);
}

#[test]
fn stereo_routing_and_source_rate_conversion_use_the_same_renderer() {
    let (mut session, mut output, _directory, id) = fixture(1000, 2, &[0, 16384, -16384, 8192]);
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut stereo = [[0.0; 2]; 2];
    output.render_into(&mut stereo);
    assert_eq!(stereo, [[0.0, 0.5], [-0.5, 0.25]]);
    let (mut session, mut output, _directory, id) = fixture_at_rate(2, 4, 1, &[0, 16384]);
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut upsampled = [[0.0; 2]; 4];
    output.render_into(&mut upsampled);
    assert_eq!(upsampled.to_vec(), mono(&[0.0, 0.25, 0.5, 0.5]));
    assert_eq!(state(&mut session)["applied"]["frame"], 2);
}

#[test]
fn bounded_handoffs_converge_to_latest_seek_without_callback_allocation_or_free() {
    let (mut session, mut output, _directory, id) =
        fixture(1000, 1, &[0, 4096, 8192, 12288, 16384, 20480, 24576, 28672]);
    ok(send(
        &mut session,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    for frame in 0..200 {
        ok(send(
            &mut session,
            "transport.seek",
            json!({"document_id":id,"frame":frame%8}),
        ));
    }
    let mut samples = [[0.0; 2]; 1];
    ALLOCS.set(0);
    FREES.set(0);
    CHECK.set(true);
    output.render_into(&mut samples);
    CHECK.set(false);
    assert_eq!(ALLOCS.get(), 0, "Callback allocated");
    assert_eq!(FREES.get(), 0, "Callback destroyed retained buffers");
    let pending = state(&mut session);
    assert_eq!(pending["pending"], true);
    CHECK.set(true);
    output.render_into(&mut samples);
    CHECK.set(false);
    assert_eq!(ALLOCS.get(), 0);
    assert_eq!(FREES.get(), 0);
    let settled = state(&mut session);
    assert_eq!(settled["pending"], false);
    assert_eq!(
        settled["applied"]["generation"],
        settled["requested"]["generation"]
    );
    assert_eq!(samples, [[0.875; 2]]);
}

#[test]
fn mixer_graph_and_control_handoffs_never_allocate_or_free_in_the_callback() {
    let (mut session, mut output, _directory, id) = fixture(48000, 1, &[16384; 8]);
    let doc = ok(send(
        &mut session,
        "documents.get",
        json!({"document_id":id}),
    ));
    let arr = ok(send(&mut session, "tracks.add", json!({})));
    let track = arr["tracks"][0]["id"].clone();
    ok(send(
        &mut session,
        "clips.insert",
        json!({"track_id":track,"document_id":id,"revision_id":doc["revision_id"],"source_start":0,"source_end":8,"position":0}),
    ));
    ok(send(
        &mut session,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    ok(send(&mut session, "mix.play", json!({})));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let mix = ok(send(&mut session, "mix.get", json!({})));
        if mix["prepared_revision_id"] == mix["revision_id"] {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    output.render_into(&mut [[0.0; 2]; 1]);
    state(&mut session);
    ok(send(
        &mut session,
        "tracks.update",
        json!({"track_id":track,"gain_db":12.041199826559248}),
    ));
    loop {
        let mix = ok(send(&mut session, "mix.get", json!({})));
        if mix["prepared_revision_id"] == mix["revision_id"] {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let mut frames = [[0.0; 2]; 1];
    ALLOCS.set(0);
    FREES.set(0);
    CHECK.set(true);
    output.render_into(&mut frames);
    CHECK.set(false);
    assert_eq!(ALLOCS.get(), 0);
    assert_eq!(FREES.get(), 0);
    assert!((frames[0][0] - std::f32::consts::SQRT_2).abs() < 1e-6);
    assert_eq!(
        ok(send(&mut session, "mix.get", json!({})))["overloaded"],
        true
    );
    for frame in 0..200 {
        ok(send(&mut session, "mix.seek", json!({"frame":frame%8})));
    }
    CHECK.set(true);
    output.render_into(&mut frames);
    CHECK.set(false);
    state(&mut session);
    CHECK.set(true);
    output.render_into(&mut frames);
    CHECK.set(false);
    assert_eq!(ALLOCS.get(), 0);
    assert_eq!(FREES.get(), 0);
    assert_eq!(state(&mut session)["pending"], false);
}

#[test]
fn fractional_device_phase_interpolates_across_whole_sample_loop_boundary() {
    let (mut session, mut output, _directory, id) = fixture_at_rate(2, 3, 1, &[0, 24576]);
    ok(send(
        &mut session,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut frames = [[0.0; 2]; 6];
    output.render_into(&mut frames);
    for (actual, expected) in frames.iter().zip([0.0, 0.5, 0.5, 0.0, 0.5, 0.5]) {
        assert!((actual[0] - expected).abs() < 1e-6);
        assert_eq!(actual[0], actual[1]);
    }
}

#[test]
fn device_loss_is_recoverable_and_headless_editing_needs_no_output() {
    let mut headless = Session::default();
    assert_eq!(state(&mut headless)["output"]["status"], "disabled");
    assert_eq!(
        send(
            &mut headless,
            "transport.play",
            json!({"document_id":"missing"})
        )["error"]["code"],
        "missing_capability"
    );
    let (mut session, mut output, _directory, id) = fixture(1000, 1, &[0, 16384, 0, -16384]);
    output.disconnect();
    let failed = state(&mut session);
    assert_eq!(failed["output"]["status"], "failed");
    let before = session.snapshot();
    assert_eq!(
        send(&mut session, "transport.play", json!({"document_id":id}))["error"]["code"],
        "audio_device_error"
    );
    assert_eq!(session.snapshot(), before);
    ok(send(&mut session, "transport.output.retry", json!({})));
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut frames = [[0.0; 2]; 2];
    output.render_into(&mut frames);
    assert_eq!(frames, [[0.0; 2], [0.5; 2]]);
}

#[test]
fn document_changes_pause_and_close_retains_audio_until_safe_handoff() {
    let (mut session, mut output, directory, id) = fixture(1000, 1, &[0, 16384, 8192, -8192]);
    ok(send(
        &mut session,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    output.render_into(&mut [[0.0; 2]; 1]);
    state(&mut session);
    let opened = ok(send(
        &mut session,
        "documents.open",
        json!({"path":directory.path().join("transport.wav")}),
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let other = loop {
        let job = ok(send(
            &mut session,
            "jobs.get",
            json!({"job_id":opened["id"]}),
        ));
        if job["status"] != "running" {
            break job["result"]["id"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!(state(&mut session)["requested"]["playing"], false);
    let mut stopped = [[1.0; 2]; 2];
    output.render_into(&mut stopped);
    assert_eq!(stopped, [[0.0; 2]; 2]);
    state(&mut session);
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":other}),
    ));
    output.render_into(&mut [[0.0; 2]; 1]);
    state(&mut session);
    ok(send(
        &mut session,
        "documents.close",
        json!({"document_id":other}),
    ));
    let mut silence = [[1.0; 2]; 2];
    output.render_into(&mut silence);
    assert_eq!(silence, [[0.0; 2]; 2]);
    assert!(state(&mut session)["applied"]["document_id"].is_null());
}

#[test]
fn pause_preserves_fractional_source_position_and_rewind_is_idempotent() {
    let (mut session, mut output, _directory, id) = fixture_at_rate(2, 4, 1, &[0, 16384]);
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut first = [[1.0; 2]; 1];
    output.render_into(&mut first);
    assert_eq!(first, [[0.0; 2]]);
    state(&mut session);
    ok(send(&mut session, "transport.pause", json!({})));
    output.render_into(&mut [[0.0; 2]; 4]);
    state(&mut session);
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    let mut resumed = [[0.0; 2]; 1];
    output.render_into(&mut resumed);
    assert_eq!(resumed, [[0.25; 2]]);
    let _ = state(&mut session);
    ok(send(&mut session, "transport.pause", json!({})));
    output.render_into(&mut []);
    state(&mut session);
    ok(send(
        &mut session,
        "transport.rewind",
        json!({"document_id":id}),
    ));
    let revision = session.snapshot().revision;
    ok(send(
        &mut session,
        "transport.rewind",
        json!({"document_id":id}),
    ));
    assert_eq!(
        session.snapshot().revision,
        revision,
        "Repeated paused rewind is a no-op"
    );
}

#[test]
fn rational_clock_reaches_exact_end_without_drift() {
    let (mut session, mut output, _directory, id) =
        fixture_at_rate(44100, 48000, 1, &vec![1000; 441]);
    ok(send(
        &mut session,
        "transport.play",
        json!({"document_id":id}),
    ));
    output.render_into(&mut [[0.0; 2]; 480]);
    let state = state(&mut session);
    assert_eq!(state["applied"]["frame"], 441);
    assert_eq!(state["applied"]["playing"], false);
}
