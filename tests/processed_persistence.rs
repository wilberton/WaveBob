use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn call(session: &mut Session, name: &str, params: Value) -> Value {
    let response = serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"utility","command":name,"params":params}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    response["result"].clone()
}
fn wait(session: &mut Session, started: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let job = call(session, "jobs.get", json!({"job_id":started["id"]}));
        if job["status"] != "running" {
            assert_eq!(job["status"], "succeeded", "{job}");
            return job;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn owner(document: &Value) -> Value {
    json!({"kind":"document","id":document["id"]})
}
fn edit(session: &mut Session, owner: &Value, change: Value) -> Value {
    let state = call(session, "processing.get", json!({"owner":owner}));
    call(
        session,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":change}),
    )
}
fn prepare(s: &mut Session) -> Value {
    let rev = s.snapshot().arrangement.revision_id;
    let job = call(s, "mix.prepare", json!({"revision_id":rev}));
    wait(s, job)
}
fn source(s: &mut Session, path: &std::path::Path, rate: u32, channels: u16, frames: u64) -> Value {
    let mut w = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for f in 0..frames {
        for c in 0..channels {
            w.write_sample((f as f64 * (0.071 + c as f64 * 0.013)).sin() as f32 * 0.15)
                .unwrap();
        }
    }
    w.finalize().unwrap();
    let j = call(s, "documents.open", json!({"path":path}));
    wait(s, j)["result"].clone()
}
fn mix_export(s: &mut Session, path: &std::path::Path) -> Value {
    let a = s.snapshot().arrangement;
    let j = call(
        s,
        "mix.export",
        json!({"revision_id":a.revision_id,"start_frame":0,"end_frame":a.frames(),"include_tail":true,"path":path,"format":{"kind":"wav","encoding":"float32"}}),
    );
    wait(s, j)["output"]["details"].clone()
}
fn pcm(path: &std::path::Path) -> Vec<f32> {
    hound::WavReader::open(path)
        .unwrap()
        .samples::<f32>()
        .map(Result::unwrap)
        .collect()
}
fn raw(s: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        s,
        &json!({"id":"raw","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn finish(s: &mut Session, j: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        assert!(std::time::Instant::now() < deadline, "job timeout: {j}");
        let state = call(s, "jobs.get", json!({"job_id":j["id"]}));
        if state["status"] != "running" {
            return state;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn all_eleven_and_combined_stacks_roundtrip_lossless_linked_pinned_without_sources_or_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let catalogue = call(&mut s, "processing.catalogue", json!({}));
    let catalogue = catalogue.as_array().unwrap();
    assert_eq!(catalogue.len(), 11);
    let mut originals = Vec::new();
    for (i, info) in catalogue.iter().enumerate() {
        let path = dir.path().join(format!("source-{i}.wav"));
        let d = source(&mut s, &path, 11025, if i % 2 == 0 { 1 } else { 2 }, 512);
        originals.push(path);
        let o = owner(&d);
        edit(&mut s, &o, json!({"action":"add","kind":info["kind"]}));
        edit(
            &mut s,
            &o,
            json!({"action":"set_envelope","envelope":{"gain_db":-3.0,"fade_in_frames":70,"fade_out_frames":60}}),
        );
        let a = call(&mut s, "tracks.add", json!({"name":format!("Effect {i}")}));
        let track = a["tracks"].as_array().unwrap().last().unwrap()["id"].clone();
        call(
            &mut s,
            "clips.insert",
            json!({"track_id":track,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":0,"source_end":512,"position":i as u64*3000}),
        );
    }
    // Persist a clamped trim and original envelope intent, with source-relative anchors.
    let trim_doc = serde_json::to_value(&s.snapshot().documents[1]).unwrap();
    let track = s.snapshot().arrangement.tracks[1].id.clone();
    call(
        &mut s,
        "clips.insert",
        json!({"track_id":track,"document_id":trim_doc["id"],"revision_id":trim_doc["revision_id"],"source_start":100,"source_end":450,"position":70000}),
    );
    let trim_clip = s.snapshot().arrangement.tracks[1].clips[1].id.clone();
    edit(
        &mut s,
        &json!({"kind":"clip","id":trim_clip}),
        json!({"action":"set_envelope","envelope":{"gain_db":-1.0,"fade_in_frames":300,"fade_out_frames":0}}),
    );
    let j = call(
        &mut s,
        "edit.delete",
        json!({"target":{"scope":"range","document_id":trim_doc["id"],"revision_id":trim_doc["revision_id"],"start_frame":350,"end_frame":512}}),
    );
    wait(&mut s, j);
    // A combined chain and independent instance use the same shared renderer.
    let d = serde_json::to_value(&s.snapshot().documents[0]).unwrap();
    let o = owner(&d);
    for kind in ["parametric_eq", "balance", "limiter"] {
        edit(&mut s, &o, json!({"action":"add","kind":kind}));
    }
    prepare(&mut s);
    let clip = s.snapshot().arrangement.tracks[0].clips[0].id.clone();
    let co = json!({"kind":"clip","id":clip});
    call(&mut s, "processing.make_independent", json!({"owner":co}));
    edit(
        &mut s,
        &co,
        json!({"action":"add","kind":"stereo_enhancer"}),
    );
    let before_path = dir.path().join("before.wav");
    let before = mix_export(&mut s, &before_path);
    let project = dir.path().join("all.wavebob");
    let j = call(&mut s, "project.save", json!({"path":project}));
    wait(&mut s, j);
    let manifest: Value = serde_json::from_slice(&std::fs::read(&project).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 2);
    assert!(
        manifest["payload"]["processing_policy"]
            .as_str()
            .unwrap()
            .contains("60s")
    );
    let saved = s.snapshot();
    assert!(!saved.project.dirty);
    for path in originals {
        std::fs::remove_file(path).unwrap();
    }
    drop(s); // Removes the in-memory bounce cache and every original Audio.
    let mut s = Session::default();
    let j = call(&mut s, "project.open", json!({"path":project}));
    wait(&mut s, j);
    assert!(!s.snapshot().project.dirty);
    assert_eq!(s.snapshot().documents, saved.documents);
    let after_path = dir.path().join("after.wav");
    let after = mix_export(&mut s, &after_path);
    assert_eq!(before["frames"], after["frames"]);
    assert_eq!(pcm(&before_path), pcm(&after_path));
    prepare(&mut s);
    assert!(
        !s.snapshot().project.dirty,
        "derived preparation must not dirty a saved project"
    );
    assert!(!s.snapshot().arrangement.tracks[0].clips[0].linked);
    let j = call(
        &mut s,
        "project.save",
        json!({"path":project,"overwrite":true}),
    );
    let summary = wait(&mut s, j);
    assert_eq!(summary["output"]["details"]["assets_written"], 0);
}
#[test]
fn processed_save_range_tail_channels_tempo_and_dirty_baselines_preserve_source_pcm() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = source(&mut s, &dir.path().join("source.wav"), 48000, 1, 1024);
    let o = owner(&d);
    let original = call(
        &mut s,
        "documents.samples",
        json!({"document_id":d["id"],"start_frame":0,"frames":1024}),
    );
    let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_enhancer"}));
    let effect = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &o,
        json!({"action":"set_parameter","effect_id":effect,"parameter_id":"phase","value":128}),
    );
    edit(
        &mut s,
        &o,
        json!({"action":"set_envelope","envelope":{"gain_db":-6.0,"fade_in_frames":300,"fade_out_frames":70}}),
    );
    let output = dir.path().join("wet.wav");
    let j = call(
        &mut s,
        "documents.save",
        json!({"document_id":d["id"],"revision_id":d["revision_id"],"path":output,"format":{"kind":"wav","encoding":"float32"}}),
    );
    let save = wait(&mut s, j);
    let summary = &save["output"]["details"];
    assert_eq!(summary["channels"], 2);
    assert!(summary["frames"].as_u64().unwrap() > 1024);
    assert!(!s.snapshot().documents[0].dirty);
    assert!(s.snapshot().documents[0].saved_render_identity.is_some());
    let j = call(&mut s, "processing.render", json!({"owner":o}));
    let r = wait(&mut s, j)["output"]["details"].clone();
    let count = r["source_body_frames"].as_u64().unwrap() + r["tail_frames"].as_u64().unwrap();
    let mut expected = [Vec::new(), Vec::new()];
    for start in (0..count).step_by(4096) {
        let values = call(
            &mut s,
            "processing.samples",
            json!({"audio_revision_id":r["audio_revision_id"],"start_frame":start,"frames":(count-start).min(4096)}),
        );
        for c in 0..2 {
            expected[c].extend(
                values["channels"][c]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap()),
            );
        }
    }
    let actual = pcm(&output);
    for (f, frame) in actual.chunks(2).enumerate() {
        for c in 0..2 {
            assert!((frame[c] as f64 - expected[c][f]).abs() < 1e-7);
        }
    }
    call(&mut s, "processing.tempo.set", json!({"bpm":90.0}));
    assert!(s.snapshot().documents[0].dirty);
    call(&mut s, "history.undo", json!({}));
    assert!(!s.snapshot().documents[0].dirty);
    let range = dir.path().join("range.wav");
    let j = call(
        &mut s,
        "audio.export",
        json!({"target":{"scope":"range","document_id":d["id"],"revision_id":d["revision_id"],"start_frame":400,"end_frame":700},"path":range,"channels":1,"sample_rate":24000,"format":{"kind":"wav","encoding":"float32"}}),
    );
    let r = wait(&mut s, j);
    let r = &r["output"]["details"];
    assert_eq!(r["channels"], 1);
    assert_eq!(r["sample_rate"], 24000);
    assert_eq!(r["source_start_frame"], 400);
    assert_eq!(r["source_end_frame"], 700);
    assert!(r["render_metadata"]["tail_frames"].as_u64().unwrap() > 0);
    let j = call(
        &mut s,
        "documents.save",
        json!({"document_id":d["id"],"revision_id":d["revision_id"],"path":dir.path().join("wet.mp3"),"channels":1,"format":{"kind":"mp3","bitrate_kbps":192}}),
    );
    wait(&mut s, j);
    assert_eq!(
        s.snapshot().documents[0].revision_id,
        d["revision_id"].as_str().unwrap()
    );
    assert_eq!(
        original,
        call(
            &mut s,
            "documents.samples",
            json!({"document_id":d["id"],"start_frame":0,"frames":1024})
        )
    );
    assert!(!s.snapshot().documents[0].dirty);
    edit(&mut s, &o, json!({"action":"bypass_all","bypassed":true}));
    assert!(s.snapshot().documents[0].dirty);
    call(&mut s, "history.undo", json!({}));
    assert!(!s.snapshot().documents[0].dirty);
}
#[test]
fn parameter_changes_reject_sample_mix_and_project_publication_and_cancel_cleans_temps() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = source(&mut s, &dir.path().join("source.wav"), 48000, 2, 150000);
    let o = owner(&d);
    edit(&mut s, &o, json!({"action":"add","kind":"balance"}));
    let a = call(&mut s, "tracks.add", json!({}));
    call(
        &mut s,
        "clips.insert",
        json!({"track_id":a["tracks"][0]["id"],"document_id":d["id"],"revision_id":d["revision_id"],"source_start":0,"source_end":150000,"position":0}),
    );
    for (name, file) in [
        ("documents.save", "stale.wav"),
        ("mix.export", "stale-mix.wav"),
        ("project.save", "stale.wavebob"),
    ] {
        let path = dir.path().join(file);
        let a = s.snapshot().arrangement;
        let params = match name {
            "project.save" => json!({"path":path}),
            "mix.export" => {
                json!({"revision_id":a.revision_id,"start_frame":0,"end_frame":150000,"path":path})
            }
            _ => {
                json!({"document_id":d["id"],"revision_id":d["revision_id"],"path":path,"format":{"kind":"wav","encoding":"float32"}})
            }
        };
        let j = call(&mut s, name, params);
        let gain = if file == "stale.wav" {
            -1.0
        } else if file == "stale-mix.wav" {
            -2.0
        } else {
            -3.0
        };
        edit(
            &mut s,
            &o,
            json!({"action":"set_envelope","envelope":{"gain_db":gain,"fade_in_frames":0,"fade_out_frames":0}}),
        );
        let result = finish(&mut s, j);
        assert_eq!(result["status"], "failed", "{result}");
        assert_eq!(result["error"]["code"], "stale_revision");
        assert!(!path.exists());
    }
    let path = dir.path().join("cancel.wav");
    let j = call(
        &mut s,
        "documents.save",
        json!({"document_id":d["id"],"revision_id":d["revision_id"],"path":path,"format":{"kind":"wav","encoding":"float32"}}),
    );
    call(&mut s, "jobs.cancel", json!({"job_id":j["id"]}));
    let result = finish(&mut s, j);
    assert_eq!(result["status"], "cancelled");
    assert!(!path.exists());
}

#[test]
fn unknown_schemas_effects_policies_and_corrupt_bounces_never_replace_workspace() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = source(&mut s, &dir.path().join("source.wav"), 48000, 1, 128);
    let o = owner(&d);
    edit(&mut s, &o, json!({"action":"add","kind":"balance"}));
    let a = call(&mut s, "tracks.add", json!({}));
    call(
        &mut s,
        "clips.insert",
        json!({"track_id":a["tracks"][0]["id"],"document_id":d["id"],"revision_id":d["revision_id"],"source_start":0,"source_end":128,"position":0}),
    );
    let path = dir.path().join("valid.wavebob");
    let j = call(&mut s, "project.save", json!({"path":path}));
    wait(&mut s, j);
    let valid: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let original = s.snapshot().documents;
    for case in 0..6 {
        let mut bad = valid.clone();
        match case {
            0 => bad["schema_version"] = json!(1),
            1 => bad["schema_version"] = json!(3),
            2 => {
                bad["payload"]["documents"][0]["processing"]["effects"][0]["kind"] =
                    json!("unknown_effect")
            }
            3 => bad["payload"]["processing_policy"] = json!("other"),
            4 => {
                bad["payload"]["arrangement"]["tracks"][0]["clips"][0]["bounce"]["metadata"]["tail_frames"] =
                    json!(999)
            }
            _ => bad["payload"]["assets"][0]["checksum"] = json!("bad"),
        }
        bad["checksum"] = json!(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&bad["payload"]).unwrap())
        ));
        let path = dir.path().join(format!("invalid-{case}.wavebob"));
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        let j = call(
            &mut s,
            "project.open",
            json!({"path":path,"discard_changes":true}),
        );
        let r = finish(&mut s, j);
        assert_eq!(r["status"], "failed", "{r}");
        assert_eq!(s.snapshot().documents, original);
    }
    let invalid = raw(
        &mut s,
        "audio.export",
        json!({"target":{"scope":"whole","document_id":d["id"],"revision_id":d["revision_id"]},"path":dir.path().join("invalid.wav"),"channels":3}),
    );
    assert_eq!(invalid["status"], "ok");
    // Invalid conversion is a worker error and cannot publish a destination.
    let r = finish(&mut s, invalid["result"].clone());
    assert_eq!(r["status"], "failed");
    assert!(!dir.path().join("invalid.wav").exists());
}
#[test]
#[ignore = "requires independent FFmpeg in WAVEBOB_QA_FFMPEG"]
fn independent_decoder_matches_processed_mp3_all_offered_settings() {
    let ffmpeg = std::env::var_os("WAVEBOB_QA_FFMPEG").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = source(&mut s, &dir.path().join("source.wav"), 44100, 2, 12000);
    let o = owner(&d);
    let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_enhancer"}));
    let e = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &o,
        json!({"action":"set_parameter","effect_id":e,"parameter_id":"phase","value":128}),
    );
    edit(
        &mut s,
        &o,
        json!({"action":"set_envelope","envelope":{"gain_db":-3.0,"fade_in_frames":50,"fade_out_frames":100}}),
    );
    let mut records = Vec::new();
    for channels in [1, 2] {
        for rate in [32000, 44100, 48000] {
            let wav = dir.path().join(format!("expected-{channels}-{rate}.wav"));
            let target = json!({"scope":"range","document_id":d["id"],"revision_id":d["revision_id"],"start_frame":500,"end_frame":11000});
            let j = call(
                &mut s,
                "audio.export",
                json!({"target":target,"path":wav,"sample_rate":rate,"channels":channels,"format":{"kind":"wav","encoding":"float32"}}),
            );
            wait(&mut s, j);
            let expected = pcm(&wav);
            for bitrate in [128, 192, 256, 320] {
                let path = dir
                    .path()
                    .join(format!("actual-{channels}-{rate}-{bitrate}.mp3"));
                let j = call(
                    &mut s,
                    "audio.export",
                    json!({"target":target,"path":path,"sample_rate":rate,"channels":channels,"format":{"kind":"mp3","bitrate_kbps":bitrate}}),
                );
                let r = wait(&mut s, j);
                let mut command = std::process::Command::new(&ffmpeg);
                command.args(["-v", "error", "-i"]).arg(path).args([
                    "-f",
                    "f32le",
                    "-acodec",
                    "pcm_f32le",
                    "pipe:1",
                ]);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let out = command.output().unwrap();
                assert!(out.status.success());
                let actual = out
                    .stdout
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| f32::from_le_bytes(*b))
                    .collect::<Vec<_>>();
                assert_eq!(expected.len(), actual.len());
                let mut square = 0.0;
                let mut max = 0.0f64;
                for (a, b) in actual.iter().zip(&expected) {
                    let e = (*a as f64 - *b as f64).abs();
                    square += e * e;
                    max = max.max(e);
                }
                let rms = (square / expected.len() as f64).sqrt();
                assert!(
                    rms < 0.02 && max < 0.07,
                    "{channels}/{rate}/{bitrate}: {rms}/{max}"
                );
                records.push(json!({"channels":channels,"rate":rate,"bitrate":bitrate,"rmse":rms,"max_error":max,"summary":r["output"]["details"]}));
            }
        }
    }
    if let Some(root) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            std::path::Path::new(&root).join("processed-mp3.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
    }
}
