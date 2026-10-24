use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn send(s: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        s,
        &json!({"id":"edit-test","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(s: &mut Session, name: &str, params: Value) -> Value {
    let value = send(s, name, params);
    assert_eq!(value["status"], "ok", "{value}");
    value["result"].clone()
}
fn wait(s: &mut Session, id: &Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let job = ok(s, "jobs.get", json!({"job_id":id}));
        if job["status"] != "running" {
            return job;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn fixture(channels: u16, values: &[f32]) -> (Session, tempfile::TempDir, Value) {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("Edit.wav");
    let mut w = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for v in values {
        w.write_sample(*v).unwrap();
    }
    w.finalize().unwrap();
    let mut s = Session::default();
    let job = ok(&mut s, "documents.open", json!({"path":path}));
    let doc = wait(&mut s, &job["id"])["result"].clone();
    (s, d, doc)
}
fn target(doc: &Value, start: u64, end: u64) -> Value {
    json!({"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":start,"end_frame":end})
}
fn samples(s: &mut Session, id: &Value, frames: u64) -> Value {
    ok(
        s,
        "documents.samples",
        json!({"document_id":id,"frames":frames}),
    )["channels"]
        .clone()
}
fn apply(s: &mut Session, target: Value, effect: Value) -> Value {
    let job = ok(s, "effects.apply", json!({"target":target,"effect":effect}));
    let job = wait(s, &job["id"]);
    assert_eq!(job["status"], "succeeded", "{job}");
    job["result"].clone()
}

#[test]
fn effects_preserve_outside_frames_and_history_restores_audio_cursor_and_selection() {
    for (effect, expected) in [
        (
            json!({"kind":"gain","db":6.020599913279624}),
            json!([
                [0.125, 0.5, 1.0, 1.5, 0.875],
                [-0.125, -0.5, -1.0, -1.5, -0.875]
            ]),
        ),
        (
            json!({"kind":"fade_in"}),
            json!([
                [0.125, 0.0, 0.25, 0.75, 0.875],
                [-0.125, 0.0, -0.25, -0.75, -0.875]
            ]),
        ),
        (
            json!({"kind":"fade_out"}),
            json!([
                [0.125, 0.25, 0.25, 0.0, 0.875],
                [-0.125, -0.25, -0.25, 0.0, -0.875]
            ]),
        ),
        (
            json!({"kind":"reverse"}),
            json!([
                [0.125, 0.75, 0.5, 0.25, 0.875],
                [-0.125, -0.75, -0.5, -0.25, -0.875]
            ]),
        ),
    ] {
        let (mut s, _d, doc) = fixture(
            2,
            &[
                0.125, -0.125, 0.25, -0.25, 0.5, -0.5, 0.75, -0.75, 0.875, -0.875,
            ],
        );
        let original = samples(&mut s, &doc["id"], 5);
        ok(
            &mut s,
            "documents.selection.set",
            json!({"document_id":doc["id"],"start_frame":1,"end_frame":4}),
        );
        let edited = apply(&mut s, target(&doc, 1, 4), effect);
        assert_ne!(edited["revision_id"], doc["revision_id"]);
        assert_eq!(edited["dirty"], true);
        assert_eq!(samples(&mut s, &doc["id"], 5), expected);
        assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 1);
        ok(
            &mut s,
            "documents.cursor.set",
            json!({"document_id":doc["id"],"frame":5}),
        );
        ok(&mut s, "history.undo", json!({}));
        let undone = ok(&mut s, "documents.get", json!({"document_id":doc["id"]}));
        assert_eq!(undone["revision_id"], doc["revision_id"]);
        assert_eq!(undone["cursor"], 1);
        assert_eq!(undone["selection"], json!({"start":1,"end":4}));
        assert_eq!(undone["dirty"], false);
        assert_eq!(samples(&mut s, &doc["id"], 5), original);
        ok(&mut s, "history.redo", json!({}));
        assert_eq!(samples(&mut s, &doc["id"], 5), expected);
        assert_eq!(
            ok(&mut s, "documents.get", json!({"document_id":doc["id"]}))["revision_id"],
            edited["revision_id"]
        );
    }
}
#[test]
fn noops_invalid_targets_stale_requests_and_cancelled_jobs_do_not_enter_history() {
    let (mut s, d, doc) = fixture(1, &[0.125, 0.25, 0.5, 0.75]);
    let bytes = std::fs::read(d.path().join("Edit.wav")).unwrap();
    for effect in [
        json!({"kind":"fade_in"}),
        json!({"kind":"fade_out"}),
        json!({"kind":"gain","db":0.0}),
    ] {
        let result = apply(&mut s, target(&doc, 1, 2), effect);
        assert_eq!(result["revision_id"], doc["revision_id"]);
    }
    for (target, effect, code) in [
        (
            target(&doc, 0, 5),
            json!({"kind":"reverse"}),
            "invalid_params",
        ),
        (
            target(&doc, 2, 2),
            json!({"kind":"reverse"}),
            "invalid_params",
        ),
        (
            target(&doc, 0, 4),
            json!({"kind":"gain","db":61.0}),
            "invalid_params",
        ),
    ] {
        assert_eq!(
            send(
                &mut s,
                "effects.apply",
                json!({"target":target,"effect":effect})
            )["error"]["code"],
            code
        );
    }
    let job = ok(
        &mut s,
        "effects.apply",
        json!({"target":target(&doc,0,4),"effect":{"kind":"reverse"}}),
    );
    ok(&mut s, "jobs.cancel", json!({"job_id":job["id"]}));
    assert_eq!(wait(&mut s, &job["id"])["status"], "cancelled");
    assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 0);
    let edited = apply(&mut s, target(&doc, 0, 4), json!({"kind":"reverse"}));
    assert_eq!(
        send(
            &mut s,
            "effects.apply",
            json!({"target":target(&doc,0,4),"effect":{"kind":"reverse"}})
        )["error"]["code"],
        "stale_revision"
    );
    assert_eq!(std::fs::read(d.path().join("Edit.wav")).unwrap(), bytes);
    assert_eq!(
        send(&mut s, "documents.close", json!({"document_id":doc["id"]}))["error"]["code"],
        "dirty_document"
    );
    ok(
        &mut s,
        "documents.close",
        json!({"document_id":edited["id"],"discard_changes":true}),
    );
    assert_eq!(ok(&mut s, "history.get", json!({}))["can_undo"], false);
}
#[test]
fn overloads_are_retained_and_reported_and_new_edits_clear_redo() {
    let (mut s, _d, doc) = fixture(1, &[0.75, -0.75]);
    let edited = apply(
        &mut s,
        target(&doc, 0, 2),
        json!({"kind":"gain","db":6.020599913279624}),
    );
    assert_eq!(edited["peak_level"], 1.5);
    assert_eq!(edited["overloaded_samples"], 2);
    ok(&mut s, "history.undo", json!({}));
    apply(&mut s, target(&doc, 0, 2), json!({"kind":"fade_out"}));
    assert_eq!(ok(&mut s, "history.get", json!({}))["can_redo"], false);
}

#[test]
fn concurrent_results_based_on_a_superseded_revision_are_rejected() {
    let mut values = vec![0.25; 2_000_000];
    values[1] = 0.5;
    let (mut s, _d, doc) = fixture(1, &values);
    let first = ok(
        &mut s,
        "effects.apply",
        json!({"target":target(&doc,0,2_000_000),"effect":{"kind":"gain","db":6.0}}),
    );
    let second = ok(
        &mut s,
        "effects.apply",
        json!({"target":target(&doc,0,2),"effect":{"kind":"reverse"}}),
    );
    let first = wait(&mut s, &first["id"]);
    let second = wait(&mut s, &second["id"]);
    let outcomes = [first, second];
    assert_eq!(
        outcomes
            .iter()
            .filter(|job| job["status"] == "succeeded")
            .count(),
        1,
        "{outcomes:?}"
    );
    assert_eq!(
        outcomes
            .iter()
            .find(|job| job["status"] == "failed")
            .unwrap()["error"]["code"],
        "stale_revision"
    );
    assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 1);
}

#[test]
fn edits_and_undo_swap_active_playback_at_a_safe_handoff() {
    let (_s, d, _doc) = fixture(1, &[0.125, 0.25, 0.5, 0.75]);
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let job = ok(
        &mut s,
        "documents.open",
        json!({"path":d.path().join("Edit.wav")}),
    );
    let doc = wait(&mut s, &job["id"])["result"].clone();
    ok(&mut s, "transport.play", json!({"document_id":doc["id"]}));
    output.render_into(&mut [[0.0; 2]; 2]);
    let edited = apply(
        &mut s,
        target(&doc, 0, 4),
        json!({"kind":"gain","db":6.020599913279624}),
    );
    let requested = ok(&mut s, "transport.get", json!({}));
    assert_eq!(
        requested["requested"]["audio_revision_id"],
        edited["revision_id"]
    );
    assert_eq!(
        requested["applied"]["audio_revision_id"],
        doc["revision_id"]
    );
    let mut pcm = [[0.0; 2]; 1];
    output.render_into(&mut pcm);
    assert_eq!(pcm, [[1.0; 2]]);
    ok(&mut s, "history.undo", json!({}));
    output.render_into(&mut pcm);
    assert_eq!(pcm, [[0.5; 2]]);
}
