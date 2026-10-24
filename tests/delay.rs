use serde_json::{Value, json};
use wavebob::{Session, protocol, transport::ManualOutput};
use wavebob_effect_reference::delay as reference;
fn call(s: &mut Session, name: &str, params: Value) -> Value {
    let r = serde_json::to_value(protocol::dispatch_line(
        s,
        &json!({"id":"delay","command":name,"params":params}).to_string(),
    ))
    .unwrap();
    assert_eq!(r["status"], "ok", "{r}");
    r["result"].clone()
}
fn wait(s: &mut Session, j: Value) -> Value {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let j = call(s, "jobs.get", json!({"job_id":j["id"]}));
        if j["status"] != "running" {
            assert_eq!(j["status"], "succeeded", "{j}");
            return j;
        }
        assert!(std::time::Instant::now() < end);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn edit(s: &mut Session, o: &Value, c: Value) -> Value {
    let p = call(s, "processing.get", json!({"owner":o}));
    call(
        s,
        "processing.edit",
        json!({"owner":o,"expected_edit_revision":p["committed"]["edit_revision_id"],"change":c}),
    )
}
fn ready(s: &mut Session, out: &mut ManualOutput) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        out.render_into(&mut []);
        let t = call(s, "transport.get", json!({}));
        assert!(t["processing"]["preparation_error"].is_null(), "{t}");
        if t["requested"]["generation"] == t["applied"]["generation"] {
            return;
        }
        assert!(std::time::Instant::now() < end);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn open(
    s: &mut Session,
    path: &std::path::Path,
    rate: u32,
    mono: bool,
    frames: usize,
    silent: bool,
) -> Value {
    let mut w = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: if mono { 1 } else { 2 },
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for i in 0..frames {
        let x = if silent {
            0.0
        } else if i == 0 {
            0.8
        } else if i < frames / 8 {
            (i as f32 * 0.173).sin() * 0.3
        } else {
            0.0
        };
        w.write_sample(x).unwrap();
        if !mono {
            w.write_sample(-x * 0.37).unwrap();
        }
    }
    w.finalize().unwrap();
    let j = call(s, "documents.open", json!({"path":path}));
    wait(s, j)["result"].clone()
}
fn add(s: &mut Session, o: &Value, v: [i32; 26]) -> Value {
    let p = edit(s, o, json!({"action":"add","kind":"delay"}));
    let id = p["committed"]["effects"][0]["id"].clone();
    for (i, key) in wavebob::dsp::delay::IDS.iter().enumerate() {
        let value = if matches!(i, 2 | 3) {
            json!(v[i] != 0)
        } else {
            json!(v[i])
        };
        edit(
            s,
            o,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":value}),
        );
    }
    id
}
fn render(s: &mut Session, o: &Value) -> Value {
    let j = call(s, "processing.render", json!({"owner":o}));
    wait(s, j)["output"]["details"].clone()
}
fn samples(s: &mut Session, m: &Value, start: usize, frames: usize) -> Vec<[f32; 2]> {
    let v = call(
        s,
        "processing.samples",
        json!({"audio_revision_id":m["audio_revision_id"],"start_frame":start,"frames":frames}),
    );
    (0..frames)
        .map(|i| {
            [
                v["channels"][0][i].as_f64().unwrap() as f32,
                v["channels"][1][i].as_f64().unwrap() as f32,
            ]
        })
        .collect()
}
#[test]
fn native_reference_agrees_with_public_offline_manual_blocks_and_seek() {
    let dir = tempfile::tempdir().unwrap();
    for (rate, model) in [(44100, 0), (48000, 1), (96000, 2), (48000, 3)] {
        let (mut s, mut out) = Session::with_manual_output(rate).unwrap();
        let frames = rate as usize / 2;
        let d = open(
            &mut s,
            &dir.path().join(format!("{rate}-{model}.wav")),
            rate,
            false,
            frames,
            false,
        );
        let o = json!({"kind":"document","id":d["id"]});
        let mut v = wavebob::dsp::delay::DEFAULTS;
        v[1] = model;
        v[2] = 0;
        v[4] = 147;
        v[14] = 1800;
        v[9] = 4300;
        v[11] = 3200;
        v[12] = 3700;
        add(&mut s, &o, v);
        let metadata = render(&mut s, &o);
        assert_eq!(metadata["latency_frames"], 0);
        assert_eq!(metadata["channels"], 2);
        assert_eq!(metadata["tail_capped"], false);
        let mut reference =
            reference::Delay::with_settings(rate, &reference::Settings { v }, 120.0);
        reference.start();
        let expected: Vec<_> = (0..frames)
            .map(|i| {
                let x = if i == 0 {
                    0.8
                } else if i < frames / 8 {
                    (i as f32 * 0.173).sin() * 0.3
                } else {
                    0.0
                };
                let (l, r) = reference.frame(x, -x * 0.37);
                [l, r]
            })
            .collect();
        for start in (0..frames).step_by(4096) {
            let actual = samples(&mut s, &metadata, start, (frames - start).min(4096));
            for (a, e) in actual.iter().zip(&expected[start..]) {
                for c in 0..2 {
                    assert!((a[c] - e[c]).abs() <= 2e-6, "offline {model}/{start}");
                }
            }
        }
        for block in [1, 7, 64] {
            call(
                &mut s,
                "transport.seek",
                json!({"document_id":d["id"],"frame":0}),
            );
            call(&mut s, "transport.play", json!({"document_id":d["id"]}));
            ready(&mut s, &mut out);
            let mut actual = vec![[0.0; 2]; frames];
            for chunk in actual.chunks_mut(block) {
                out.render_into(chunk);
            }
            for (a, e) in actual.iter().zip(&expected) {
                for c in 0..2 {
                    assert!((a[c] - e[c]).abs() <= 2e-6, "manual {model}/{block}");
                }
            }
            call(&mut s, "transport.pause", json!({}));
            ready(&mut s, &mut out);
        }
        for start in [113, 2049] {
            call(
                &mut s,
                "transport.seek",
                json!({"document_id":d["id"],"frame":start}),
            );
            call(&mut s, "transport.play", json!({"document_id":d["id"]}));
            ready(&mut s, &mut out);
            let mut actual = [[0.0; 2]; 256];
            out.render_into(&mut actual);
            for (a, e) in actual.iter().zip(&expected[start..]) {
                for c in 0..2 {
                    assert!((a[c] - e[c]).abs() <= 2e-6, "seek {model}/{start}");
                }
            }
            call(&mut s, "transport.pause", json!({}));
            ready(&mut s, &mut out);
        }
        assert_eq!(
            call(&mut s, "documents.get", json!({"document_id":d["id"]}))["revision_id"],
            d["revision_id"]
        );
    }
}
#[test]
fn tails_silence_bypass_muted_output_and_feedback_cap_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    for silent in [false, true] {
        let d = open(
            &mut s,
            &dir.path().join(format!("impulse-{silent}.wav")),
            8000,
            true,
            800,
            silent,
        );
        let o = json!({"kind":"document","id":d["id"]});
        let mut v = wavebob::dsp::delay::DEFAULTS;
        v[2] = 0;
        v[4] = 60;
        v[14] = 0;
        v[25] = 0;
        let id = add(&mut s, &o, v);
        let m = render(&mut s, &o);
        assert_eq!(m["channels"], 2);
        assert_eq!(m["tail_capped"], false);
        assert!(m["tail_frames"].as_u64().unwrap() < 16000);
        if silent {
            assert_eq!(m["tail_frames"], 0);
        } else {
            edit(
                &mut s,
                &o,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":"feedback","value":6000}),
            );
            let m = render(&mut s, &o);
            assert_eq!(m["tail_capped"], true);
            assert_eq!(m["tail_frames"], 480000);
            let last = samples(&mut s, &m, 800 + 480000 - 1, 1);
            assert_eq!(last[0], [0.0; 2]);
        }
        edit(
            &mut s,
            &o,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":"wet","value":0}),
        );
        assert_eq!(render(&mut s, &o)["tail_frames"], 0);
        edit(
            &mut s,
            &o,
            json!({"action":"bypass","effect_id":id,"bypassed":true}),
        );
        let m = render(&mut s, &o);
        assert_eq!(m["channels"], 2); // Stable prepared layout is retained while bypassed.
        assert_eq!(m["tail_frames"], 0);
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
    let added = edit(&mut session, &owner, json!({"action":"add","kind":"delay"}));
    let distortion = added["committed"]["effects"][0]["id"].clone();
    for (key, value) in [("drive", 5500), ("wet", 3500), ("type", 3)] {
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
fn owner(document: &Value) -> Value {
    json!({"kind":"document","id":document["id"]})
}

#[test]
fn loop_wrap_carries_echo_and_modulation_state_across_non_aligned_boundaries() {
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
        let added = edit(&mut session, &owner, json!({"action":"add","kind":"delay"}));
        let id = added["committed"]["effects"][0]["id"].clone();
        for (key, value) in [("time", 1), ("mod_time", 3000), ("diffusion_level", 4000)] {
            edit(
                &mut session,
                &owner,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":key,"value":value}),
            );
        }
        edit(
            &mut session,
            &owner,
            json!({"action":"set_parameter","effect_id":id,"parameter_id":"tempo_sync","value":false}),
        );
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
        for channel in 0..2 {
            let e = expected["channels"][channel][frame].as_f64().unwrap() as f32;
            assert!(
                (value[channel] - e).abs() <= 2e-6,
                "loop {frame}/{channel}: {} != {e}",
                value[channel]
            );
        }
    }
}
#[test]
fn twelve_second_silent_predelay_remains_active_and_tempo_does_not_move_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = open(
        &mut s,
        &dir.path().join("long-echo.wav"),
        8000,
        true,
        800,
        false,
    );
    let o = owner(&d);
    let mut v = wavebob::dsp::delay::DEFAULTS;
    v[4] = 768;
    v[14] = 0;
    v[6] = 0;
    v[25] = 0;
    add(&mut s, &o, v);
    call(
        &mut s,
        "transport.seek",
        json!({"document_id":d["id"],"frame":113}),
    );
    let before = call(&mut s, "processing.get", json!({"owner":o}));
    call(&mut s, "processing.tempo.set", json!({"bpm":20.0}));
    let after = call(&mut s, "processing.get", json!({"owner":o}));
    assert_ne!(before["render_identity"], after["render_identity"]);
    assert_eq!(
        call(&mut s, "documents.get", json!({"document_id":d["id"]}))["cursor"],
        113
    );
    let m = render(&mut s, &o);
    assert_eq!(m["latency_frames"], 0);
    assert_eq!(m["tail_capped"], false);
    assert!(m["tail_frames"].as_u64().unwrap() > 95000);
    let echo = samples(&mut s, &m, 95800, 512);
    assert!(echo.iter().flatten().any(|v| v.abs() > 0.001));
}
