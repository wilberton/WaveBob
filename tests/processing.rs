use serde_json::{Value, json};
use wavebob::{Session, protocol};

fn send(s: &mut Session, command: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        s,
        &json!({"id":"processing-test","command":command,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(s: &mut Session, command: &str, params: Value) -> Value {
    let r = send(s, command, params);
    assert_eq!(r["status"], "ok", "{r}");
    r["result"].clone()
}
fn fixture() -> (Session, tempfile::TempDir, Value) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for _ in 0..32 {
        writer.write_sample(0.5_f32).unwrap();
    }
    writer.finalize().unwrap();
    let mut s = Session::default();
    let job = ok(&mut s, "documents.open", json!({"path":path}));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let job = ok(&mut s, "jobs.get", json!({"job_id":job["id"]}));
        if job["status"] == "succeeded" {
            return (s, dir, job["result"].clone());
        }
        assert_eq!(job["status"], "running", "{job}");
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn owner(doc: &Value) -> Value {
    json!({"kind":"document","id":doc["id"]})
}
fn envelope(db: f64) -> Value {
    json!({"action":"set_envelope","envelope":{"gain_db":db,"fade_in_frames":4,"fade_out_frames":8}})
}
fn get(s: &mut Session, owner: &Value) -> Value {
    ok(s, "processing.get", json!({"owner":owner}))
}

#[test]
fn preview_coalesces_cancels_validates_and_commits_once_without_replacing_pcm() {
    let (mut s, _dir, doc) = fixture();
    let owner = owner(&doc);
    let before = get(&mut s, &owner);
    let revision = s.snapshot().revision;
    let begin = ok(
        &mut s,
        "processing.preview.begin",
        json!({"owner":owner,"expected_edit_revision":before["committed"]["edit_revision_id"]}),
    );
    for db in [-1.0, -2.0, -3.0] {
        ok(
            &mut s,
            "processing.preview.update",
            json!({"preview_id":begin["preview_id"],"change":envelope(db)}),
        );
    }
    assert_eq!(s.snapshot().revision, revision);
    let invalid = send(
        &mut s,
        "processing.preview.update",
        json!({"preview_id":begin["preview_id"],"change":envelope(100.0)}),
    );
    assert_eq!(invalid["error"]["code"], "invalid_params");
    let stale = send(
        &mut s,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":"stale","change":envelope(0.0)}),
    );
    assert_eq!(stale["error"]["code"], "stale_revision");
    assert_eq!(get(&mut s, &owner)["preview"]["envelope"]["gain_db"], -3.0);
    ok(
        &mut s,
        "processing.preview.cancel",
        json!({"preview_id":begin["preview_id"]}),
    );
    assert_eq!(get(&mut s, &owner), before);
    let begin = ok(
        &mut s,
        "processing.preview.begin",
        json!({"owner":owner,"expected_edit_revision":before["committed"]["edit_revision_id"]}),
    );
    ok(
        &mut s,
        "processing.preview.update",
        json!({"preview_id":begin["preview_id"],"change":envelope(-6.0)}),
    );
    let committed = ok(
        &mut s,
        "processing.preview.commit",
        json!({"preview_id":begin["preview_id"]}),
    );
    assert_ne!(before["render_identity"], committed["render_identity"]);
    assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 1);
    let current = ok(&mut s, "documents.get", json!({"document_id":doc["id"]}));
    assert_eq!(current["revision_id"], doc["revision_id"]);
    assert_eq!(current["dirty"], true);
    assert_eq!(
        ok(
            &mut s,
            "documents.samples",
            json!({"document_id":doc["id"],"frames":1})
        )["channels"][0][0],
        0.5
    );
    ok(&mut s, "history.undo", json!({}));
    assert_eq!(
        get(&mut s, &owner)["render_identity"],
        before["render_identity"]
    );
    assert_eq!(
        ok(&mut s, "documents.get", json!({"document_id":doc["id"]}))["dirty"],
        false
    );
    ok(&mut s, "history.redo", json!({}));
    assert_eq!(
        get(&mut s, &owner)["render_identity"],
        committed["render_identity"]
    );
    let stale = send(
        &mut s,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":before["committed"]["edit_revision_id"],"change":envelope(0.0)}),
    );
    assert_eq!(stale["error"]["code"], "stale_revision");
}

#[test]
fn registry_is_explicit_and_navigation_does_not_change_render_identity() {
    let (mut s, _dir, doc) = fixture();
    let catalogue = ok(&mut s, "processing.catalogue", json!({}));
    assert_eq!(catalogue.as_array().unwrap().len(), 11);
    let owner = owner(&doc);
    let before = get(&mut s, &owner);
    for info in catalogue
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["available"] == false)
    {
        let result = send(
            &mut s,
            "processing.edit",
            json!({"owner":owner,"expected_edit_revision":before["committed"]["edit_revision_id"],"change":{"action":"add","kind":info["kind"]}}),
        );
        assert_eq!(result["error"]["code"], "missing_capability");
    }
    ok(
        &mut s,
        "documents.cursor.set",
        json!({"document_id":doc["id"],"frame":8}),
    );
    assert_eq!(
        get(&mut s, &owner)["render_identity"],
        before["render_identity"]
    );
    let begin = ok(
        &mut s,
        "processing.preview.begin",
        json!({"owner":owner,"expected_edit_revision":before["committed"]["edit_revision_id"]}),
    );
    ok(
        &mut s,
        "processing.preview.commit",
        json!({"preview_id":begin["preview_id"]}),
    );
    assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 0);
    ok(&mut s, "processing.tempo.set", json!({"bpm":90}));
    assert_ne!(
        get(&mut s, &owner)["render_identity"],
        before["render_identity"]
    );
    ok(&mut s, "history.undo", json!({}));
    assert_eq!(
        get(&mut s, &owner)["render_identity"],
        before["render_identity"]
    );
}

#[test]
fn linked_sources_survive_tab_close_and_clip_processing_is_isolated() {
    let (mut s, _dir, doc) = fixture();
    let arrangement = ok(&mut s, "tracks.add", json!({"name":"linked"}));
    let track = arrangement["tracks"][0]["id"].clone();
    for position in [0, 64] {
        ok(
            &mut s,
            "clips.insert",
            json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":32,"position":position}),
        );
    }
    let arrangement = ok(&mut s, "arrangement.get", json!({}));
    let clip = &arrangement["tracks"][0]["clips"][0];
    assert_eq!(clip["linked"], true);
    assert_eq!(clip["full_document"], true);
    let clip_owner = json!({"kind":"clip","id":clip["id"]});
    let clip_state = get(&mut s, &clip_owner);
    ok(
        &mut s,
        "processing.edit",
        json!({"owner":clip_owner,"expected_edit_revision":clip_state["committed"]["edit_revision_id"],"change":envelope(-12.0)}),
    );
    assert_eq!(
        get(&mut s, &owner(&doc))["committed"]["envelope"]["gain_db"],
        0.0
    );
    ok(&mut s, "documents.close", json!({"document_id":doc["id"]}));
    assert!(s.snapshot().documents.is_empty());
    assert_eq!(get(&mut s, &owner(&doc))["body_frames"], 32);
    ok(
        &mut s,
        "documents.activate",
        json!({"document_id":doc["id"]}),
    );
    assert_eq!(s.snapshot().documents.len(), 1);
}

#[test]
fn replaced_and_concurrently_cancelled_previews_cannot_publish() {
    let (mut s, _dir, doc) = fixture();
    let owner = owner(&doc);
    let before = get(&mut s, &owner);
    let begin = |s: &mut Session| {
        ok(
            s,
            "processing.preview.begin",
            json!({"owner":owner,"expected_edit_revision":before["committed"]["edit_revision_id"]}),
        )
    };
    let first = begin(&mut s);
    ok(
        &mut s,
        "processing.preview.update",
        json!({"preview_id":first["preview_id"],"change":envelope(-6.0)}),
    );
    let second = begin(&mut s);
    assert_ne!(first["preview_id"], second["preview_id"]);
    assert_eq!(
        send(
            &mut s,
            "processing.preview.commit",
            json!({"preview_id":first["preview_id"]})
        )["error"]["code"],
        "not_found"
    );
    ok(
        &mut s,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":before["committed"]["edit_revision_id"],"change":envelope(-12.0)}),
    );
    assert_eq!(
        send(
            &mut s,
            "processing.preview.commit",
            json!({"preview_id":second["preview_id"]})
        )["error"]["code"],
        "not_found"
    );
    assert_eq!(
        get(&mut s, &owner)["committed"]["envelope"]["gain_db"],
        -12.0
    );
    assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 1);
    assert_eq!(
        send(&mut s, "effects.apply", json!({}))["error"]["code"],
        "unknown_command"
    );
}
