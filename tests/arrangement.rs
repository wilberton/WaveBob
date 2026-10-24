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
fn clip_job(session: &mut Session, job: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let result = ok(session, "jobs.get", json!({"job_id":job["id"]}));
        if result["status"] != "running" {
            assert_eq!(result["status"], "succeeded", "{result}");
            return result["output"]["details"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn clip_effects_trim_and_history_are_independent_of_shared_source_instances() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let doc = open(&mut session, dir.path());
    let arr = ok(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    let insert = |position| json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":4,"position":position});
    let arr = ok(&mut session, "clips.insert", insert(0));
    let a = arr["tracks"][0]["clips"][0].clone();
    let arr = ok(&mut session, "clips.insert", insert(48_000));
    let b = arr["tracks"][0]["clips"][1].clone();
    let raw = ok(
        &mut session,
        "clips.samples",
        json!({"clip_id":b["id"],"frames":4}),
    )["channels"]
        .clone();
    let started = ok(
        &mut session,
        "clips.pcm.process",
        json!({"clip_id":a["id"],"edit_revision_id":a["edit_revision_id"],"effect":{"kind":"reverse"}}),
    );
    let reversed = clip_job(&mut session, started);
    assert_eq!(reversed["revision_id"], doc["revision_id"]);
    assert_eq!(
        ok(
            &mut session,
            "clips.samples",
            json!({"clip_id":a["id"],"frames":4})
        )["channels"],
        json!([[0.5, 0.375, 0.25, 0.125]])
    );
    assert_eq!(
        ok(
            &mut session,
            "clips.samples",
            json!({"clip_id":b["id"],"frames":4})
        )["channels"],
        raw
    );
    assert_eq!(
        ok(
            &mut session,
            "documents.samples",
            json!({"document_id":doc["id"],"frames":4})
        )["channels"],
        raw
    );
    let started = ok(
        &mut session,
        "clips.trim",
        json!({"clip_id":a["id"],"edit_revision_id":reversed["edit_revision_id"],"source_start":1,"source_end":3}),
    );
    let trimmed = clip_job(&mut session, started);
    assert_eq!(trimmed["position"], 2);
    assert_eq!(trimmed["frames"], 4);
    assert_eq!(
        ok(
            &mut session,
            "clips.samples",
            json!({"clip_id":a["id"],"frames":2})
        )["channels"],
        json!([[0.375, 0.25]])
    );
    ok(&mut session, "history.undo", json!({}));
    assert_eq!(
        session.snapshot().arrangement.tracks[0].clips[0].source_start,
        0
    );
    ok(&mut session, "history.redo", json!({}));
    assert_eq!(
        ok(
            &mut session,
            "clips.samples",
            json!({"clip_id":a["id"],"frames":2})
        )["channels"],
        json!([[0.375, 0.25]])
    );
    let before = session.snapshot();
    for (start, end) in [(0, 0), (3, 2), (0, 5)] {
        assert_eq!(
            dispatch(
                &mut session,
                "clips.trim",
                json!({"clip_id":a["id"],"edit_revision_id":trimmed["edit_revision_id"],"source_start":start,"source_end":end})
            )["status"],
            "error"
        );
        assert_eq!(session.snapshot(), before);
    }
    assert_eq!(
        dispatch(
            &mut session,
            "clips.pcm.process",
            json!({"clip_id":a["id"],"edit_revision_id":a["edit_revision_id"],"effect":{"kind":"fade_in"}})
        )["error"]["code"],
        "stale_revision"
    );
}
#[test]
fn cancelled_clip_processing_cannot_publish_and_unity_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let doc = open(&mut session, dir.path());
    let arr = ok(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    let arr = ok(
        &mut session,
        "clips.insert",
        json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":4,"position":0}),
    );
    let clip = arr["tracks"][0]["clips"][0].clone();
    let history = ok(&mut session, "history.get", json!({}));
    let started = ok(
        &mut session,
        "clips.pcm.process",
        json!({"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":{"kind":"gain","db":0.0}}),
    );
    clip_job(&mut session, started);
    assert_eq!(ok(&mut session, "history.get", json!({})), history);
    let started = ok(
        &mut session,
        "clips.pcm.process",
        json!({"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":{"kind":"reverse"}}),
    );
    // Cancellation dispatch deliberately skips job polling so it wins before publication.
    ok(&mut session, "jobs.cancel", json!({"job_id":started["id"]}));
    assert_eq!(
        session.snapshot().arrangement,
        serde_json::from_value::<wavebob::arrangement::Arrangement>(arr).unwrap()
    );
}

#[test]
fn every_clip_effect_matches_the_shared_waveform_dsp() {
    for effect in [
        json!({"kind":"gain","db":-6.0}),
        json!({"kind":"fade_in"}),
        json!({"kind":"fade_out"}),
        json!({"kind":"reverse"}),
        json!({"kind":"reverb","room_size":0.2,"decay_seconds":0.7,"wet":0.4}),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::default();
        let doc = open(&mut session, dir.path());
        let arr = ok(&mut session, "tracks.add", json!({}));
        let track = arr["tracks"][0]["id"].clone();
        let arr = ok(
            &mut session,
            "clips.insert",
            json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":4,"position":0}),
        );
        let clip = &arr["tracks"][0]["clips"][0];
        let started = ok(
            &mut session,
            "clips.pcm.process",
            json!({"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":effect}),
        );
        clip_job(&mut session, started);
        let started = ok(
            &mut session,
            "edit.process",
            json!({"target":{"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]},"effect":effect}),
        );
        finish(&mut session, started);
        let clip_pcm = ok(
            &mut session,
            "clips.samples",
            json!({"clip_id":clip["id"],"frames":4}),
        )["channels"]
            .clone();
        assert_eq!(
            clip_pcm,
            ok(
                &mut session,
                "documents.samples",
                json!({"document_id":doc["id"],"frames":4})
            )["channels"]
        );
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
        "edit.process",
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
    ok(&mut session, "clips.insert", insert.clone());
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

#[test]
fn duplicates_copy_envelopes_and_stack_values_with_independent_owners_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let document = open(&mut session, dir.path());
    let arrangement = ok(&mut session, "tracks.add", json!({}));
    let track = arrangement["tracks"][0]["id"].clone();
    let arrangement = ok(
        &mut session,
        "clips.insert",
        json!({"track_id":track,"document_id":document["id"],"revision_id":document["revision_id"],"source_start":0,"source_end":4,"position":0}),
    );
    let original = arrangement["tracks"][0]["clips"][0].clone();
    let owner = json!({"kind":"clip","id":original["id"]});
    let edit = |session: &mut Session, owner: &Value, change: Value| {
        let state = ok(session, "processing.get", json!({"owner":owner}));
        ok(
            session,
            "processing.edit",
            json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":change}),
        )
    };
    edit(
        &mut session,
        &owner,
        json!({"action":"set_envelope","envelope":{"gain_db":-12.0,"fade_in_frames":4,"fade_out_frames":3}}),
    );
    let state = edit(
        &mut session,
        &owner,
        json!({"action":"add","kind":"soft_clipper"}),
    );
    let history = ok(&mut session, "history.get", json!({}))["undo_count"]
        .as_u64()
        .unwrap();
    let arrangement = ok(
        &mut session,
        "clips.duplicate",
        json!({"clip_id":original["id"],"position":8}),
    );
    let copy = arrangement["tracks"][0]["clips"][1].clone();
    assert_ne!(copy["id"], original["id"]);
    assert_eq!(copy["revision_id"], original["revision_id"]);
    assert_eq!(copy["document_id"], original["document_id"]);
    assert_eq!(copy["linked"], original["linked"]);
    assert_eq!(
        copy["processing"]["envelope"],
        state["committed"]["envelope"]
    );
    assert_eq!(
        copy["processing"]["effects"][0]["parameters"],
        state["committed"]["effects"][0]["parameters"]
    );
    assert_ne!(
        copy["processing"]["effects"][0]["id"],
        state["committed"]["effects"][0]["id"]
    );
    assert_eq!(
        ok(&mut session, "history.get", json!({}))["undo_count"]
            .as_u64()
            .unwrap(),
        history + 1
    );
    ok(&mut session, "history.undo", json!({}));
    assert_eq!(
        ok(&mut session, "arrangement.get", json!({}))["tracks"][0]["clips"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(&mut session, "history.redo", json!({}));
    let copy_owner = json!({"kind":"clip","id":copy["id"]});
    edit(
        &mut session,
        &copy_owner,
        json!({"action":"set_envelope","envelope":{"gain_db":6.0,"fade_in_frames":0,"fade_out_frames":0}}),
    );
    assert_eq!(
        ok(&mut session, "processing.get", json!({"owner":owner}))["committed"]["envelope"]["gain_db"],
        -12.0
    );
    let current = ok(&mut session, "arrangement.get", json!({}))["tracks"][0]["clips"][0].clone();
    let started = ok(
        &mut session,
        "clips.trim",
        json!({"clip_id":original["id"],"edit_revision_id":current["edit_revision_id"],"source_start":1,"source_end":3}),
    );
    let trimmed = clip_job(&mut session, started);
    assert_eq!(
        trimmed["processing"]["envelope"],
        json!({"gain_db":-12.0,"fade_in_frames":2,"fade_out_frames":2})
    );
    let source = ok(
        &mut session,
        "documents.get",
        json!({"document_id":document["id"]}),
    );
    assert_eq!(source["revision_id"], document["revision_id"]);
    let before = ok(&mut session, "arrangement.get", json!({}));
    assert_eq!(
        dispatch(
            &mut session,
            "clips.duplicate",
            json!({"clip_id":copy["id"],"position":wavebob::arrangement::MAX_TIMELINE_FRAME})
        )["error"]["code"],
        "invalid_params"
    );
    assert_eq!(ok(&mut session, "arrangement.get", json!({})), before);
}
