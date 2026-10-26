use serde_json::{Value, json};
use wavebob::{Session, protocol, transport::ManualOutput};
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

fn rendered(session: &mut Session, owner: &Value) -> Value {
    let job = call(session, "processing.render", json!({"owner":owner}));
    let m = wait(session, job)["output"]["details"].clone();
    assert_eq!(m["latency_frames"], 0);
    call(
        session,
        "processing.samples",
        json!({"audio_revision_id":m["audio_revision_id"],"start_frame":0,"frames":1024}),
    )["channels"]
        .clone()
}
fn near(actual: &Value, expected: &[f32], label: &str) {
    for i in 0..expected.len() / 2 {
        for c in 0..2 {
            let a = actual[c][i].as_f64().unwrap() as f32;
            assert!(
                (a - expected[i * 2 + c]).abs() <= 2e-6,
                "{label} {i}/{c}: {a} != {}",
                expected[i * 2 + c]
            );
        }
    }
}
#[test]
fn balance_reference_pcm_public_offline_manual_seek_loop_and_history() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/processing/balance");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let input = pcm(&root.join(case["input"].as_str().unwrap()));
        let expected = pcm(&root.join(case["output"].as_str().unwrap()));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Balance.wav");
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
        for x in input {
            wav.write_sample(x).unwrap();
        }
        wav.finalize().unwrap();
        let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
        let job = call(&mut s, "documents.open", json!({"path":path}));
        let d = wait(&mut s, job)["result"].clone();
        let o = owner(&d);
        let p = edit(&mut s, &o, json!({"action":"add","kind":"balance"}));
        let id = p["committed"]["effects"][0]["id"].clone();
        for key in ["balance", "volume"] {
            edit(
                &mut s,
                &o,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":case[key]}),
            );
        }
        near(&rendered(&mut s, &o), &expected, "offline");
        for block in [1, 7, 64] {
            call(
                &mut s,
                "transport.seek",
                json!({"document_id":d["id"],"frame":0}),
            );
            call(&mut s, "transport.play", json!({"document_id":d["id"]}));
            ready(&mut s, &mut output);
            let mut live = vec![[0.0; 2]; 1024];
            for chunk in live.chunks_mut(block) {
                output.render_into(chunk);
            }
            for (i, f) in live.iter().enumerate() {
                for c in 0..2 {
                    assert!((f[c] - expected[2 * i + c]).abs() <= 2e-6);
                }
            }
            call(&mut s, "transport.pause", json!({}));
            ready(&mut s, &mut output);
        }
        call(&mut s, "transport.loop.set", json!({"enabled":true}));
        call(
            &mut s,
            "transport.seek",
            json!({"document_id":d["id"],"frame":113}),
        );
        call(&mut s, "transport.play", json!({"document_id":d["id"]}));
        ready(&mut s, &mut output);
        let mut live = vec![[0.0; 2]; 2048];
        output.render_into(&mut live);
        for (i, f) in live.iter().enumerate() {
            for c in 0..2 {
                assert!((f[c] - expected[((i + 113) % 1024) * 2 + c]).abs() <= 2e-6);
            }
        }
        let p = call(&mut s, "processing.get", json!({"owner":o}));
        let h = call(&mut s, "history.get", json!({}))["undo_count"].clone();
        let preview = call(
            &mut s,
            "processing.preview.begin",
            json!({"owner":o,"expected_edit_revision":p["committed"]["edit_revision_id"]}),
        );
        call(
            &mut s,
            "processing.preview.update",
            json!({"preview_id":preview["preview_id"],"change":{"action":"set_parameter","effect_id":id,"parameter_id":"balance","value":63}}),
        );
        call(
            &mut s,
            "processing.preview.cancel",
            json!({"preview_id":preview["preview_id"]}),
        );
        assert_eq!(call(&mut s, "history.get", json!({}))["undo_count"], h);
        near(&rendered(&mut s, &o), &expected, "cancel");
        assert_eq!(
            call(&mut s, "documents.get", json!({"document_id":d["id"]}))["revision_id"],
            d["revision_id"]
        );
    }
}

fn utility_owner_order_bypass_and_mono_contract(kind: &str, parameter: &str, value: i32) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Mono utility.wav");
    let mut wav = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for _ in 0..1024 {
        wav.write_sample(2.0f32).unwrap();
    }
    wav.finalize().unwrap();
    let mut s = Session::default();
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let d = wait(&mut s, job)["result"].clone();
    let o = owner(&d);
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let other = wait(&mut s, job)["result"].clone();
    let other = owner(&other);
    let p = edit(&mut s, &o, json!({"action":"add","kind":kind}));
    let effect = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &o,
        json!({"action":"set_parameter","effect_id":effect,"parameter_id":parameter,"value":value}),
    );
    assert_eq!(
        rendered(&mut s, &o).as_array().unwrap().len(),
        if kind == "transient_processor" { 1 } else { 2 }
    );
    let p = edit(&mut s, &o, json!({"action":"add","kind":"soft_clipper"}));
    let clipper = p["committed"]["effects"][1]["id"].clone();
    let before = rendered(&mut s, &o);
    edit(
        &mut s,
        &o,
        json!({"action":"move","effect_id":clipper,"index":0}),
    );
    assert_ne!(before, rendered(&mut s, &o));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(before, rendered(&mut s, &o));
    for id in [effect, clipper] {
        edit(
            &mut s,
            &o,
            json!({"action":"bypass","effect_id":id,"bypassed":true}),
        );
    }
    assert_eq!(rendered(&mut s, &o)[0][0], 2.0);
    let untouched = rendered(&mut s, &other);
    assert_eq!(untouched.as_array().unwrap().len(), 1);
    assert_eq!(untouched[0][0], 2.0);
    assert_eq!(
        call(&mut s, "documents.get", json!({"document_id":d["id"]}))["revision_id"],
        d["revision_id"]
    );
}
#[test]
fn balance_mono_promotion_instance_isolation_order_and_bypass() {
    utility_owner_order_bypass_and_mono_contract("balance", "volume", 320);
}

#[test]
fn stereo_enhancer_reference_pcm_public_manual_seek_and_delayed_extent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/processing/stereo_enhancer");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let input = pcm(&root.join(case["input"].as_str().unwrap()));
        let expected = pcm(&root.join(case["output"].as_str().unwrap()));
        let rate = case["sample_rate"].as_u64().unwrap() as u32;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Enhancer.wav");
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
        for &x in &input {
            wav.write_sample(x).unwrap();
        }
        wav.finalize().unwrap();
        let (mut s, mut output) = Session::with_manual_output(rate).unwrap();
        let job = call(&mut s, "documents.open", json!({"path":path}));
        let d = wait(&mut s, job)["result"].clone();
        let o = owner(&d);
        let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_enhancer"}));
        let id = p["committed"]["effects"][0]["id"].clone();
        for (i, key) in wavebob::dsp::stereo_enhancer::IDS.iter().enumerate() {
            edit(
                &mut s,
                &o,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":case["values"][i]}),
            );
        }
        let job = call(&mut s, "processing.render", json!({"owner":o}));
        let m = wait(&mut s, job)["output"]["details"].clone();
        assert_eq!(m["latency_frames"], 0);
        assert_eq!(m["channels"], 2);
        assert_eq!(m["source_body_frames"], 1024);
        assert!(m["tail_frames"].as_u64().unwrap() >= case["delay_frames"].as_u64().unwrap());
        let n = expected.len() / 2;
        for start in (0..n).step_by(4096) {
            let frames = (n - start).min(4096);
            let samples = call(
                &mut s,
                "processing.samples",
                json!({"audio_revision_id":m["audio_revision_id"],"start_frame":start,"frames":frames}),
            );
            near(
                &samples["channels"],
                &expected[start * 2..(start + frames) * 2],
                "offline offset",
            );
        }
        for block in [1, 7, 64] {
            call(
                &mut s,
                "transport.seek",
                json!({"document_id":d["id"],"frame":0}),
            );
            call(&mut s, "transport.play", json!({"document_id":d["id"]}));
            ready(&mut s, &mut output);
            let mut live = vec![[0.0; 2]; n];
            for chunk in live.chunks_mut(block) {
                output.render_into(chunk);
            }
            for (i, f) in live.iter().enumerate() {
                for c in 0..2 {
                    assert!(
                        (f[c] - expected[i * 2 + c]).abs() <= 2e-6,
                        "{} block{block} {i}/{c}",
                        case["name"]
                    );
                }
            }
            call(&mut s, "transport.pause", json!({}));
            ready(&mut s, &mut output);
        }
        call(
            &mut s,
            "transport.seek",
            json!({"document_id":d["id"],"frame":113}),
        );
        call(&mut s, "transport.play", json!({"document_id":d["id"]}));
        ready(&mut s, &mut output);
        let mut live = vec![[0.0; 2]; n - 113];
        output.render_into(&mut live);
        for (i, f) in live.iter().enumerate() {
            for c in 0..2 {
                assert!(
                    (f[c] - expected[(i + 113) * 2 + c]).abs() <= 2e-6,
                    "seek {} {i}/{c}",
                    case["name"]
                );
            }
        }
    }
}
#[test]
fn stereo_enhancer_mono_promotion_order_bypass_and_isolation() {
    utility_owner_order_bypass_and_mono_contract("stereo_enhancer", "volume", 320);
}
#[test]
fn stereo_enhancer_loop_carries_phase_line_instead_of_resetting() {
    use wavebob_effect_reference::stereo_enhancer as reference;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Loop.wav");
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
    let input: Vec<[f32; 2]> = (0..127)
        .map(|i| [(i as f32 * 0.13).sin(), (i as f32 * 0.17).cos() * 0.5])
        .collect();
    for f in &input {
        for &x in f {
            wav.write_sample(x).unwrap();
        }
    }
    wav.finalize().unwrap();
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let d = wait(&mut s, job)["result"].clone();
    let o = owner(&d);
    let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_enhancer"}));
    let id = p["committed"]["effects"][0]["id"].clone();
    let v = [0, 256, -54, -248, 1, 0];
    for (i, key) in wavebob::dsp::stereo_enhancer::IDS.iter().enumerate() {
        edit(
            &mut s,
            &o,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":v[i]}),
        );
    }
    call(&mut s, "transport.loop.set", json!({"enabled":true}));
    call(&mut s, "transport.play", json!({"document_id":d["id"]}));
    ready(&mut s, &mut output);
    let mut live = vec![[0.0; 2]; 2032];
    for chunk in live.chunks_mut(7) {
        output.render_into(chunk);
    }
    let mut left: Vec<_> = (0..2032).map(|i| input[i % 127][0]).collect();
    let mut right: Vec<_> = (0..2032).map(|i| input[i % 127][1]).collect();
    let mut r = reference::StereoEnhancer::with_settings(48000, &reference::Settings { v });
    r.flush();
    r.process(&mut left, &mut right);
    for (i, f) in live.iter().enumerate() {
        assert!((f[0] - left[i]).abs() <= 2e-6);
        assert!((f[1] - right[i]).abs() <= 2e-6);
    }
}

#[test]
fn stereo_shaper_reference_pcm_public_manual_seek_and_delayed_extent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/processing/stereo_shaper");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let input = pcm(&root.join(case["input"].as_str().unwrap()));
        let expected = pcm(&root.join(case["output"].as_str().unwrap()));
        let rate = case["sample_rate"].as_u64().unwrap() as u32;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Shaper.wav");
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
        for &x in &input {
            wav.write_sample(x).unwrap();
        }
        wav.finalize().unwrap();
        let (mut s, mut output) = Session::with_manual_output(rate).unwrap();
        let job = call(&mut s, "documents.open", json!({"path":path}));
        let d = wait(&mut s, job)["result"].clone();
        let o = owner(&d);
        let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_shaper"}));
        let id = p["committed"]["effects"][0]["id"].clone();
        for (i, key) in wavebob::dsp::stereo_shaper::IDS.iter().enumerate() {
            edit(
                &mut s,
                &o,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":case["values"][i]}),
            );
        }
        let job = call(&mut s, "processing.render", json!({"owner":o}));
        let m = wait(&mut s, job)["output"]["details"].clone();
        assert_eq!(m["latency_frames"], 0);
        assert_eq!(m["channels"], 2);
        assert_eq!(m["source_body_frames"], 1024);
        assert!(
            m["tail_frames"].as_u64().unwrap()
                >= if case["values"][6] == -1 {
                    0
                } else {
                    case["delay_frames"].as_u64().unwrap()
                }
        );
        let total = m["source_body_frames"].as_u64().unwrap() + m["tail_frames"].as_u64().unwrap();
        let n = (expected.len() / 2).min(total as usize);
        assert!(expected[n * 2..].iter().all(|x| x.abs() <= 1e-5));
        for start in (0..n).step_by(4096) {
            let frames = (n - start).min(4096);
            let samples = call(
                &mut s,
                "processing.samples",
                json!({"audio_revision_id":m["audio_revision_id"],"start_frame":start,"frames":frames}),
            );
            near(
                &samples["channels"],
                &expected[start * 2..(start + frames) * 2],
                "offline offset",
            );
        }
        for block in [1, 7, 64] {
            call(
                &mut s,
                "transport.seek",
                json!({"document_id":d["id"],"frame":0}),
            );
            call(&mut s, "transport.play", json!({"document_id":d["id"]}));
            ready(&mut s, &mut output);
            let mut live = vec![[0.0; 2]; n];
            for chunk in live.chunks_mut(block) {
                output.render_into(chunk);
            }
            for (i, f) in live.iter().enumerate() {
                for c in 0..2 {
                    assert!(
                        (f[c] - expected[i * 2 + c]).abs() <= 2e-6,
                        "{} block{block} {i}/{c}",
                        case["name"]
                    );
                }
            }
            call(&mut s, "transport.pause", json!({}));
            ready(&mut s, &mut output);
        }
        call(
            &mut s,
            "transport.seek",
            json!({"document_id":d["id"],"frame":113}),
        );
        call(&mut s, "transport.play", json!({"document_id":d["id"]}));
        ready(&mut s, &mut output);
        let mut live = vec![[0.0; 2]; n - 113];
        output.render_into(&mut live);
        for (i, f) in live.iter().enumerate() {
            for c in 0..2 {
                assert!(
                    (f[c] - expected[(i + 113) * 2 + c]).abs() <= 2e-6,
                    "seek {} {i}/{c}",
                    case["name"]
                );
            }
        }
    }
}

#[test]
fn stereo_shaper_mono_promotion_order_bypass_and_isolation() {
    utility_owner_order_bypass_and_mono_contract("stereo_shaper", "left", 25600);
}

#[test]
fn stereo_shaper_loop_carries_phase_line_instead_of_resetting() {
    use wavebob_effect_reference::stereo_shaper as reference;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Loop.wav");
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
    let input: Vec<[f32; 2]> = (0..127)
        .map(|i| [(i as f32 * 0.13).sin(), (i as f32 * 0.17).cos() * 0.5])
        .collect();
    for f in &input {
        for &x in f {
            wav.write_sample(x).unwrap();
        }
    }
    wav.finalize().unwrap();
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let d = wait(&mut s, job)["result"].clone();
    let o = owner(&d);
    let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_shaper"}));
    let id = p["committed"]["effects"][0]["id"].clone();
    let v = [0, 12800, 12800, 0, 1408, -2496, 1];
    for (i, key) in wavebob::dsp::stereo_shaper::IDS.iter().enumerate() {
        edit(
            &mut s,
            &o,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":v[i]}),
        );
    }
    call(&mut s, "transport.loop.set", json!({"enabled":true}));
    call(&mut s, "transport.play", json!({"document_id":d["id"]}));
    ready(&mut s, &mut output);
    let mut live = vec![[0.0; 2]; 2032];
    for chunk in live.chunks_mut(7) {
        output.render_into(chunk);
    }
    let mut left: Vec<_> = (0..2032).map(|i| input[i % 127][0]).collect();
    let mut right: Vec<_> = (0..2032).map(|i| input[i % 127][1]).collect();
    let mut r = reference::StereoShaper::with_settings(
        48000,
        &reference::Settings {
            v: [v[0], v[1], v[2], v[3], v[4], v[5], 0, v[6]],
        },
    );
    r.flush();
    for (l, rpart) in left.chunks_mut(64).zip(right.chunks_mut(64)) {
        r.process(l, rpart);
    }
    for (i, f) in live.iter().enumerate() {
        assert!((f[0] - left[i]).abs() <= 2e-6);
        assert!((f[1] - right[i]).abs() <= 2e-6);
    }
}

#[test]
fn transient_processor_reference_pcm_public_manual_seek_and_filter_tail() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/processing/transient_processor");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let input = pcm(&root.join(case["input"].as_str().unwrap()));
        let expected = pcm(&root.join(case["output"].as_str().unwrap()));
        let rate = case["sample_rate"].as_u64().unwrap() as u32;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Transient.wav");
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
        for &x in &input {
            wav.write_sample(x).unwrap();
        }
        wav.finalize().unwrap();
        let (mut s, mut output) = Session::with_manual_output(rate).unwrap();
        let job = call(&mut s, "documents.open", json!({"path":path}));
        let d = wait(&mut s, job)["result"].clone();
        let o = owner(&d);
        let p = edit(
            &mut s,
            &o,
            json!({"action":"add","kind":"transient_processor"}),
        );
        let id = p["committed"]["effects"][0]["id"].clone();
        for (i, key) in wavebob::dsp::transient_processor::IDS.iter().enumerate() {
            edit(
                &mut s,
                &o,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":case["values"][i]}),
            );
        }
        let job = call(&mut s, "processing.render", json!({"owner":o}));
        let m = wait(&mut s, job)["output"]["details"].clone();
        assert_eq!(m["latency_frames"], 0);
        assert_eq!(m["channels"], 2);
        assert_eq!(m["source_body_frames"], 1024);
        if case["values"][8] == 0 {
            assert_eq!(m["tail_frames"], 0);
        }
        let total = m["source_body_frames"].as_u64().unwrap() + m["tail_frames"].as_u64().unwrap();
        let n = (expected.len() / 2).min(total as usize);
        assert!(expected[n * 2..].iter().all(|x| x.abs() <= 1e-5));
        for start in (0..n).step_by(4096) {
            let frames = (n - start).min(4096);
            let samples = call(
                &mut s,
                "processing.samples",
                json!({"audio_revision_id":m["audio_revision_id"],"start_frame":start,"frames":frames}),
            );
            near(
                &samples["channels"],
                &expected[start * 2..(start + frames) * 2],
                "offline offset",
            );
        }
        for block in [1, 7, 64] {
            call(
                &mut s,
                "transport.seek",
                json!({"document_id":d["id"],"frame":0}),
            );
            call(&mut s, "transport.play", json!({"document_id":d["id"]}));
            ready(&mut s, &mut output);
            let mut live = vec![[0.0; 2]; n];
            for chunk in live.chunks_mut(block) {
                output.render_into(chunk);
            }
            for (i, f) in live.iter().enumerate() {
                for c in 0..2 {
                    assert!(
                        (f[c] - expected[i * 2 + c]).abs() <= 2e-6,
                        "{} block{block} {i}/{c}",
                        case["name"]
                    );
                }
            }
            call(&mut s, "transport.pause", json!({}));
            ready(&mut s, &mut output);
        }
        call(
            &mut s,
            "transport.seek",
            json!({"document_id":d["id"],"frame":113}),
        );
        call(&mut s, "transport.play", json!({"document_id":d["id"]}));
        ready(&mut s, &mut output);
        let mut live = vec![[0.0; 2]; n - 113];
        output.render_into(&mut live);
        for (i, f) in live.iter().enumerate() {
            for c in 0..2 {
                assert!(
                    (f[c] - expected[(i + 113) * 2 + c]).abs() <= 2e-6,
                    "seek {} {i}/{c}",
                    case["name"]
                );
            }
        }
    }
}

#[test]
fn transient_processor_mono_order_bypass_and_isolation() {
    utility_owner_order_bypass_and_mono_contract("transient_processor", "gain", 160);
}

#[test]
fn transient_processor_loop_carries_detector_and_crossover_state() {
    use wavebob_effect_reference::transient_processor as reference;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Loop.wav");
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
    let input: Vec<[f32; 2]> = (0..127)
        .map(|i| [(i as f32 * 0.13).sin(), (i as f32 * 0.17).cos() * 0.5])
        .collect();
    for f in &input {
        for &x in f {
            wav.write_sample(x).unwrap();
        }
    }
    wav.finalize().unwrap();
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let d = wait(&mut s, job)["result"].clone();
    let o = owner(&d);
    let p = edit(
        &mut s,
        &o,
        json!({"action":"add","kind":"transient_processor"}),
    );
    let id = p["committed"]["effects"][0]["id"].clone();
    let v = [127, 93, 41, 113, 2, 0, 51, 129, 1];
    for (i, key) in wavebob::dsp::transient_processor::IDS.iter().enumerate() {
        edit(
            &mut s,
            &o,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":v[i]}),
        );
    }
    call(&mut s, "transport.loop.set", json!({"enabled":true}));
    call(&mut s, "transport.play", json!({"document_id":d["id"]}));
    ready(&mut s, &mut output);
    let mut live = vec![[0.0; 2]; 2032];
    for chunk in live.chunks_mut(7) {
        output.render_into(chunk);
    }
    let mut left: Vec<_> = (0..2032).map(|i| input[i % 127][0]).collect();
    let mut right: Vec<_> = (0..2032).map(|i| input[i % 127][1]).collect();
    let mut r = reference::TransientProcessor::with_settings(48000, &reference::Settings { v });
    for (l, rpart) in left.chunks_mut(1).zip(right.chunks_mut(1)) {
        r.process(l, rpart);
    }
    for (i, f) in live.iter().enumerate() {
        assert!((f[0] - left[i]).abs() <= 2e-6);
        assert!((f[1] - right[i]).abs() <= 2e-6);
    }
}
