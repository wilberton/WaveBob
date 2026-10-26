use serde_json::{Value, json};
use wavebob::{Session, protocol, transport::ManualOutput};
fn call(session: &mut Session, name: &str, params: Value) -> Value {
    let response = serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"clipper","command":name,"params":params}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    response["result"].clone()
}
fn wait(session: &mut Session, started: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
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
fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/processing/soft-clipper")
}
fn pcm(path: &std::path::Path) -> Vec<f32> {
    std::fs::read(path)
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
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
fn ready(session: &mut Session, output: &mut ManualOutput) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        output.render_into(&mut []);
        let state = call(session, "transport.get", json!({}));
        assert!(
            state["processing"]["preparation_error"].is_null(),
            "{state}"
        );
        if state["requested"]["generation"] == state["applied"]["generation"] {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{state}");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[test]
fn pinned_reference_sweeps_match_the_wavebob_transfer_without_alignment_or_gain_correction() {
    let root = fixture_path();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["maximum_absolute_error"], 2e-7);
    let mut maximum = 0.0f32;
    for case in manifest["cases"].as_array().unwrap() {
        let input = pcm(&root.join(case["input"].as_str().unwrap()));
        let expected = pcm(&root.join(case["output"].as_str().unwrap()));
        for (input, expected) in input.into_iter().zip(expected) {
            let actual = wavebob::dsp::soft_clipper::curve(
                input,
                case["threshold"].as_i64().unwrap() as i32,
                case["post"].as_i64().unwrap() as i32,
            );
            maximum = maximum.max((actual - expected).abs());
            assert!(actual.is_finite());
            assert!(
                (actual - expected).abs() <= 2e-7,
                "{case}: {input}: {actual} != {expected}"
            );
        }
    }
    eprintln!("Soft Clipper pinned-reference maximum absolute error: {maximum}");
}
#[test]
fn reference_cases_agree_through_public_offline_and_manual_commands() {
    let root = fixture_path();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for rate in [44100, 48000, 96000] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reference.wav");
        let input = pcm(&root.join(format!("input-{rate}.f32")));
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for value in &input {
            writer.write_sample(*value).unwrap();
        }
        writer.finalize().unwrap();
        let (mut session, mut output) = Session::with_manual_output(rate).unwrap();
        let started = call(&mut session, "documents.open", json!({"path":path}));
        let document = wait(&mut session, started)["result"].clone();
        let owner = owner(&document);
        let state = edit(
            &mut session,
            &owner,
            json!({"action":"add","kind":"soft_clipper"}),
        );
        let effect = state["committed"]["effects"][0]["id"].clone();
        for case in manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["sample_rate"] == rate)
        {
            for id in ["threshold", "post"] {
                edit(
                    &mut session,
                    &owner,
                    json!({"action":"set_parameter","effect_id":effect,"parameter_id":id,"value":case[id]}),
                );
            }
            let started = call(&mut session, "processing.render", json!({"owner":owner}));
            let rendered = wait(&mut session, started)["output"]["details"].clone();
            assert_eq!(rendered["source_body_frames"], document["frames"]);
            assert_eq!(rendered["latency_frames"], 0);
            assert_eq!(rendered["tail_frames"], 0);
            assert_eq!(rendered["channels"], 2);
            let expected = pcm(&root.join(case["output"].as_str().unwrap()));
            let frames = expected.len() / 2;
            let mut offline = Vec::new();
            for start in (0..frames).step_by(4096) {
                let samples = call(
                    &mut session,
                    "processing.samples",
                    json!({"audio_revision_id":rendered["audio_revision_id"],"start_frame":start,"frames":(frames-start).min(4096)}),
                );
                for n in 0..(frames - start).min(4096) {
                    for channel in 0..2 {
                        offline.push(samples["channels"][channel][n].as_f64().unwrap() as f32);
                    }
                }
            }
            for (actual, expected) in offline.iter().zip(&expected) {
                assert!((*actual - *expected).abs() <= 2e-7);
            }
            for block in [1, 7, 64] {
                call(
                    &mut session,
                    "transport.seek",
                    json!({"document_id":document["id"],"frame":0}),
                );
                call(
                    &mut session,
                    "transport.play",
                    json!({"document_id":document["id"]}),
                );
                ready(&mut session, &mut output);
                let mut actual = vec![[0.0; 2]; frames];
                for chunk in actual.chunks_mut(block) {
                    output.render_into(chunk);
                }
                for (n, value) in actual.iter().enumerate() {
                    for channel in 0..2 {
                        assert!(
                            (value[channel] - expected[n * 2 + channel]).abs() <= 2e-7,
                            "rate {rate}, block {block}, frame {n}, {case}: {} != {}",
                            value[channel],
                            expected[n * 2 + channel]
                        );
                    }
                }
                call(&mut session, "transport.pause", json!({}));
                ready(&mut session, &mut output);
            }
        }
        let source = call(
            &mut session,
            "documents.get",
            json!({"document_id":document["id"]}),
        );
        assert_eq!(source["revision_id"], document["revision_id"]);
    }
}

#[test]
fn mono_clip_instances_order_bypass_and_history_use_the_production_processor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mono.wav");
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
    for _ in 0..2048 {
        writer.write_sample(0.75f32).unwrap();
    }
    writer.finalize().unwrap();
    let mut session = Session::default();
    let started = call(&mut session, "documents.open", json!({"path":path}));
    let document = wait(&mut session, started)["result"].clone();
    let arrangement = call(&mut session, "tracks.add", json!({}));
    let track = arrangement["tracks"][0]["id"].clone();
    for position in [0, 2048] {
        call(
            &mut session,
            "clips.insert",
            json!({"track_id":track,"document_id":document["id"],"revision_id":document["revision_id"],"source_start":0,"source_end":2048,"position":position}),
        );
    }
    let arrangement = call(&mut session, "arrangement.get", json!({}));
    let owner = json!({"kind":"clip","id":arrangement["tracks"][0]["clips"][0]["id"]});
    let other = json!({"kind":"clip","id":arrangement["tracks"][0]["clips"][1]["id"]});
    let added = edit(
        &mut session,
        &owner,
        json!({"action":"add","kind":"soft_clipper"}),
    );
    let first = added["committed"]["effects"][0]["id"].clone();
    for (id, value) in [("threshold", 1), ("post", 160)] {
        edit(
            &mut session,
            &owner,
            json!({"action":"set_parameter","effect_id":first,"parameter_id":id,"value":value}),
        );
    }
    let added = edit(
        &mut session,
        &owner,
        json!({"action":"add","kind":"soft_clipper"}),
    );
    let second = added["committed"]["effects"][1]["id"].clone();
    let render = |session: &mut Session, owner: &Value| {
        let started = call(session, "processing.render", json!({"owner":owner}));
        let render = wait(session, started)["output"]["details"].clone();
        assert_eq!(render["channels"], 1);
        call(
            session,
            "processing.samples",
            json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":1}),
        )["channels"][0][0]
            .as_f64()
            .unwrap() as f32
    };
    let first_output = wavebob::dsp::soft_clipper::curve(0.75, 1, 160);
    assert_eq!(
        render(&mut session, &owner),
        wavebob::dsp::soft_clipper::curve(first_output, 100, 128)
    );
    assert_eq!(render(&mut session, &other), 0.75);
    edit(
        &mut session,
        &owner,
        json!({"action":"move","effect_id":first,"index":1}),
    );
    assert_eq!(render(&mut session, &owner), first_output);
    edit(
        &mut session,
        &owner,
        json!({"action":"bypass","effect_id":first,"bypassed":true}),
    );
    assert_eq!(render(&mut session, &owner), 0.75);
    call(&mut session, "history.undo", json!({}));
    assert_eq!(render(&mut session, &owner), first_output);
    edit(
        &mut session,
        &owner,
        json!({"action":"remove","effect_id":second}),
    );
    assert_eq!(render(&mut session, &owner), first_output);
    assert_eq!(
        call(&mut session, "processing.get", json!({"owner":other}))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        call(
            &mut session,
            "documents.samples",
            json!({"document_id":document["id"],"frames":1})
        )["channels"][0][0],
        0.75
    );
    assert_eq!(
        call(
            &mut session,
            "documents.get",
            json!({"document_id":document["id"]})
        )["revision_id"],
        document["revision_id"]
    );
}
