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
fn staggered_crossfade_audio_endpoints_and_move_trim_history_follow_geometry() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, mut output) = Session::with_manual_output(48000).unwrap();
    let a = source(&mut session, dir.path(), "A.wav", 48000, 2, &[0.25; 16]);
    let b = source(&mut session, dir.path(), "B.wav", 48000, 2, &[0.75; 16]);
    let arr = call(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    insert(&mut session, &a, &track, 0);
    let arr = insert(&mut session, &b, &track, 4);
    let first = arr["tracks"][0]["clips"][0].clone();
    let second = arr["tracks"][0]["clips"][1].clone();
    let regions = call(&mut session, "arrangement.crossfades", json!({}));
    assert_eq!(regions["regions"][0]["start"], 4);
    assert_eq!(regions["regions"][0]["end"], 8);
    assert_eq!(regions["regions"][0]["kind"], "staggered");
    prepare(&mut session);
    let offline = samples(&mut session, 0, 12);
    for frame in 4..8 {
        let t = (frame - 4) as f64 / 3.0;
        near(
            offline["channels"][0][frame].as_f64().unwrap(),
            0.25 * (t * std::f64::consts::FRAC_PI_2).cos()
                + 0.75 * (t * std::f64::consts::FRAC_PI_2).sin(),
        );
    }
    call(&mut session, "mix.play", json!({}));
    let mut live = [[0.0; 2]; 12];
    output.render_into(&mut live);
    for (frame, value) in live.iter().enumerate() {
        assert_eq!(
            value[0] as f64,
            offline["channels"][0][frame].as_f64().unwrap()
        );
    }
    assert_eq!(
        call(
            &mut session,
            "clips.samples",
            json!({"clip_id":first["id"],"frames":8})
        )["channels"],
        json!(vec![vec![0.25; 8]; 2])
    );
    call(
        &mut session,
        "clips.move",
        json!({"clip_id":second["id"],"track_id":track,"position":8}),
    );
    assert!(
        call(&mut session, "arrangement.crossfades", json!({}))["regions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(&mut session, "mix.seek", json!({"frame":10}));
    prepare(&mut session);
    output.render_into(&mut []);
    assert_eq!(
        call(&mut session, "transport.get", json!({}))["applied"]["frame"],
        10
    );
    call(&mut session, "history.undo", json!({}));
    assert_eq!(
        call(&mut session, "arrangement.crossfades", json!({})),
        regions
    );
    call(&mut session, "history.redo", json!({}));
    assert!(
        call(&mut session, "arrangement.crossfades", json!({}))["regions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(&mut session, "history.undo", json!({}));
    let first = call(&mut session, "arrangement.get", json!({}))["tracks"][0]["clips"][0].clone();
    let started = call(
        &mut session,
        "clips.trim",
        json!({"clip_id":first["id"],"edit_revision_id":first["edit_revision_id"],"source_start":0,"source_end":4}),
    );
    job(&mut session, started);
    assert!(
        call(&mut session, "arrangement.crossfades", json!({}))["regions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(&mut session, "history.undo", json!({}));
    assert_eq!(
        call(&mut session, "arrangement.crossfades", json!({})),
        regions
    );
}

#[test]
fn containment_shared_edges_and_coincident_insertion_order_have_defined_audio() {
    for (outer_start, outer_frames, inner_start, inner_frames, kind) in [
        (0, 9, 2, 5, "contained"),
        (0, 9, 0, 5, "contained"),
        (0, 9, 4, 5, "contained"),
        (0, 5, 0, 5, "staggered"),
        (0, 3, 1, 1, "contained"),
        (0, 4, 1, 2, "contained"),
        (0, 2, 1, 2, "staggered"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::default();
        let outer = source(
            &mut session,
            dir.path(),
            "Outer.wav",
            48000,
            2,
            &vec![0.25; outer_frames * 2],
        );
        let inner = source(
            &mut session,
            dir.path(),
            "Inner.wav",
            48000,
            2,
            &vec![0.75; inner_frames * 2],
        );
        let arr = call(&mut session, "tracks.add", json!({}));
        let track = arr["tracks"][0]["id"].clone();
        let arr = insert(&mut session, &outer, &track, outer_start);
        let first = arr["tracks"][0]["clips"][0]["id"].clone();
        insert(&mut session, &inner, &track, inner_start);
        let data = call(&mut session, "arrangement.crossfades", json!({}));
        let region = &data["regions"][0];
        assert_eq!(region["kind"], kind);
        assert_eq!(region["first_clip_id"], first);
        prepare(&mut session);
        let mixed = samples(
            &mut session,
            0,
            (outer_start + outer_frames as u64).max(inner_start + inner_frames as u64) as u32,
        );
        let start = region["start"].as_u64().unwrap();
        let end = region["end"].as_u64().unwrap();
        for frame in start..end {
            let length = end - start;
            let expected = if length == 1 || (kind == "contained" && length == 2) {
                std::f64::consts::FRAC_1_SQRT_2
            } else {
                let t = (frame - start) as f64 / (length - 1) as f64;
                if kind == "contained" {
                    0.25 * (t * std::f64::consts::PI).cos().abs()
                        + 0.75 * (t * std::f64::consts::PI).sin()
                } else {
                    0.25 * (t * std::f64::consts::FRAC_PI_2).cos()
                        + 0.75 * (t * std::f64::consts::FRAC_PI_2).sin()
                }
            };
            near(
                mixed["channels"][0][frame as usize].as_f64().unwrap(),
                expected,
            );
        }
    }
    // A short clip inserted first still has the inner role when a later, longer
    // clip shares its start; insertion order remains independently observable.
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let inner = source(
        &mut session,
        dir.path(),
        "Short first.wav",
        48000,
        2,
        &[0.75; 10],
    );
    let outer = source(
        &mut session,
        dir.path(),
        "Long second.wav",
        48000,
        2,
        &[0.25; 18],
    );
    let arr = call(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    let arr = insert(&mut session, &inner, &track, 0);
    let first = arr["tracks"][0]["clips"][0]["id"].clone();
    insert(&mut session, &outer, &track, 0);
    let data = call(&mut session, "arrangement.crossfades", json!({}));
    assert_eq!(data["regions"][0]["first_clip_id"], first);
    assert_eq!(data["regions"][0]["contained_clip_id"], first);
    prepare(&mut session);
    let mixed = samples(&mut session, 0, 9);
    for (frame, expected) in [(0, 0.25), (2, 0.75), (4, 0.25), (8, 0.25)] {
        assert_eq!(mixed["channels"][0][frame].as_f64().unwrap(), expected);
    }
}

#[test]
fn third_simultaneous_clip_is_rejected_atomically_but_adjacent_regions_are_valid() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let doc = source(&mut session, dir.path(), "One.wav", 48000, 2, &[0.25; 16]);
    let arr = call(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    insert(&mut session, &doc, &track, 0);
    insert(&mut session, &doc, &track, 4);
    let before = session.snapshot();
    let history = call(&mut session, "history.get", json!({}));
    let rejected=serde_json::to_value(protocol::dispatch_line(&mut session,&json!({"id":"third","command":"clips.insert","params":{"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":8,"position":6}}).to_string())).unwrap();
    assert_eq!(rejected["error"]["code"], "invalid_params");
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("two simultaneous")
    );
    assert_eq!(session.snapshot(), before);
    assert_eq!(call(&mut session, "history.get", json!({})), history);
    insert(&mut session, &doc, &track, 8);
    assert_eq!(
        call(&mut session, "arrangement.crossfades", json!({}))["regions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
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
    assert_eq!(mixed["channels"][1], json!(vec![0.0; 4]));
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
        "clips.pcm.process",
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

#[test]
fn live_clip_envelopes_match_mix_reads_at_multiple_rates_and_block_sizes() {
    for rate in [44100, 48000, 96000] {
        for block in [1, 7, 64] {
            let dir = tempfile::tempdir().unwrap();
            let (mut session, mut output) = Session::with_manual_output(48000).unwrap();
            let pcm: Vec<f32> = (0..160).map(|n| n as f32 / 320.0).collect();
            let doc = source(&mut session, dir.path(), "range.wav", rate, 1, &pcm);
            let arr = call(&mut session, "tracks.add", json!({}));
            let arr = call(
                &mut session,
                "clips.insert",
                json!({"track_id":arr["tracks"][0]["id"],"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":16,"source_end":144,"position":3}),
            );
            let clip = &arr["tracks"][0]["clips"][0];
            let owner = json!({"kind":"clip","id":clip["id"]});
            let state = call(&mut session, "processing.get", json!({"owner":owner}));
            call(
                &mut session,
                "processing.edit",
                json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_envelope","envelope":{"gain_db":-6.020599913279624,"fade_in_frames":8,"fade_out_frames":12}}}),
            );
            prepare(&mut session);
            let frames = session.snapshot().arrangement.tracks[0].clips[0].frames as usize + 3;
            let expected = samples(&mut session, 0, frames as u32);
            call(&mut session, "mix.play", json!({}));
            let mut actual = vec![[0.0; 2]; frames];
            for chunk in actual.chunks_mut(block) {
                output.render_into(chunk);
            }
            for (n, frame) in actual.iter().enumerate() {
                assert_eq!(
                    frame[0] as f64,
                    expected["channels"][0][n].as_f64().unwrap(),
                    "rate {rate} block {block} frame {n}"
                );
                assert_eq!(frame[0], frame[1]);
            }
            let local = 20u64;
            let position = local as f64 * rate as f64 / 48000.0;
            let expected_level =
                ((16.0 + position) / 320.0) * 0.5 * std::f64::consts::FRAC_1_SQRT_2;
            near(actual[local as usize + 3][0] as f64, expected_level);
            assert_eq!(
                call(
                    &mut session,
                    "documents.samples",
                    json!({"document_id":doc["id"],"start_frame":16,"frames":1})
                )["channels"][0][0],
                pcm[16] as f64
            );
        }
    }
}

#[test]
fn clip_preview_preparation_is_audible_cancelable_and_does_not_commit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, mut output) = Session::with_manual_output(48000).unwrap();
    let doc = source(
        &mut session,
        dir.path(),
        "preview.wav",
        48000,
        2,
        &vec![0.5; 16384],
    );
    let arr = call(&mut session, "tracks.add", json!({}));
    let arr = insert(&mut session, &doc, &arr["tracks"][0]["id"], 0);
    let owner = json!({"kind":"clip","id":arr["tracks"][0]["clips"][0]["id"]});
    prepare(&mut session);
    call(&mut session, "mix.play", json!({}));
    output.render_into(&mut [[0.0; 2]; 64]);
    let revision = session.snapshot().arrangement.revision_id;
    let state = call(&mut session, "processing.get", json!({"owner":owner}));
    let preview = call(
        &mut session,
        "processing.preview.begin",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"]}),
    );
    call(
        &mut session,
        "processing.preview.update",
        json!({"preview_id":preview["preview_id"],"change":{"action":"set_envelope","envelope":{"gain_db":-6.020599913279624,"fade_in_frames":0,"fade_out_frames":0}}}),
    );
    prepare(&mut session);
    let mut frames = [[0.0; 2]; 600];
    output.render_into(&mut frames);
    near(frames[599][0] as f64, 0.25);
    assert_eq!(session.snapshot().arrangement.revision_id, revision);
    call(
        &mut session,
        "processing.preview.cancel",
        json!({"preview_id":preview["preview_id"]}),
    );
    prepare(&mut session);
    output.render_into(&mut frames);
    near(frames[599][0] as f64, 0.5);
    assert_eq!(session.snapshot().arrangement.revision_id, revision);
}
