use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn dispatch(session: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"arrange","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(session: &mut Session, name: &str, params: Value) -> Value {
    let response = dispatch(session, name, params);
    assert_eq!(response["status"], "ok", "{response}");
    response["result"].clone()
}
fn open(session: &mut Session, dir: &std::path::Path) -> Value {
    let path = dir.join("Pinned.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 24_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for value in [0.125, 0.25, 0.375, 0.5] {
        writer.write_sample(value).unwrap();
    }
    writer.finalize().unwrap();
    let started = ok(session, "documents.open", json!({"path":path}));
    finish(session, started)
}
fn finish(session: &mut Session, job: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let result = ok(session, "jobs.get", json!({"job_id":job["id"]}));
        if result["status"] != "running" {
            assert_eq!(result["status"], "succeeded", "{result}");
            return result["result"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn pinned_audio_survives_edits_close_track_removal_and_chronological_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let doc = open(&mut session, dir.path());
    let arr = ok(&mut session, "tracks.add", json!({"name":"Ambience"}));
    let track = arr["tracks"][0]["id"].clone();
    let arr = ok(
        &mut session,
        "clips.insert",
        json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":1,"source_end":4,"position":48_000}),
    );
    let clip = arr["tracks"][0]["clips"][0].clone();
    assert_eq!(clip["frames"], 6);
    let job = ok(
        &mut session,
        "effects.apply",
        json!({"target":{"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]},"effect":{"kind":"reverse"}}),
    );
    finish(&mut session, job);
    let peaks = ok(
        &mut session,
        "clips.waveform",
        json!({"clip_id":clip["id"],"bins":3}),
    );
    assert_eq!(peaks["channels"][0][0]["min"], 0.25);
    ok(&mut session, "history.undo", json!({}));
    assert_eq!(
        ok(
            &mut session,
            "documents.get",
            json!({"document_id":doc["id"]})
        )["revision_id"],
        doc["revision_id"]
    );
    ok(
        &mut session,
        "documents.close",
        json!({"document_id":doc["id"]}),
    );
    assert_eq!(
        ok(
            &mut session,
            "clips.waveform",
            json!({"clip_id":clip["id"],"bins":3})
        ),
        peaks
    );
    ok(&mut session, "tracks.remove", json!({"track_id":track}));
    assert!(session.snapshot().arrangement.tracks.is_empty());
    ok(&mut session, "history.undo", json!({}));
    assert_eq!(
        ok(
            &mut session,
            "clips.waveform",
            json!({"clip_id":clip["id"],"bins":3})
        ),
        peaks
    );
    ok(&mut session, "history.undo", json!({}));
    assert!(session.snapshot().arrangement.tracks[0].clips.is_empty());
    ok(&mut session, "history.redo", json!({}));
    assert_eq!(
        session.snapshot().arrangement.tracks[0].clips[0].revision_id,
        doc["revision_id"]
    );
}
#[test]
fn invalid_placement_is_atomic_and_navigation_stays_out_of_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let doc = open(&mut session, dir.path());
    let arr = ok(&mut session, "tracks.add", json!({}));
    let first = arr["tracks"][0]["id"].clone();
    let arr = ok(
        &mut session,
        "tracks.add",
        json!({"name":"Second","index":0}),
    );
    let second = arr["tracks"][0]["id"].clone();
    let insert = json!({"track_id":first,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":4,"position":0});
    let arr = ok(&mut session, "clips.insert", insert.clone());
    let clip = arr["tracks"][1]["clips"][0]["id"].clone();
    let before = session.snapshot();
    let history = ok(&mut session, "history.get", json!({}));
    for params in [
        insert,
        json!({"track_id":first,"document_id":doc["id"],"revision_id":"stale","source_start":0,"source_end":4,"position":48_000}),
        json!({"track_id":first,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":4,"position":u64::MAX}),
    ] {
        assert_eq!(
            dispatch(&mut session, "clips.insert", params)["status"],
            "error"
        );
        assert_eq!(session.snapshot(), before);
    }
    ok(
        &mut session,
        "arrangement.navigate",
        json!({"cursor":12,"selection":{"start":10,"end":20},"selected_clip_ids":[clip]}),
    );
    assert_eq!(ok(&mut session, "history.get", json!({})), history);
    let arr = ok(
        &mut session,
        "clips.move",
        json!({"clip_id":clip,"track_id":second,"position":48_000}),
    );
    assert_eq!(arr["tracks"][0]["clips"][0]["position"], 48_000);
    ok(&mut session, "history.undo", json!({}));
    assert_eq!(
        session.snapshot().arrangement.tracks[1].clips[0].position,
        0
    );
    let before = session.snapshot();
    let request = json!({"id":"stale","command":"clips.move","params":{"clip_id":clip,"track_id":second,"position":12},"expected_revision":0});
    assert_eq!(
        serde_json::to_value(protocol::dispatch_line(&mut session, &request.to_string())).unwrap()
            ["error"]["code"],
        "stale_revision"
    );
    assert_eq!(session.snapshot(), before);
}
