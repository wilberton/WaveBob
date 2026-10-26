use serde_json::{Value, json};
use wavebob::{Session, protocol, transport::ManualOutput};
fn call(session: &mut Session, name: &str, params: Value) -> Value {
    let response = serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"limiter","command":name,"params":params}).to_string(),
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
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/processing/limiter")
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
    for name in [
        "default-44100",
        "compressor",
        "held-out-96000",
        "one-second-lookahead",
    ] {
        let case = manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        let rate = case["sample_rate"].as_u64().unwrap() as u32;
        let input = pcm(&root.join(case["input"].as_str().unwrap()));
        let raw = pcm(&root.join(case["output"].as_str().unwrap()));
        let latency = case["latency_frames"].as_u64().unwrap() as usize;
        let expected = &raw[latency * 2..];
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
            json!({"action":"add","kind":"limiter"}),
        );
        let id = added["committed"]["effects"][0]["id"].clone();
        for (index, key) in wavebob::dsp::limiter::IDS.iter().enumerate() {
            edit(
                &mut session,
                &owner,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":case["values"][index]}),
            );
        }
        edit(
            &mut session,
            &owner,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":"exact_ceiling","value":case["exact_ceiling"]}),
        );
        let started = call(&mut session, "processing.render", json!({"owner":owner}));
        let rendered = wait(&mut session, started)["output"]["details"].clone();
        assert_eq!(rendered["source_body_frames"], document["frames"]);
        assert_eq!(rendered["latency_frames"], latency);
        assert_eq!(rendered["tail_frames"], 0);
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
        json!({"action":"add","kind":"limiter"}),
    );
    let distortion = added["committed"]["effects"][0]["id"].clone();
    for (key, value) in [("gain", 2000), ("c_thres", 500), ("c_ratio", 1000)] {
        edit(
            &mut session,
            &owner,
            json!({"action":"set_parameter","effect_id":distortion,"parameter_id":key,"value":value}),
        );
    }
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
    for id in [&distortion, &clipper] {
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
fn loop_wrap_carries_peak_rms_and_gate_state_across_non_aligned_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, mut output) = Session::with_manual_output(8000).unwrap();
    let mut documents = Vec::new();
    for (name, repetitions) in [("loop.wav", 1), ("continuous.wav", 16)] {
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
            json!({"action":"add","kind":"limiter"}),
        );
        let id = added["committed"]["effects"][0]["id"].clone();
        for (key, value) in [("c_thres", 500), ("c_ratio", 1000), ("g_thres", 600)] {
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
        json!({"audio_revision_id":rendered["audio_revision_id"],"start_frame":0,"frames":2032}),
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
        for (channel, &sample) in value.iter().enumerate() {
            let e = expected["channels"][0][frame].as_f64().unwrap() as f32;
            assert!(
                (sample - e).abs() <= 2e-6,
                "loop {frame}/{channel}: {} != {e}",
                sample
            );
        }
    }
    call(&mut session, "transport.pause", json!({}));
    ready(&mut session, &mut output);
    output.render_into(&mut [[0.0; 2]; 64]);
    call(
        &mut session,
        "transport.play",
        json!({"document_id":documents[0]["id"]}),
    );
    ready(&mut session, &mut output);
    let mut resumed = [[0.0; 2]; 1016];
    output.render_into(&mut resumed);
    for (frame, value) in resumed.iter().enumerate().skip(64) {
        for (channel, &sample) in value.iter().enumerate() {
            let e = expected["channels"][0][1016 + frame].as_f64().unwrap() as f32;
            assert!(
                (sample - e).abs() <= 2e-6,
                "resume {frame}/{channel}: {} != {e}",
                sample
            );
        }
    }
}
#[test]
fn actual_instance_meter_history_is_bounded_finite_and_marks_host_bypass() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Meter.wav");
    let mut wav = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..8192 {
        let x = (frame as f32 * 0.173).sin() * 1.8;
        wav.write_sample(x).unwrap();
        wav.write_sample(-x * 0.3).unwrap();
    }
    wav.finalize().unwrap();
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let d = wait(&mut s, job)["result"].clone();
    let o = owner(&d);
    let state = edit(&mut s, &o, json!({"action":"add","kind":"limiter"}));
    let id = state["committed"]["effects"][0]["id"].clone();
    call(&mut s, "transport.loop.set", json!({"enabled":true}));
    call(&mut s, "transport.play", json!({"document_id":d["id"]}));
    ready(&mut s, &mut output);
    let mut last = 0;
    let mut reduced = false;
    for _ in 0..80 {
        output.render_into(&mut [[0.0; 2]; 64]);
        let t = call(&mut s, "transport.get", json!({}));
        assert_eq!(t["meters_truncated"], false);
        let meters = t["effect_meters"].as_array().unwrap();
        assert_eq!(meters.len(), 1);
        let m = &meters[0];
        assert_eq!(m["instance_id"], id);
        assert_eq!(m["bypassed"], false);
        let cols = m["columns"].as_array().unwrap();
        assert!(!cols.is_empty() && cols.len() <= 32);
        let latest = cols.last().unwrap();
        let seq = latest["sequence"].as_u64().unwrap();
        assert!(seq > last);
        last = seq;
        assert_eq!(latest["sample_rate"], 48000);
        assert!(latest["frames"].as_u64().unwrap() > 0);
        for column in cols {
            for key in ["input", "output", "level", "gain"] {
                assert!(column["levels"][key].as_f64().unwrap().is_finite());
            }
            assert!(column["levels"]["output"].as_f64().unwrap() <= 1.000001);
            reduced |= column["levels"]["gain"].as_f64().unwrap() < 0.9;
        }
    }
    assert!(reduced);
    edit(
        &mut s,
        &o,
        json!({"action":"bypass","effect_id":id,"bypassed":true}),
    );
    ready(&mut s, &mut output);
    output.render_into(&mut [[0.0; 2]; 1024]); // Finish the prepared 20 ms bypass transition.
    let t = call(&mut s, "transport.get", json!({}));
    assert_eq!(t["effect_meters"][0]["bypassed"], true);
    edit(&mut s, &o, json!({"action":"remove","effect_id":id}));
    ready(&mut s, &mut output);
    output.render_into(&mut [[0.0; 2]; 64]);
    assert_eq!(
        call(&mut s, "transport.get", json!({}))["effect_meters"],
        json!([])
    );
}

#[test]
fn one_second_clip_lookahead_stays_aligned_with_a_parallel_dry_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Parallel.wav");
    let mut wav = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    let source: Vec<f32> = (0..2048)
        .map(|i| {
            if i == 63 {
                0.2
            } else {
                (i as f32 * 0.147).sin() * 0.05
            }
        })
        .collect();
    for &x in &source {
        wav.write_sample(x).unwrap();
        wav.write_sample(-x * 0.5).unwrap();
    }
    wav.finalize().unwrap();
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let d = wait(&mut s, job)["result"].clone();
    let mut wet = Value::Null;
    for index in 0..2 {
        let a = call(&mut s, "tracks.add", json!({}));
        let track = a["tracks"][index]["id"].clone();
        let a = call(
            &mut s,
            "clips.insert",
            json!({"track_id":track,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":0,"source_end":2048,"position":137}),
        );
        if index == 0 {
            wet = json!({"kind":"clip","id":a["tracks"][0]["clips"][0]["id"]});
        }
    }
    let p = edit(&mut s, &wet, json!({"action":"add","kind":"limiter"}));
    let id = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &wet,
        json!({"action":"set_parameter","effect_id":id,"parameter_id":"att","value":10000}),
    );
    let revision = s.snapshot().arrangement.revision_id;
    let job = call(&mut s, "mix.prepare", json!({"revision_id":revision}));
    wait(&mut s, job);
    let mixed = call(
        &mut s,
        "mix.samples",
        json!({"revision_id":revision,"start_frame":0,"frames":2185}),
    );
    for (i, &x) in source.iter().enumerate() {
        assert!(
            (mixed["channels"][0][137 + i].as_f64().unwrap() as f32 - x * 2.0).abs() < 2e-6,
            "left {i}"
        );
        assert!(
            (mixed["channels"][1][137 + i].as_f64().unwrap() as f32 + x).abs() < 2e-6,
            "right {i}"
        );
    }
    call(&mut s, "mix.play", json!({}));
    ready(&mut s, &mut output);
    let mut live = vec![[0.0; 2]; 2185];
    for chunk in live.chunks_mut(7) {
        output.render_into(chunk);
    }
    for (i, frame) in live.iter().enumerate() {
        for (channel, &x) in frame.iter().enumerate() {
            assert!(
                (x - mixed["channels"][channel][i].as_f64().unwrap() as f32).abs() < 2e-6,
                "live {i}/{channel}"
            );
        }
    }
    let a = call(&mut s, "arrangement.get", json!({}));
    assert_eq!(a["tracks"][0]["clips"][0]["position"], 137);
    assert_eq!(a["tracks"][1]["clips"][0]["position"], 137);
}
