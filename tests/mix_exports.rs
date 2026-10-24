use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn send(session: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":"mix-output","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(session: &mut Session, name: &str, params: Value) -> Value {
    let value = send(session, name, params);
    assert_eq!(value["status"], "ok", "{value}");
    value["result"].clone()
}
fn finish(session: &mut Session, job: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let job = ok(session, "jobs.get", json!({"job_id":job["id"]}));
        if job["status"] != "running" {
            return job;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn succeeded(session: &mut Session, job: Value) -> Value {
    let job = finish(session, job);
    assert_eq!(job["status"], "succeeded", "{job}");
    job
}
fn fixture() -> (Session, tempfile::TempDir, Value, Value) {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let mut docs = Vec::new();
    for (index, (rate, channels)) in [(24000, 1), (48000, 2)].into_iter().enumerate() {
        let path = dir.path().join(format!("Source {index}.wav"));
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
        for frame in 0..if index == 0 { 4 } else { 8 } {
            for channel in 0..channels {
                writer
                    .write_sample(
                        (frame as f32 + 1.0) * 0.03125 * if channel == 0 { 1.0 } else { -1.0 },
                    )
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
        let opened = ok(&mut session, "documents.open", json!({"path":path}));
        docs.push(succeeded(&mut session, opened)["result"].clone());
    }
    let arr = ok(&mut session, "tracks.add", json!({"name":"Mixed sources"}));
    let track = arr["tracks"][0]["id"].clone();
    for (doc, position) in docs.iter().zip([2, 5]) {
        ok(
            &mut session,
            "clips.insert",
            json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":position}),
        );
    }
    let arr = ok(&mut session, "arrangement.get", json!({}));
    let clip = &arr["tracks"][0]["clips"][1];
    let edited = ok(
        &mut session,
        "clips.effects.apply",
        json!({"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":{"kind":"fade_out"}}),
    );
    succeeded(&mut session, edited);
    ok(
        &mut session,
        "tracks.update",
        json!({"track_id":track,"gain_db":-3.0,"pan":0.25}),
    );
    let arr = ok(&mut session, "arrangement.get", json!({}));
    (session, dir, arr, docs[0].clone())
}
fn request(arr: &Value, path: &std::path::Path, format: Value, rate: u32) -> Value {
    json!({"revision_id":arr["revision_id"],"start_frame":1,"end_frame":14,"path":path,"format":format,"sample_rate":rate})
}
#[test]
fn device_free_mix_output_matches_offline_pcm_and_reports_exact_range() {
    for (encoding, bits, format, tolerance) in [
        ("float32", 32, hound::SampleFormat::Float, 0.0),
        ("pcm16", 16, hound::SampleFormat::Int, 1.0 / 32768.0),
        ("pcm24", 24, hound::SampleFormat::Int, 1.0 / 8388608.0),
        ("pcm32", 32, hound::SampleFormat::Int, 1.0 / 2147483648.0),
    ] {
        let (mut session, dir, arr, _) = fixture();
        let path = dir.path().join("Mix.wav");
        let prepared = ok(
            &mut session,
            "mix.prepare",
            json!({"revision_id":arr["revision_id"]}),
        );
        succeeded(&mut session, prepared);
        let expected = ok(
            &mut session,
            "mix.samples",
            json!({"revision_id":arr["revision_id"],"start_frame":1,"frames":13}),
        );
        let started = ok(
            &mut session,
            "mix.export",
            request(
                &arr,
                &path,
                json!({"kind":"wav","encoding":encoding}),
                48000,
            ),
        );
        let job = succeeded(&mut session, started);
        let summary = &job["output"]["details"];
        assert_eq!(summary["source_start_frame"], 1);
        assert_eq!(summary["source_end_frame"], 14);
        assert_eq!(summary["frames"], 13);
        assert_eq!(summary["channels"], 2);
        assert_eq!(summary["source_revision_id"], arr["revision_id"]);
        let mut reader = hound::WavReader::open(path).unwrap();
        assert_eq!(reader.spec().bits_per_sample, bits);
        assert_eq!(reader.spec().sample_format, format);
        let actual: Vec<f64> = if format == hound::SampleFormat::Float {
            reader
                .samples::<f32>()
                .map(|value| value.unwrap() as f64)
                .collect()
        } else {
            reader
                .samples::<i32>()
                .map(|value| value.unwrap() as f64 / 2.0f64.powi(bits as i32 - 1))
                .collect()
        };
        for frame in 0..13 {
            for channel in 0..2 {
                assert!(
                    (actual[frame * 2 + channel]
                        - expected["channels"][channel][frame].as_f64().unwrap())
                    .abs()
                        <= tolerance
                );
            }
        }
        assert_eq!(
            session.transport_snapshot().output.status,
            wavebob::transport::OutputStatus::Disabled
        );
    }
}
#[test]
fn mix_export_snapshot_survives_concurrent_arrangement_edits_without_marking_saved() {
    let (mut session, dir, arr, _) = fixture();
    let old = dir.path().join("Original.wav");
    let baseline = ok(
        &mut session,
        "mix.export",
        request(
            &arr,
            &old,
            json!({"kind":"wav","encoding":"float32"}),
            48000,
        ),
    );
    succeeded(&mut session, baseline);
    let path = dir.path().join("Captured.wav");
    let started = ok(
        &mut session,
        "mix.export",
        request(
            &arr,
            &path,
            json!({"kind":"wav","encoding":"float32"}),
            48000,
        ),
    );
    ok(
        &mut session,
        "tracks.update",
        json!({"track_id":arr["tracks"][0]["id"],"mute":true}),
    );
    let job = succeeded(&mut session, started);
    assert_eq!(
        job["output"]["details"]["source_revision_id"],
        arr["revision_id"]
    );
    assert_ne!(
        session.snapshot().arrangement.revision_id,
        arr["revision_id"]
    );
    assert!(session.snapshot().arrangement.dirty);
    assert_eq!(std::fs::read(path).unwrap(), std::fs::read(old).unwrap());
}
#[test]
fn failed_cancelled_and_protected_mix_outputs_preserve_destinations() {
    let (mut session, dir, arr, doc) = fixture();
    let existing = dir.path().join("Existing.wav");
    std::fs::write(&existing, b"retained").unwrap();
    let started = ok(
        &mut session,
        "mix.export",
        request(&arr, &existing, json!({"kind":"wav"}), 48000),
    );
    assert_eq!(
        finish(&mut session, started)["error"]["code"],
        "output_exists"
    );
    assert_eq!(std::fs::read(existing).unwrap(), b"retained");
    let source = std::path::Path::new(doc["path"].as_str().unwrap());
    let before = std::fs::read(source).unwrap();
    let mut params = request(&arr, source, json!({"kind":"wav"}), 48000);
    params["overwrite"] = json!(true);
    let started = ok(&mut session, "mix.export", params);
    assert_eq!(
        finish(&mut session, started)["error"]["code"],
        "invalid_params"
    );
    assert_eq!(std::fs::read(source).unwrap(), before);
    let path = dir.path().join("Cancelled.mp3");
    let started = ok(
        &mut session,
        "mix.export",
        request(&arr, &path, json!({"kind":"mp3"}), 48000),
    );
    ok(&mut session, "jobs.cancel", json!({"job_id":started["id"]}));
    drop(session);
    assert!(!path.exists());
}
#[test]
fn mp3_mix_export_uses_gapless_stereo_encoding_and_shared_output_conversion() {
    for rate in [32000, 44100, 48000] {
        let (mut session, dir, arr, _) = fixture();
        let path = dir.path().join("Mix.mp3");
        let started = ok(
            &mut session,
            "mix.export",
            request(&arr, &path, json!({"kind":"mp3","bitrate_kbps":192}), rate),
        );
        let summary = succeeded(&mut session, started)["output"]["details"].clone();
        let intended = wavebob::exports::converted_frames(13, 48000, rate).unwrap();
        assert_eq!(summary["frames"], intended);
        assert_eq!(summary["lossy"], true);
        assert!(summary["encoder_delay"].as_u64().is_some());
        let opened = ok(&mut session, "documents.open", json!({"path":path}));
        let reimported = succeeded(&mut session, opened)["result"].clone();
        assert_eq!(reimported["frames"], intended);
        assert_eq!(reimported["sample_rate"], rate);
        assert_eq!(reimported["channels"], 2);
    }
}

#[test]
#[ignore = "requires independent FFmpeg decoder for codec QA"]
fn independent_decoder_verifies_resampled_crossfaded_processed_mp3_mix() {
    let ffmpeg = std::env::var_os("WAVEBOB_QA_FFMPEG").expect("Set WAVEBOB_QA_FFMPEG");
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let mut docs = Vec::new();
    for (index, (rate, channels)) in [(48000, 1), (44100, 2)].into_iter().enumerate() {
        let path = dir.path().join(format!("Tone {index}.wav"));
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
        for frame in 0..rate * 2 {
            let t = frame as f64 / rate as f64;
            for channel in 0..channels {
                writer
                    .write_sample(
                        ((t * (220.0 + index as f64 * 110.0 + channel as f64 * 110.0)
                            * std::f64::consts::TAU)
                            .sin()
                            * 0.1) as f32,
                    )
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
        let opened = ok(&mut session, "documents.open", json!({"path":path}));
        docs.push(succeeded(&mut session, opened)["result"].clone());
    }
    let arr = ok(&mut session, "tracks.add", json!({}));
    let track = arr["tracks"][0]["id"].clone();
    for (doc, position) in docs.iter().zip([0, 48000]) {
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
    succeeded(&mut session, started);
    ok(
        &mut session,
        "tracks.update",
        json!({"track_id":track,"gain_db":-2.0,"pan":-0.2}),
    );
    let arr = ok(&mut session, "arrangement.get", json!({}));
    let mut records = Vec::new();
    for (rate, bitrate) in [(32000, 128), (44100, 192), (48000, 320)] {
        let wav = dir.path().join(format!("Reference {rate}.wav"));
        let mp3 = dir.path().join(format!("Mix {rate}.mp3"));
        let params = |path: &std::path::Path, format: Value| json!({"revision_id":arr["revision_id"],"start_frame":24000,"end_frame":120000,"path":path,"format":format,"sample_rate":rate});
        let started = ok(
            &mut session,
            "mix.export",
            params(&wav, json!({"kind":"wav","encoding":"float32"})),
        );
        succeeded(&mut session, started);
        let started = ok(
            &mut session,
            "mix.export",
            params(&mp3, json!({"kind":"mp3","bitrate_kbps":bitrate})),
        );
        let summary = succeeded(&mut session, started)["output"]["details"].clone();
        let expected = hound::WavReader::open(wav)
            .unwrap()
            .samples::<f32>()
            .map(|value| value.unwrap() as f64)
            .collect::<Vec<_>>();
        let mut command = std::process::Command::new(&ffmpeg);
        command.args(["-v", "error", "-i"]).arg(&mp3).args([
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
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = output
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes) as f64)
            .collect::<Vec<_>>();
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual.len() as u64 / 2, summary["frames"]);
        let mut squared = 0.0;
        let mut max_error = 0.0f64;
        let mut dot = 0.0;
        let mut aa = 0.0;
        let mut bb = 0.0;
        let mut count = 0;
        for index in 2048..actual.len() - 2048 {
            let error = (actual[index] - expected[index]).abs();
            squared += error * error;
            max_error = max_error.max(error);
            dot += actual[index] * expected[index];
            aa += actual[index] * actual[index];
            bb += expected[index] * expected[index];
            count += 1;
        }
        let rmse = (squared / count as f64).sqrt();
        let correlation = dot / (aa * bb).sqrt();
        assert!(
            rmse < 0.01 && max_error < 0.06 && correlation > 0.98,
            "{rate}/{bitrate}: {rmse}/{max_error}/{correlation}"
        );
        records.push(json!({"sample_rate":rate,"bitrate_kbps":bitrate,"frames":actual.len()/2,"rmse":rmse,"max_error":max_error,"correlation":correlation,"summary":summary}));
    }
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("encoder-mix-qa.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
    }
}
