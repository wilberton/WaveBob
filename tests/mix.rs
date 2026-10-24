use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn call(session: &mut Session, name: &str, params: Value) -> Value {
    let response = serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"mix","command":name,"params":params}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    response["result"].clone()
}
fn job(session: &mut Session, started: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let job = call(session, "jobs.get", json!({"job_id":started["id"]}));
        if job["status"] != "running" {
            assert_eq!(job["status"], "succeeded", "{job}");
            return job;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn source(
    session: &mut Session,
    dir: &std::path::Path,
    name: &str,
    rate: u32,
    channels: u16,
    values: &[f32],
) -> Value {
    let path = dir.join(name);
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for value in values {
        writer.write_sample(*value).unwrap();
    }
    writer.finalize().unwrap();
    let started = call(session, "documents.open", json!({"path":path}));
    job(session, started)["result"].clone()
}
fn insert(session: &mut Session, doc: &Value, track: &Value, position: u64) -> Value {
    call(
        session,
        "clips.insert",
        json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":position}),
    )
}
fn prepare(session: &mut Session) -> Value {
    let revision = session.snapshot().arrangement.revision_id;
    let started = call(session, "mix.prepare", json!({"revision_id":revision}));
    job(session, started)["output"]["details"].clone()
}
fn samples(session: &mut Session, start: u64, frames: u32) -> Value {
    let revision = session.snapshot().arrangement.revision_id;
    call(
        session,
        "mix.samples",
        json!({"revision_id":revision,"start_frame":start,"frames":frames}),
    )
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}
#[test]
fn prepared_mix_places_mixed_rate_sources_and_live_frames_equal_offline() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, mut output) = Session::with_manual_output(48000).unwrap();
    let mono = source(&mut session, dir.path(), "Mono.wav", 24000, 1, &[0.25; 4]);
    let stereo = source(
        &mut session,
        dir.path(),
        "Stereo.wav",
        48000,
        2,
        &[0.5, -0.25, 0.5, -0.25, 0.5, -0.25, 0.5, -0.25],
    );
    let arr = call(&mut session, "tracks.add", json!({}));
    let a = arr["tracks"][0]["id"].clone();
    let arr = call(&mut session, "tracks.add", json!({}));
    let b = arr["tracks"][1]["id"].clone();
    insert(&mut session, &mono, &a, 2);
    insert(&mut session, &stereo, &b, 4);
    prepare(&mut session);
    let offline = samples(&mut session, 0, 12);
    near(offline["channels"][0][0].as_f64().unwrap(), 0.0);
    for frame in 2..10 {
        near(
            offline["channels"][0][frame].as_f64().unwrap(),
            0.25 * std::f64::consts::FRAC_1_SQRT_2
                + if (4..8).contains(&frame) { 0.5 } else { 0.0 },
        );
    }
    call(&mut session, "mix.play", json!({}));
    let mut live = [[0.0; 2]; 12];
    output.render_into(&mut live);
    for (frame, values) in live.iter().enumerate() {
        for (channel, value) in values.iter().enumerate() {
            assert_eq!(
                *value as f64,
                offline["channels"][channel][frame].as_f64().unwrap()
            );
        }
    }
    let transport = call(&mut session, "transport.get", json!({}));
    assert_eq!(transport["applied"]["frame"], 10);
    assert_eq!(transport["applied"]["playing"], false);
    assert_eq!(
        call(
            &mut session,
            "documents.get",
            json!({"document_id":mono["id"]})
        )["revision_id"],
        mono["revision_id"]
    );
}
#[test]
fn mute_solo_pan_gain_master_and_overload_are_observable_without_a_device() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let mono = source(&mut session, dir.path(), "Mono.wav", 48000, 1, &[0.5; 8]);
    let stereo = source(
        &mut session,
        dir.path(),
        "Stereo.wav",
        48000,
        2,
        &[0.25, -0.5, 0.25, -0.5, 0.25, -0.5, 0.25, -0.5],
    );
    let arr = call(&mut session, "tracks.add", json!({}));
    let a = arr["tracks"][0]["id"].clone();
    let arr = call(&mut session, "tracks.add", json!({}));
    let b = arr["tracks"][1]["id"].clone();
    insert(&mut session, &mono, &a, 0);
    insert(&mut session, &stereo, &b, 0);
    call(
        &mut session,
        "tracks.update",
        json!({"track_id":a,"pan":-1.0,"gain_db":6.020599913279624}),
    );
    call(
        &mut session,
        "tracks.update",
        json!({"track_id":b,"pan":1.0}),
    );
    prepare(&mut session);
    let mixed = samples(&mut session, 0, 4);
    near(mixed["channels"][0][0].as_f64().unwrap(), 1.0);
    near(mixed["channels"][1][0].as_f64().unwrap(), -0.5);
    call(
        &mut session,
        "tracks.update",
        json!({"track_id":a,"solo":true,"mute":true}),
    );
    prepare(&mut session);
    let mixed = samples(&mut session, 0, 4);
    assert_eq!(mixed["channels"], json!(vec![vec![0.0; 4]; 2]));
    let state = call(&mut session, "mix.get", json!({}));
    assert_eq!(state["tracks"][0]["audible"], false);
    assert_eq!(state["tracks"][1]["audible"], false);
    call(
        &mut session,
        "tracks.update",
        json!({"track_id":a,"mute":false}),
    );
    call(
        &mut session,
        "mix.master.set",
        json!({"gain_db":6.020599913279624}),
    );
    prepare(&mut session);
    let mixed = samples(&mut session, 0, 4);
    near(mixed["channels"][0][0].as_f64().unwrap(), 2.0);
    assert_eq!(mixed["overloaded_samples"], 4);
    near(mixed["peak_level"].as_f64().unwrap(), 2.0);
}
#[test]
fn timeline_loops_silence_and_processed_clips_share_live_mixing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, mut output) = Session::with_manual_output(48000).unwrap();
    let doc = source(
        &mut session,
        dir.path(),
        "Stereo.wav",
        48000,
        2,
        &[0.25, 0.5, 0.25, 0.5, 0.25, 0.5, 0.25, 0.5],
    );
    let arr = call(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    let arr = insert(&mut session, &doc, &track, 2);
    let clip = &arr["tracks"][0]["clips"][0];
    let started = call(
        &mut session,
        "clips.effects.apply",
        json!({"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":{"kind":"gain","db":6.020599913279624}}),
    );
    job(&mut session, started);
    prepare(&mut session);
    call(
        &mut session,
        "arrangement.navigate",
        json!({"cursor":0,"selection":{"start":3,"end":9}}),
    );
    call(&mut session, "transport.loop.set", json!({"enabled":true}));
    call(&mut session, "mix.play", json!({}));
    let mut live = [[0.0; 2]; 12];
    output.render_into(&mut live);
    assert_eq!(
        live,
        [
            [0.5, 1.0],
            [0.5, 1.0],
            [0.5, 1.0],
            [0.0, 0.0],
            [0.0, 0.0],
            [0.0, 0.0],
            [0.5, 1.0],
            [0.5, 1.0],
            [0.5, 1.0],
            [0.0, 0.0],
            [0.0, 0.0],
            [0.0, 0.0]
        ]
    );
    call(&mut session, "transport.get", json!({}));
    assert_eq!(session.snapshot().arrangement.cursor, 3);
    call(&mut session, "mix.play_pause", json!({}));
    let mut silent = [[1.0; 2]; 4];
    output.render_into(&mut silent);
    assert_eq!(silent, [[0.0; 2]; 4]);
    call(&mut session, "mix.rewind", json!({}));
    output.render_into(&mut silent);
    call(&mut session, "transport.get", json!({}));
    assert_eq!(session.snapshot().arrangement.cursor, 0);
    assert_eq!(session.snapshot().arrangement.selection.unwrap().start, 3);
}

#[test]
fn preparing_play_can_be_paused_and_stale_inspection_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, mut output) = Session::with_manual_output(48000).unwrap();
    let doc = source(
        &mut session,
        dir.path(),
        "Long.wav",
        24000,
        1,
        &vec![0.125; 24000],
    );
    let arr = call(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    insert(&mut session, &doc, &track, 0);
    let started = call(&mut session, "mix.play", json!({}));
    assert_eq!(started["requested_playing"], true);
    call(&mut session, "mix.play_pause", json!({}));
    let revision = session.snapshot().arrangement.revision_id;
    let prepared = call(&mut session, "mix.prepare", json!({"revision_id":revision}));
    job(&mut session, prepared);
    let mut silence = [[1.0; 2]; 8];
    output.render_into(&mut silence);
    assert_eq!(silence, [[0.0; 2]; 8]);
    assert_eq!(
        call(&mut session, "mix.get", json!({}))["requested_playing"],
        false
    );
    let before = session.snapshot();
    let result = serde_json::to_value(protocol::dispatch_line(
        &mut session,
        &json!({"id":"bad","command":"mix.samples","params":{"revision_id":"old","frames":8}})
            .to_string(),
    ))
    .unwrap();
    assert_eq!(result["error"]["code"], "stale_revision");
    assert_eq!(session.snapshot(), before);
    let prepared = call(
        &mut session,
        "mix.prepare",
        json!({"revision_id":before.arrangement.revision_id}),
    );
    assert_eq!(prepared["status"], "succeeded");
}
