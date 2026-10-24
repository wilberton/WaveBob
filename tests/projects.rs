use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wavebob::{Session, protocol};
fn send(session: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"project","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(session: &mut Session, name: &str, params: Value) -> Value {
    let response = send(session, name, params);
    assert_eq!(response["status"], "ok", "{response}");
    response["result"].clone()
}
fn finish(session: &mut Session, started: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let job = ok(session, "jobs.get", json!({"job_id":started["id"]}));
        if job["status"] != "running" {
            return job;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn success(session: &mut Session, started: Value) -> Value {
    let job = finish(session, started);
    assert_eq!(job["status"], "succeeded", "{job}");
    job
}
fn fixture() -> (Session, tempfile::TempDir, Value, Value) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Tone.wav");
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
    for frame in 0..8192 {
        writer
            .write_sample(((frame as f64 * 0.031).sin() * 10000.0) as i16)
            .unwrap();
        writer
            .write_sample(((frame as f64 * 0.043).sin() * 8000.0) as i16)
            .unwrap();
    }
    writer.finalize().unwrap();
    let mut session = Session::default();
    let opened = ok(&mut session, "documents.open", json!({"path":path}));
    let wav = success(&mut session, opened)["result"].clone();
    let path = dir.path().join("音色 texture.mp3");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3"),
        &path,
    )
    .unwrap();
    let opened = ok(&mut session, "documents.open", json!({"path":path}));
    let mp3 = success(&mut session, opened)["result"].clone();
    let arr = ok(
        &mut session,
        "tracks.add",
        json!({"name":"Portable texture"}),
    );
    let track = arr["tracks"][0]["id"].clone();
    for (doc, position) in [(&mp3, 0), (&wav, 4000)] {
        ok(
            &mut session,
            "clips.insert",
            json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":position}),
        );
    }
    let arr = ok(&mut session, "arrangement.get", json!({}));
    let clip = &arr["tracks"][0]["clips"][1];
    let started = ok(
        &mut session,
        "clips.effects.apply",
        json!({"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":{"kind":"reverb","room_size":0.4,"decay_seconds":0.7,"wet":0.2}}),
    );
    success(&mut session, started);
    let edited = ok(
        &mut session,
        "effects.apply",
        json!({"target":{"scope":"whole","document_id":mp3["id"],"revision_id":mp3["revision_id"]},"effect":{"kind":"gain","db":-3.0}}),
    );
    let edited = success(&mut session, edited)["result"].clone();
    ok(
        &mut session,
        "edit.copy",
        json!({"target":{"scope":"range","document_id":edited["id"],"revision_id":edited["revision_id"],"start_frame":100,"end_frame":1000}}),
    );
    ok(
        &mut session,
        "documents.selection.set",
        json!({"document_id":edited["id"],"start_frame":100,"end_frame":1000}),
    );
    ok(
        &mut session,
        "arrangement.navigate",
        json!({"cursor":5000,"selection":{"start":4000,"end":9000},"selected_clip_ids":[clip["id"]]}),
    );
    ok(&mut session, "transport.loop.set", json!({"enabled":true}));
    (session, dir, wav, edited)
}
fn save(session: &mut Session, path: &std::path::Path, overwrite: bool) -> Value {
    let started = ok(
        session,
        "project.save",
        json!({"path":path,"overwrite":overwrite}),
    );
    success(session, started)["output"]["details"].clone()
}
fn mixed(session: &mut Session) -> Value {
    let revision = session.snapshot().arrangement.revision_id;
    let started = ok(session, "mix.prepare", json!({"revision_id":revision}));
    success(session, started);
    ok(
        session,
        "mix.samples",
        json!({"revision_id":revision,"start_frame":5000,"frames":1024}),
    )["channels"]
        .clone()
}
fn resign(value: &mut Value) {
    value["checksum"] = json!(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value["payload"]).unwrap())
    ));
}

#[test]
fn portable_project_roundtrip_preserves_working_pinned_processed_audio_and_no_mp3_generations() {
    let (mut session, dir, wav, mp3) = fixture();
    let path = dir.path().join("Portable session.wavebob");
    let expected = mixed(&mut session);
    let summary = save(&mut session, &path, false);
    assert!(summary["assets"].as_u64().unwrap() >= 4);
    assert_eq!(ok(&mut session, "project.get", json!({}))["dirty"], false);
    let saved = session.snapshot();
    let asset_root = dir.path().join("Portable session.assets");
    let assets = std::fs::read_dir(&asset_root)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            assert_eq!(path.extension().unwrap(), "wav");
            (
                path.file_name().unwrap().to_owned(),
                std::fs::read(path).unwrap(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let repeated = save(&mut session, &path, true);
    assert_eq!(repeated["assets_written"], 0);
    assert_eq!(repeated["assets_reused"], summary["assets"]);
    for (name, bytes) in &assets {
        assert_eq!(std::fs::read(asset_root.join(name)).unwrap(), *bytes);
    }
    std::fs::remove_file(wav["path"].as_str().unwrap()).unwrap();
    std::fs::remove_file(mp3["path"].as_str().unwrap()).unwrap();
    let mut loaded = Session::default();
    let opened = ok(&mut loaded, "project.open", json!({"path":path}));
    success(&mut loaded, opened);
    let actual = loaded.snapshot();
    assert_eq!(actual.documents, saved.documents);
    assert_eq!(actual.arrangement, saved.arrangement);
    assert_eq!(actual.active_document_id, saved.active_document_id);
    assert_eq!(actual.loop_enabled, saved.loop_enabled);
    assert_eq!(actual.project.revision_id, saved.project.revision_id);
    assert!(!actual.project.dirty);
    assert_eq!(mixed(&mut loaded), expected);
    assert_eq!(ok(&mut loaded, "history.get", json!({}))["undo_count"], 0);
    assert_eq!(
        ok(&mut loaded, "edit.clipboard.get", json!({}))["frames"],
        0
    );
    let closed = ok(
        &mut loaded,
        "documents.close",
        json!({"document_id":mp3["id"]}),
    );
    assert_eq!(closed["dirty"], true);
    assert_eq!(ok(&mut loaded, "project.get", json!({}))["dirty"], true);
}

#[test]
fn corrupt_missing_unsupported_or_escaping_projects_leave_the_workspace_intact() {
    let (mut session, dir, _, _) = fixture();
    let path = dir.path().join("Base.wavebob");
    save(&mut session, &path, false);
    let base: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for (name, mut value, expected) in [
        (
            "Version.wavebob",
            base.clone(),
            "unsupported_project_version",
        ),
        ("Checksum.wavebob", base.clone(), "invalid_project"),
        ("Missing.wavebob", base.clone(), "missing_asset"),
        ("Escape.wavebob", base.clone(), "invalid_project"),
    ] {
        match expected {
            "unsupported_project_version" => value["schema_version"] = json!(999),
            "missing_asset" => {
                value["payload"]["assets"][0]["file"] = json!("Base.assets/missing.wav");
                resign(&mut value);
            }
            _ if name == "Escape.wavebob" => {
                value["payload"]["assets"][0]["file"] = json!("../escape.wav");
                resign(&mut value);
            }
            _ => value["payload"]["arrangement"]["master_gain_db"] = json!(4.0),
        }
        let bad = dir.path().join(name);
        std::fs::write(&bad, serde_json::to_vec(&value).unwrap()).unwrap();
        let before = session.snapshot();
        let opened = ok(&mut session, "project.open", json!({"path":bad}));
        let job = finish(&mut session, opened);
        assert_eq!(job["status"], "failed");
        assert_eq!(job["error"]["code"], expected);
        let after = session.snapshot();
        assert_eq!(after.documents, before.documents);
        assert_eq!(after.arrangement, before.arrangement);
        assert_eq!(after.revision, before.revision);
    }
    let asset = dir
        .path()
        .join(base["payload"]["assets"][0]["file"].as_str().unwrap());
    let mut bytes = std::fs::read(&asset).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&asset, bytes).unwrap();
    let before = session.snapshot();
    let opened = ok(&mut session, "project.open", json!({"path":path}));
    assert_eq!(
        finish(&mut session, opened)["error"]["code"],
        "invalid_project"
    );
    assert_eq!(session.snapshot().documents, before.documents);
}

#[test]
fn dirty_policy_saved_baselines_cancellation_and_stale_loads_are_explicit() {
    let (mut session, dir, _, mp3) = fixture();
    let path = dir.path().join("Policy.wavebob");
    assert_eq!(
        send(&mut session, "project.open", json!({"path":path}))["error"]["code"],
        "dirty_document"
    );
    save(&mut session, &path, false);
    ok(&mut session, "history.undo", json!({}));
    assert_eq!(ok(&mut session, "project.get", json!({}))["dirty"], true);
    ok(&mut session, "history.redo", json!({}));
    assert_eq!(ok(&mut session, "project.get", json!({}))["dirty"], false);
    let before = session.snapshot();
    assert_eq!(
        send(
            &mut session,
            "project.save",
            json!({"path":path,"expected_project_revision":"old","overwrite":true})
        )["error"]["code"],
        "stale_revision"
    );
    assert_eq!(session.snapshot(), before);
    let target = dir.path().join("Cancelled.wavebob");
    let started = ok(&mut session, "project.save", json!({"path":target}));
    ok(&mut session, "jobs.cancel", json!({"job_id":started["id"]}));
    drop(session);
    assert!(!target.exists());
    let mut loaded = Session::default();
    let started = ok(&mut loaded, "project.open", json!({"path":path}));
    let added = ok(
        &mut loaded,
        "tracks.add",
        json!({"name":"New work during load"}),
    );
    let result = finish(&mut loaded, started);
    if added["tracks"].as_array().unwrap().len() == 1 {
        assert_eq!(result["status"], "failed");
        assert_eq!(result["error"]["code"], "stale_revision");
        assert!(
            !loaded
                .snapshot()
                .documents
                .iter()
                .any(|doc| doc.id == mp3["id"])
        );
    } else {
        assert_eq!(result["status"], "succeeded");
    }
    assert!(
        loaded
            .snapshot()
            .arrangement
            .tracks
            .iter()
            .any(|track| track.name == "New work during load")
    );
}

#[test]
fn failed_overwrite_and_managed_asset_output_cannot_corrupt_a_saved_project() {
    let (mut session, dir, _, _) = fixture();
    let path = dir.path().join("Protected.wavebob");
    save(&mut session, &path, false);
    let original = std::fs::read(&path).unwrap();
    let started = ok(&mut session, "project.save", json!({"path":path}));
    assert_eq!(
        finish(&mut session, started)["error"]["code"],
        "output_exists"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let manifest: Value = serde_json::from_slice(&original).unwrap();
    let asset = dir
        .path()
        .join(manifest["payload"]["assets"][0]["file"].as_str().unwrap());
    let before = std::fs::read(&asset).unwrap();
    let doc = session.snapshot().documents[0].clone();
    let started = ok(
        &mut session,
        "documents.save",
        json!({"document_id":doc.id,"revision_id":doc.revision_id,"path":asset,"format":{"kind":"wav","encoding":"float32"},"overwrite":true}),
    );
    assert_eq!(
        finish(&mut session, started)["error"]["code"],
        "invalid_params"
    );
    assert_eq!(std::fs::read(asset).unwrap(), before);
    let abandoned = dir.path().join("Abandoned.wavebob");
    ok(&mut session, "project.save", json!({"path":abandoned}));
    drop(session);
    assert!(!abandoned.exists());
    assert!(!dir.path().join("Abandoned.assets").exists());
}
