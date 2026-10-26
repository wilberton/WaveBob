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

fn prepare(s: &mut Session) -> Value {
    let rev = s.snapshot().arrangement.revision_id;
    let job = call(s, "mix.prepare", json!({"revision_id":rev}));
    wait(s, job)
}
fn mixed(s: &mut Session, start: u64, frames: u32) -> Value {
    let revision = s.snapshot().arrangement.revision_id;
    call(
        s,
        "mix.samples",
        json!({"revision_id":revision,"start_frame":start,"frames":frames}),
    )["channels"]
        .clone()
}
#[test]
fn shared_full_and_trim_bounces_refresh_all_links_once_without_history_or_ripple() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Linked.wav");
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
    for _ in 0..1024 {
        wav.write_sample(0.4f32).unwrap();
        wav.write_sample(-0.2f32).unwrap();
    }
    wav.finalize().unwrap();
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let job = call(&mut s, "documents.open", json!({"path":path}));
    let d = wait(&mut s, job)["result"].clone();
    let o = owner(&d);
    let p = edit(&mut s, &o, json!({"action":"add","kind":"balance"}));
    let effect = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &o,
        json!({"action":"set_parameter","effect_id":effect,"parameter_id":"volume","value":320}),
    );
    let env = json!({"gain_db":-6.020599913279624,"fade_in_frames":512,"fade_out_frames":0});
    edit(&mut s, &o, json!({"action":"set_envelope","envelope":env}));
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    for (start, end, position) in [(0, 1024, 0), (0, 1024, 2048), (100, 500, 4096)] {
        call(
            &mut s,
            "clips.insert",
            json!({"track_id":track,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":start,"source_end":end,"position":position}),
        );
    }
    let before = s.snapshot().arrangement;
    let first = before.tracks[0].clips[0].id.clone();
    let clip = json!({"kind":"clip","id":first});
    edit(
        &mut s,
        &clip,
        json!({"action":"set_envelope","envelope":{"gain_db":-6.020599913279624,"fade_in_frames":0,"fade_out_frames":0}}),
    );
    let history = call(&mut s, "history.get", json!({}))["undo_count"].clone();
    let rendered = call(&mut s, "processing.render", json!({"owner":o}));
    let rendered = wait(&mut s, rendered)["output"]["details"].clone();
    let expected = call(
        &mut s,
        "processing.samples",
        json!({"audio_revision_id":rendered["audio_revision_id"],"start_frame":0,"frames":1024}),
    )["channels"]
        .clone();
    prepare(&mut s);
    let a = s.snapshot().arrangement;
    let clips = &a.tracks[0].clips;
    assert_eq!(
        clips[0].bounce.as_ref().unwrap().audio_revision_id,
        clips[1].bounce.as_ref().unwrap().audio_revision_id
    );
    assert_ne!(
        clips[1].bounce.as_ref().unwrap().audio_revision_id,
        clips[2].bounce.as_ref().unwrap().audio_revision_id
    );
    assert_eq!(clips[2].source_start, 100);
    assert_eq!(clips[2].frames, 400);
    assert_eq!(
        clips[0].processing.envelope.gain_db,
        Some(-6.020599913279624)
    );
    assert_eq!(
        call(&mut s, "history.get", json!({}))["undo_count"],
        history
    );
    let first = mixed(&mut s, 0, 1024);
    let second = mixed(&mut s, 2048, 1024);
    let trim = mixed(&mut s, 4096, 400);
    for c in 0..2 {
        for i in 0..1024 {
            let x = expected[c][i].as_f64().unwrap();
            assert!((first[c][i].as_f64().unwrap() - x * 0.5).abs() < 2e-6);
            assert!((second[c][i].as_f64().unwrap() - x).abs() < 2e-6);
        }
        for i in 0..400 {
            assert!(
                (trim[c][i].as_f64().unwrap() - expected[c][i + 100].as_f64().unwrap()).abs()
                    < 2e-6
            );
        }
    }
    let ids: Vec<_> = clips
        .iter()
        .map(|c| (&c.id, c.position, c.insertion_order))
        .collect();
    let old_asset = clips[0].bounce.as_ref().unwrap().audio_revision_id.clone();
    let _ = ids;
    edit(
        &mut s,
        &o,
        json!({"action":"set_envelope","envelope":{"gain_db":-12.041199826559248,"fade_in_frames":512,"fade_out_frames":0}}),
    );
    let stale=protocol::dispatch_line(&mut s,&json!({"id":"stale","command":"mix.samples","params":{"revision_id":a.revision_id,"start_frame":0,"frames":1}}).to_string());
    assert!(serde_json::to_value(stale).unwrap()["error"]["code"] == "busy");
    prepare(&mut s);
    let refreshed = s.snapshot().arrangement;
    assert_ne!(
        refreshed.tracks[0].clips[0]
            .bounce
            .as_ref()
            .unwrap()
            .audio_revision_id,
        old_asset
    );
    for (now, old) in refreshed.tracks[0]
        .clips
        .iter()
        .zip(&before.tracks[0].clips)
    {
        assert_eq!(now.id, old.id);
        assert_eq!(now.position, old.position);
        assert_eq!(now.insertion_order, old.insertion_order);
    }
    let after = mixed(&mut s, 2048, 1024);
    for c in 0..2 {
        for i in 0..1024 {
            assert!(
                (after[c][i].as_f64().unwrap() - second[c][i].as_f64().unwrap() * 0.5).abs() < 2e-6
            );
        }
    }
    call(&mut s, "mix.play", json!({}));
    ready(&mut s, &mut output);
    let mut actual = [[0.0; 2]; 256];
    output.render_into(&mut actual);
    let expected = mixed(&mut s, 0, 256);
    for (i, f) in actual.iter().enumerate() {
        for c in 0..2 {
            assert!((f[c] as f64 - expected[c][i].as_f64().unwrap()).abs() < 2e-6);
        }
    }
    assert_eq!(
        call(&mut s, "documents.get", json!({"document_id":d["id"]}))["revision_id"],
        d["revision_id"]
    );
}

fn open_stereo(
    s: &mut Session,
    path: &std::path::Path,
    frames: usize,
    signal: impl Fn(usize) -> [f32; 2],
) -> Value {
    let mut wav = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for i in 0..frames {
        for x in signal(i) {
            wav.write_sample(x).unwrap();
        }
    }
    wav.finalize().unwrap();
    let job = call(s, "documents.open", json!({"path":path}));
    wait(s, job)["result"].clone()
}
fn insert(s: &mut Session, track: &Value, d: &Value, position: u64) -> String {
    let a = call(
        s,
        "clips.insert",
        json!({"track_id":track,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":0,"source_end":d["frames"],"position":position}),
    );
    a["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["clips"].as_array().unwrap())
        .find(|c| c["position"] == position && c["document_id"] == d["id"])
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
#[test]
fn intentional_document_tail_and_clip_lookahead_mix_past_a_later_unmoved_body() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let d = open_stereo(&mut s, &dir.path().join("Late.wav"), 64, |i| {
        if i == 0 { [0.0, 1.0] } else { [0.0; 2] }
    });
    let dry = open_stereo(&mut s, &dir.path().join("Later.wav"), 64, |_| [0.25; 2]);
    let o = owner(&d);
    let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_enhancer"}));
    let id = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &o,
        json!({"action":"set_parameter","effect_id":id,"parameter_id":"phase","value":248}),
    );
    edit(
        &mut s,
        &o,
        json!({"action":"set_envelope","envelope":{"gain_db":-6.020599913279624,"fade_in_frames":0,"fade_out_frames":0}}),
    );
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    let first = insert(&mut s, &track, &d, 0);
    let second = insert(&mut s, &track, &dry, 128);
    let clip = json!({"kind":"clip","id":first});
    edit(
        &mut s,
        &clip,
        json!({"action":"set_envelope","envelope":{"gain_db":-6.020599913279624,"fade_in_frames":0,"fade_out_frames":0}}),
    );
    let p = edit(&mut s, &clip, json!({"action":"add","kind":"limiter"}));
    let limiter = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &clip,
        json!({"action":"set_parameter","effect_id":limiter,"parameter_id":"att","value":10000}),
    );
    let job = prepare(&mut s);
    let prepared = &job["output"]["details"];
    assert!(prepared["frames"].as_u64().unwrap() > 658);
    let a = s.snapshot().arrangement;
    assert_eq!(a.frames(), 192);
    assert_eq!(a.tracks[0].clips[1].id, second);
    assert_eq!(a.tracks[0].clips[1].position, 128);
    assert_eq!(a.tracks[0].clips[0].frames, 64);
    assert!(a.tracks[0].clips[0].tail_frames > 594);
    assert_eq!(
        call(&mut s, "arrangement.crossfades", json!({}))["regions"],
        json!([])
    );
    let tap = mixed(&mut s, 658, 1);
    assert_eq!(tap[0][0], 0.0);
    assert!((tap[1][0].as_f64().unwrap() - 0.25).abs() < 2e-6);
    let render = call(&mut s, "processing.render", json!({"owner":clip}));
    let render = wait(&mut s, render)["output"]["details"].clone();
    assert_eq!(render["source_body_frames"], 64);
    assert_eq!(render["latency_frames"], 48000);
    let samples = call(
        &mut s,
        "processing.samples",
        json!({"audio_revision_id":render["audio_revision_id"],"start_frame":658,"frames":1}),
    );
    assert!((samples["channels"][1][0].as_f64().unwrap() - 0.25).abs() < 2e-6);
    call(&mut s, "mix.play", json!({}));
    ready(&mut s, &mut output);
    let mut live = vec![[0.0; 2]; 700];
    for chunk in live.chunks_mut(7) {
        output.render_into(chunk);
    }
    assert!((live[658][1] - 0.25).abs() < 2e-6);
    assert_eq!(live[128], [0.25; 2]);
    assert_eq!(
        call(&mut s, "transport.get", json!({}))["processing"]["latency_frames"],
        48000
    );
    call(
        &mut s,
        "tracks.update",
        json!({"track_id":track,"mute":true}),
    );
    let j = prepare(&mut s);
    assert_eq!(j["output"]["details"]["frames"], 0);
}

#[test]
fn every_effect_bounces_links_and_pins_mixed_rate_mono_without_source_changes() {
    for kind in wavebob::processing::EffectKind::ALL {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Every effect.wav");
        let mut wav = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 11025,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for i in 0..128 {
            wav.write_sample(if i == 0 {
                0.8f32
            } else {
                (i as f32 * 0.17).sin() * 0.2
            })
            .unwrap();
        }
        wav.finalize().unwrap();
        let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
        let job = call(&mut s, "documents.open", json!({"path":path}));
        let d = wait(&mut s, job)["result"].clone();
        let o = owner(&d);
        edit(&mut s, &o, json!({"action":"add","kind":kind}));
        let job = call(&mut s, "processing.render", json!({"owner":o}));
        let expected = wait(&mut s, job)["output"]["details"].clone();
        let reference = call(
            &mut s,
            "processing.samples",
            json!({"audio_revision_id":expected["audio_revision_id"],"start_frame":0,"frames":128}),
        )["channels"]
            .clone();
        let a = call(&mut s, "tracks.add", json!({}));
        let track = a["tracks"][0]["id"].clone();
        let first = insert(&mut s, &track, &d, 0);
        let second = insert(&mut s, &track, &d, 2048);
        prepare(&mut s);
        let a = s.snapshot().arrangement;
        let first_asset = a.tracks[0].clips[0].bounce.as_ref().unwrap();
        assert_eq!(
            first_asset.metadata.channels,
            expected["channels"].as_u64().unwrap() as u16
        );
        assert_eq!(
            first_asset.metadata.latency_frames,
            expected["latency_frames"]
        );
        let actual = call(
            &mut s,
            "clips.samples",
            json!({"clip_id":first,"start_frame":0,"frames":128}),
        )["channels"]
            .clone();
        assert_eq!(
            actual.as_array().unwrap().len(),
            reference.as_array().unwrap().len()
        );
        for (a, b) in actual
            .as_array()
            .unwrap()
            .iter()
            .zip(reference.as_array().unwrap())
        {
            for (x, y) in a.as_array().unwrap().iter().zip(b.as_array().unwrap()) {
                assert!(
                    (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 2e-6,
                    "{kind:?}"
                );
            }
        }
        let second_owner = json!({"kind":"clip","id":second});
        call(
            &mut s,
            "processing.make_independent",
            json!({"owner":second_owner}),
        );
        let pinned = call(
            &mut s,
            "clips.samples",
            json!({"clip_id":second,"start_frame":0,"frames":128}),
        )["channels"]
            .clone();
        assert_eq!(pinned, actual);
        let old_asset = s.snapshot().arrangement.tracks[0].clips[1]
            .bounce
            .as_ref()
            .unwrap()
            .audio_revision_id
            .clone();
        edit(
            &mut s,
            &o,
            json!({"action":"set_envelope","envelope":{"gain_db":-6.020599913279624,"fade_in_frames":0,"fade_out_frames":0}}),
        );
        prepare(&mut s);
        let a = s.snapshot().arrangement;
        assert_eq!(
            a.tracks[0].clips[1]
                .bounce
                .as_ref()
                .unwrap()
                .audio_revision_id,
            old_asset
        );
        assert_ne!(
            a.tracks[0].clips[0]
                .bounce
                .as_ref()
                .unwrap()
                .audio_revision_id,
            old_asset
        );
        assert_eq!(
            call(
                &mut s,
                "clips.samples",
                json!({"clip_id":second,"start_frame":0,"frames":128})
            )["channels"],
            pinned
        );
        assert_eq!(a.tracks[0].clips[1].position, 2048);
        call(&mut s, "mix.play", json!({}));
        ready(&mut s, &mut output);
        let mut live = [[0.0; 2]; 256];
        for chunk in live.chunks_mut(7) {
            output.render_into(chunk);
        }
        let offline = mixed(&mut s, 0, 256);
        for (i, f) in live.iter().enumerate() {
            for c in 0..2 {
                assert!(
                    (f[c] as f64 - offline[c][i].as_f64().unwrap()).abs() < 2e-6,
                    "{kind:?} mix {i}/{c}"
                );
            }
        }
        assert_eq!(
            call(&mut s, "documents.get", json!({"document_id":d["id"]}))["revision_id"],
            d["revision_id"]
        );
    }
}

fn wait_finished(s: &mut Session, id: &Value) -> Value {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let j = call(s, "jobs.get", json!({"job_id":id}));
        if j["status"] != "running" {
            return j;
        }
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[test]
fn source_growth_conflicts_block_all_links_and_explicit_keep_prior_detaches_without_ripple() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = open_stereo(&mut s, &dir.path().join("Grow.wav"), 128, |_| [0.2; 2]);
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    let first = insert(&mut s, &track, &d, 0);
    insert(&mut s, &track, &d, 128);
    insert(&mut s, &track, &d, 192);
    prepare(&mut s);
    let original = s.snapshot().arrangement;
    let old_assets: Vec<_> = original.tracks[0]
        .clips
        .iter()
        .map(|c| c.bounce.as_ref().unwrap().audio_revision_id.clone())
        .collect();
    call(
        &mut s,
        "edit.copy",
        json!({"target":{"scope":"whole","document_id":d["id"],"revision_id":d["revision_id"]}}),
    );
    let pasted = call(
        &mut s,
        "edit.paste",
        json!({"document_id":d["id"],"revision_id":d["revision_id"],"start_frame":128,"end_frame":128}),
    );
    let grown = wait(&mut s, pasted)["result"].clone();
    assert_eq!(grown["frames"], 256);
    let before = call(&mut s, "history.get", json!({}))["undo_count"].clone();
    let j = call(
        &mut s,
        "mix.prepare",
        json!({"revision_id":original.revision_id}),
    );
    let failed = wait_finished(&mut s, &j["id"]);
    assert_eq!(failed["status"], "failed");
    assert_eq!(
        failed["error"]["details"]["affected_clip_ids"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let a = s.snapshot().arrangement;
    for (clip, old) in a.tracks[0].clips.iter().zip(&old_assets) {
        assert_eq!(clip.frames, 128);
        assert_eq!(&clip.bounce.as_ref().unwrap().audio_revision_id, old);
    }
    assert_eq!(call(&mut s, "history.get", json!({}))["undo_count"], before);
    let denied=protocol::dispatch_line(&mut s,&json!({"id":"detach","command":"processing.make_independent","params":{"owner":{"kind":"clip","id":first}}}).to_string());
    assert_eq!(
        serde_json::to_value(denied).unwrap()["error"]["code"],
        "busy"
    );
    call(
        &mut s,
        "processing.make_independent",
        json!({"owner":{"kind":"clip","id":first},"keep_prior_source":true}),
    );
    prepare(&mut s);
    let a = s.snapshot().arrangement;
    assert!(!a.tracks[0].clips[0].linked);
    assert_eq!(a.tracks[0].clips[0].frames, 128);
    assert_eq!(a.tracks[0].clips[1].frames, 256);
    assert_eq!(a.tracks[0].clips[1].position, 128);
    assert_eq!(a.tracks[0].clips[2].position, 192);
    assert_eq!(
        call(&mut s, "clips.samples", json!({"clip_id":first,"frames":1}))["channels"][0][0],
        0.2f32 as f64
    );
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "history.undo", json!({}));
    prepare(&mut s);
    let a = s.snapshot().arrangement;
    assert!(
        a.tracks[0]
            .clips
            .iter()
            .all(|c| c.linked && c.frames == 128)
    );
    assert_eq!(
        a.tracks[0].clips[0]
            .bounce
            .as_ref()
            .unwrap()
            .audio_revision_id,
        old_assets[0]
    );
}
#[test]
fn crossfade_boundary_zero_suppresses_an_earlier_tail_and_audible_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = open_stereo(&mut s, &dir.path().join("Faded tail.wav"), 128, |i| {
        if i == 0 { [0.0, 1.0] } else { [0.0; 2] }
    });
    let later = open_stereo(&mut s, &dir.path().join("Overlap.wav"), 128, |_| [0.25; 2]);
    let o = owner(&d);
    let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_enhancer"}));
    let id = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &o,
        json!({"action":"set_parameter","effect_id":id,"parameter_id":"phase","value":248}),
    );
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    insert(&mut s, &track, &d, 0);
    insert(&mut s, &track, &later, 64);
    let j = prepare(&mut s);
    assert_eq!(j["output"]["details"]["frames"], 192);
    assert_eq!(s.snapshot().arrangement.tracks[0].clips[0].tail_frames, 0);
    assert_eq!(mixed(&mut s, 658, 1), json!([[0.0], [0.0]]));
    let x = call(&mut s, "arrangement.crossfades", json!({}));
    assert_eq!(x["regions"][0]["start"], 64);
    assert_eq!(x["regions"][0]["end"], 128);
}

#[test]
fn source_shrink_clamps_current_bounds_and_fades_but_undo_restores_trim_intent() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = open_stereo(&mut s, &dir.path().join("Shrink.wav"), 1000, |_| [0.2; 2]);
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    insert(&mut s, &track, &d, 0);
    let a = call(
        &mut s,
        "clips.insert",
        json!({"track_id":track,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":200,"source_end":800,"position":2048}),
    );
    let id = a["tracks"][0]["clips"][1]["id"].clone();
    let o = json!({"kind":"clip","id":id});
    edit(
        &mut s,
        &o,
        json!({"action":"set_envelope","envelope":{"gain_db":0.0,"fade_in_frames":600,"fade_out_frames":0}}),
    );
    prepare(&mut s);
    let j = call(
        &mut s,
        "edit.delete",
        json!({"target":{"scope":"range","document_id":d["id"],"revision_id":d["revision_id"],"start_frame":450,"end_frame":1000}}),
    );
    wait(&mut s, j);
    prepare(&mut s);
    let a = s.snapshot().arrangement;
    let c = &a.tracks[0].clips[1];
    assert_eq!((c.source_start, c.source_end, c.frames), (200, 450, 250));
    assert_eq!(c.processing.envelope.fade_in_frames, 250);
    assert_eq!(c.position, 2048);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(
        call(&mut s, "documents.get", json!({"document_id":d["id"]}))["frames"],
        1000
    );
    prepare(&mut s);
    let a = s.snapshot().arrangement;
    let c = &a.tracks[0].clips[1];
    assert_eq!((c.source_start, c.source_end, c.frames), (200, 800, 600));
    assert_eq!(c.processing.envelope.fade_in_frames, 600);
    assert_eq!(c.position, 2048);
    call(&mut s, "history.redo", json!({}));
    prepare(&mut s);
    edit(
        &mut s,
        &o,
        json!({"action":"set_envelope","envelope":{"gain_db":-6.020599913279624,"fade_in_frames":250,"fade_out_frames":0}}),
    );
    let current = call(&mut s, "documents.get", json!({"document_id":d["id"]}));
    call(
        &mut s,
        "edit.copy",
        json!({"target":{"scope":"whole","document_id":d["id"],"revision_id":current["revision_id"]}}),
    );
    let j = call(
        &mut s,
        "edit.paste",
        json!({"document_id":d["id"],"revision_id":current["revision_id"],"start_frame":450,"end_frame":450}),
    );
    wait(&mut s, j);
    prepare(&mut s);
    let a = s.snapshot().arrangement;
    let c = &a.tracks[0].clips[1];
    assert_eq!((c.source_start, c.source_end, c.frames), (200, 800, 600));
    assert_eq!(c.processing.envelope.fade_in_frames, 600);
    assert_eq!(c.processing.envelope.gain_db, Some(-6.020599913279624));
}

#[test]
fn cancelled_and_superseded_refreshes_keep_prior_assets_and_only_publish_current_sound() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = open_stereo(&mut s, &dir.path().join("Cancel.wav"), 1_000_000, |_| {
        [0.2; 2]
    });
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    let id = insert(&mut s, &track, &d, 0);
    prepare(&mut s);
    let old = s.snapshot().arrangement.tracks[0].clips[0]
        .bounce
        .as_ref()
        .unwrap()
        .audio_revision_id
        .clone();
    let o = owner(&d);
    let change = |s: &mut Session, gain: f64| {
        edit(
            s,
            &o,
            json!({"action":"set_envelope","envelope":{"gain_db":gain,"fade_in_frames":0,"fade_out_frames":0}}),
        );
    };
    change(&mut s, -6.0);
    let rev = s.snapshot().arrangement.revision_id;
    let j = call(&mut s, "mix.prepare", json!({"revision_id":rev}));
    call(&mut s, "jobs.cancel", json!({"job_id":j["id"]}));
    let j = wait_finished(&mut s, &j["id"]);
    assert_eq!(j["status"], "cancelled");
    assert_eq!(
        s.snapshot().arrangement.tracks[0].clips[0]
            .bounce
            .as_ref()
            .unwrap()
            .audio_revision_id,
        old
    );
    prepare(&mut s);
    change(&mut s, -12.0);
    let a = call(&mut s, "mix.prepare", json!({"revision_id":rev}));
    change(&mut s, -24.0);
    let b = call(&mut s, "mix.prepare", json!({"revision_id":rev}));
    assert_ne!(a["id"], b["id"]);
    let last = wait_finished(&mut s, &b["id"]);
    assert_eq!(last["status"], "succeeded", "{last}");
    let first = wait_finished(&mut s, &a["id"]);
    assert!(
        first["status"] == "cancelled" || first["error"]["code"] == "stale_revision",
        "{first}"
    );
    let samples = call(&mut s, "clips.samples", json!({"clip_id":id,"frames":1}));
    assert!(
        (samples["channels"][0][0].as_f64().unwrap() - 0.2 * 10f64.powf(-24.0 / 20.0)).abs() < 2e-6
    );
    assert_eq!(
        call(&mut s, "documents.get", json!({"document_id":d["id"]}))["revision_id"],
        d["revision_id"]
    );
}
#[test]
fn empty_trim_after_source_shrink_blocks_all_reference_publication() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = open_stereo(&mut s, &dir.path().join("Empty trim.wav"), 128, |_| {
        [0.2; 2]
    });
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    insert(&mut s, &track, &d, 0);
    let a = call(
        &mut s,
        "clips.insert",
        json!({"track_id":track,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":100,"source_end":128,"position":256}),
    );
    let trimmed = a["tracks"][0]["clips"][1]["id"].clone();
    prepare(&mut s);
    let before = s.snapshot().arrangement;
    let j = call(
        &mut s,
        "edit.delete",
        json!({"target":{"scope":"range","document_id":d["id"],"revision_id":d["revision_id"],"start_frame":80,"end_frame":128}}),
    );
    wait(&mut s, j);
    let j = call(
        &mut s,
        "mix.prepare",
        json!({"revision_id":before.revision_id}),
    );
    let j = wait_finished(&mut s, &j["id"]);
    assert_eq!(j["status"], "failed");
    assert_eq!(j["error"]["details"]["affected_clip_ids"], json!([trimmed]));
    assert_eq!(s.snapshot().arrangement.tracks, before.tracks);
}
#[test]
fn separately_bounded_tail_voices_and_total_voices_fail_explicitly() {
    for (count, tail) in [(65, true), (129, false)] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::default();
        let d = open_stereo(&mut s, &dir.path().join("Voices.wav"), 64, |i| {
            if i == 0 { [0.0, 1.0] } else { [0.0; 2] }
        });
        if tail {
            let o = owner(&d);
            let p = edit(&mut s, &o, json!({"action":"add","kind":"stereo_enhancer"}));
            let id = p["committed"]["effects"][0]["id"].clone();
            edit(
                &mut s,
                &o,
                json!({"action":"set_parameter","effect_id":id,"parameter_id":"phase","value":512}),
            );
        }
        let a = call(&mut s, "tracks.add", json!({}));
        let track = a["tracks"][0]["id"].clone();
        for i in 0..count {
            insert(&mut s, &track, &d, i * 64);
        }
        let before = s.snapshot().arrangement;
        let j = call(
            &mut s,
            "mix.prepare",
            json!({"revision_id":before.revision_id}),
        );
        let j = wait_finished(&mut s, &j["id"]);
        assert_eq!(j["status"], "failed");
        assert_eq!(j["error"]["code"], "resource_limit");
        assert!(j["error"]["message"].as_str().unwrap().contains(if tail {
            "64 simultaneous tail"
        } else {
            "128 prepared clip"
        }));
        assert_eq!(s.snapshot().arrangement.tracks, before.tracks);
    }
}
#[test]
fn streamed_clip_feedback_tail_reports_sixty_second_cap_and_final_taper() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let d = open_stereo(&mut s, &dir.path().join("Cap.wav"), 64, |i| {
        if i == 0 { [0.2; 2] } else { [0.0; 2] }
    });
    let a = call(&mut s, "tracks.add", json!({}));
    let track = a["tracks"][0]["id"].clone();
    let id = insert(&mut s, &track, &d, 0);
    let o = json!({"kind":"clip","id":id});
    let p = edit(&mut s, &o, json!({"action":"add","kind":"delay"}));
    let effect = p["committed"]["effects"][0]["id"].clone();
    edit(
        &mut s,
        &o,
        json!({"action":"set_parameter","effect_id":effect,"parameter_id":"feedback","value":6000}),
    );
    let j = prepare(&mut s);
    assert_eq!(j["output"]["details"]["frames"], 64 + 48000 * 60);
    let c = &s.snapshot().arrangement.tracks[0].clips[0];
    assert_eq!(c.tail_frames, 48000 * 60);
    assert!(c.tail_capped);
    assert_eq!(mixed(&mut s, 64 + 48000 * 60 - 1, 1), json!([[0.0], [0.0]]));
}
