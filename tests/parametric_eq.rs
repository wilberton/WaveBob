use serde_json::{Value, json};
use wavebob::{Session, protocol, transport::ManualOutput};
fn call(session: &mut Session, name: &str, params: Value) -> Value {
    let response = serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"parametric-eq","command":name,"params":params}).to_string(),
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
fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/processing/parametric-eq")
}
fn pcm(path: &std::path::Path) -> Vec<f32> {
    use std::io::Read;
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
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
fn reference_pcm_agrees_through_offline_and_manual_output_at_all_block_sizes() {
    let root = fixture_path();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for name in ["default-44100", "peak-48000", "held-out-mixed-96000"] {
        let case = manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        let rate = case["sample_rate"].as_u64().unwrap() as u32;
        let input = pcm(&root.join(case["input"].as_str().unwrap()));
        let expected = pcm(&root.join(case["output"].as_str().unwrap()));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reference.wav");
        let mut wav = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for v in &input {
            wav.write_sample(*v).unwrap();
        }
        wav.finalize().unwrap();
        let (mut session, mut output) = Session::with_manual_output(rate).unwrap();
        let started = call(&mut session, "documents.open", json!({"path":path}));
        let document = wait(&mut session, started)["result"].clone();
        let owner = owner(&document);
        let added = edit(
            &mut session,
            &owner,
            json!({"action":"add","kind":"parametric_eq"}),
        );
        let id = added["committed"]["effects"][0]["id"].clone();
        for (group, offset) in [
            ("level", 0),
            ("freq", 7),
            ("width", 14),
            ("type", 21),
            ("order", 28),
        ] {
            for band in 0..7 {
                edit(
                    &mut session,
                    &owner,
                    json!({"action":"set_parameter","effect_id":id,"parameter_id":wavebob::dsp::parametric_eq::parameter_id(group,band),"value":case["parameters"][offset+band]}),
                );
            }
        }
        for band in 0..7 {
            edit(
                &mut session,
                &owner,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":wavebob::dsp::parametric_eq::parameter_id("enabled",band),"value":case["mask"].as_u64().unwrap()&(1<<band)!=0}),
            );
        }
        edit(
            &mut session,
            &owner,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":"main","value":case["parameters"][35]}),
        );
        let started = call(&mut session, "processing.render", json!({"owner":owner}));
        let rendered = wait(&mut session, started)["output"]["details"].clone();
        assert_eq!(rendered["source_body_frames"], document["frames"]);
        assert_eq!(rendered["latency_frames"], 64);
        assert_eq!(rendered["channels"], 2);
        let frames = input.len() / 2;
        for start in (0..frames).step_by(4096) {
            let n = (frames - start).min(4096);
            let samples = call(
                &mut session,
                "processing.samples",
                json!({"audio_revision_id":rendered["audio_revision_id"],"start_frame":start,"frames":n}),
            );
            for frame in 0..n {
                for channel in 0..2 {
                    let actual = samples["channels"][channel][frame].as_f64().unwrap() as f32;
                    assert!(
                        (actual - expected[(start + frame) * 2 + channel]).abs() <= 2e-6,
                        "{name}: offline {start}+{frame}"
                    );
                }
            }
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
            for (frame, values) in actual.iter().enumerate() {
                for channel in 0..2 {
                    assert!(
                        (values[channel] - expected[frame * 2 + channel]).abs() <= 2e-6,
                        "{name}: block{block}, frame{frame}, channel{channel}: {} != {}",
                        values[channel],
                        expected[frame * 2 + channel]
                    );
                }
            }
            call(&mut session, "transport.pause", json!({}));
            ready(&mut session, &mut output);
        }
        for start in [113usize, 2049] {
            call(
                &mut session,
                "transport.seek",
                json!({"document_id":document["id"],"frame":start}),
            );
            call(
                &mut session,
                "transport.play",
                json!({"document_id":document["id"]}),
            );
            ready(&mut session, &mut output);
            let mut actual = [[0.0; 2]; 256];
            output.render_into(&mut actual);
            for (frame, values) in actual.iter().enumerate() {
                for channel in 0..2 {
                    assert!(
                        (values[channel] - expected[(start + frame) * 2 + channel]).abs() <= 2e-6,
                        "{name}: seek{start}, frame{frame}"
                    );
                }
            }
            call(&mut session, "transport.pause", json!({}));
            ready(&mut session, &mut output);
        }
        assert_eq!(
            call(
                &mut session,
                "documents.get",
                json!({"document_id":document["id"]})
            )["revision_id"],
            document["revision_id"]
        );
    }
}

#[test]
fn independent_instances_bypass_order_and_undo_keep_source_pcm_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mono.wav");
    let mut wav = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for n in 0..1024 {
        wav.write_sample(if n < 100 { 2.0f32 } else { 0.0 })
            .unwrap();
    }
    wav.finalize().unwrap();
    let mut session = Session::default();
    let started = call(&mut session, "documents.open", json!({"path":path}));
    let document = wait(&mut session, started)["result"].clone();
    let owner = owner(&document);
    let started = call(&mut session, "documents.open", json!({"path":path}));
    let other = wait(&mut session, started)["result"].clone();
    let other = json!({"kind":"document","id":other["id"]});
    let added = edit(
        &mut session,
        &owner,
        json!({"action":"add","kind":"parametric_eq"}),
    );
    let equalizer = added["committed"]["effects"][0]["id"].clone();
    edit(
        &mut session,
        &owner,
        json!({"action":"set_parameter","effect_id":equalizer,"parameter_id":"level4","value":1800}),
    );
    let added = edit(
        &mut session,
        &owner,
        json!({"action":"add","kind":"soft_clipper"}),
    );
    let clipper = added["committed"]["effects"][1]["id"].clone();
    let render = |session: &mut Session, owner: &Value| {
        let started = call(session, "processing.render", json!({"owner":owner}));
        let details = wait(session, started)["output"]["details"].clone();
        let frames = (details["source_body_frames"].as_u64().unwrap()
            + details["tail_frames"].as_u64().unwrap())
        .min(4096);
        let samples = call(
            session,
            "processing.samples",
            json!({"audio_revision_id":details["audio_revision_id"],"start_frame":0,"frames":frames}),
        );
        (details, samples["channels"].clone())
    };
    let (_, before) = render(&mut session, &owner);
    edit(
        &mut session,
        &owner,
        json!({"action":"move","effect_id":clipper,"index":0}),
    );
    assert_ne!(render(&mut session, &owner).1, before);
    call(&mut session, "history.undo", json!({}));
    assert_eq!(render(&mut session, &owner).1, before);
    for id in [&equalizer, &clipper] {
        edit(
            &mut session,
            &owner,
            json!({"action":"bypass","effect_id":id,"bypassed":true}),
        );
    }
    let (metadata, bypassed) = render(&mut session, &owner);
    assert_eq!(metadata["tail_frames"], 0);
    assert_eq!(bypassed[0][0], 2.0);
    let (metadata, untouched) = render(&mut session, &other);
    assert_eq!(metadata["channels"], 1);
    assert_eq!(metadata["tail_frames"], 0);
    assert_eq!(untouched[0][0], 2.0);
    assert_eq!(
        call(&mut session, "processing.get", json!({"owner":other}))["committed"]["effects"],
        json!([])
    );
    assert_eq!(
        call(
            &mut session,
            "documents.get",
            json!({"document_id":document["id"]})
        )["revision_id"],
        document["revision_id"]
    );
    assert_eq!(
        call(
            &mut session,
            "documents.samples",
            json!({"document_id":document["id"],"frames":1})
        )["channels"][0][0],
        2.0
    );
}

#[test]
fn loop_wrap_carries_iir_and_internal_quantum_state_across_non_aligned_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, mut output) = Session::with_manual_output(8000).unwrap();
    let mut documents = Vec::new();
    for (name, repetitions) in [("loop.wav", 1), ("continuous.wav", 8)] {
        let path = dir.path().join(name);
        let mut wav = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 8000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for _ in 0..repetitions {
            for frame in 0..127 {
                wav.write_sample(if frame == 0 { 1.0_f32 } else { 0.0 })
                    .unwrap();
            }
        }
        wav.finalize().unwrap();
        let started = call(&mut session, "documents.open", json!({"path":path}));
        let doc = wait(&mut session, started)["result"].clone();
        let owner = owner(&doc);
        let added = edit(
            &mut session,
            &owner,
            json!({"action":"add","kind":"parametric_eq"}),
        );
        let id = added["committed"]["effects"][0]["id"].clone();
        for (key, value) in [("level4", 1800), ("freq4", 0), ("width4", 0)] {
            edit(
                &mut session,
                &owner,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":value}),
            );
        }
        documents.push(doc);
    }
    let started = call(
        &mut session,
        "processing.render",
        json!({"owner":owner(&documents[1])}),
    );
    let rendered = wait(&mut session, started)["output"]["details"].clone();
    let expected = call(
        &mut session,
        "processing.samples",
        json!({"audio_revision_id":rendered["audio_revision_id"],"start_frame":0,"frames":1016}),
    );
    call(&mut session, "transport.loop.set", json!({"enabled":true}));
    call(
        &mut session,
        "transport.seek",
        json!({"document_id":documents[0]["id"],"frame":0}),
    );
    call(
        &mut session,
        "transport.play",
        json!({"document_id":documents[0]["id"]}),
    );
    ready(&mut session, &mut output);
    let mut actual = [[0.0; 2]; 1016];
    for chunk in actual.chunks_mut(7) {
        output.render_into(chunk);
    }
    for (frame, value) in actual.iter().enumerate() {
        let expected = expected["channels"][0][frame].as_f64().unwrap() as f32;
        assert!(
            (value[0] - expected).abs() <= 2e-6,
            "loop frame{frame}: {} != {expected}",
            value[0]
        );
        assert_eq!(value[0], value[1]);
    }
}
