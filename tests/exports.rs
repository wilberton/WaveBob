use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn send(s: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        s,
        &json!({"id":"export-test","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(s: &mut Session, name: &str, params: Value) -> Value {
    let response = send(s, name, params);
    assert_eq!(response["status"], "ok", "{response}");
    response["result"].clone()
}
fn wait(s: &mut Session, id: &Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let job = ok(s, "jobs.get", json!({"job_id":id}));
        if job["status"] != "running" {
            return job;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn imported(s: &mut Session, path: &std::path::Path) -> Value {
    let started = ok(s, "documents.open", json!({"path":path}));
    let job = wait(s, &started["id"]);
    assert_eq!(job["status"], "succeeded", "{job}");
    job["result"].clone()
}
fn source(
    s: &mut Session,
    dir: &std::path::Path,
    rate: u32,
    channels: u16,
    encoding: &str,
) -> Value {
    source_frames(s, dir, rate, channels, encoding, rate / 4 + 137)
}
fn source_frames(
    s: &mut Session,
    dir: &std::path::Path,
    rate: u32,
    channels: u16,
    encoding: &str,
    frames: u32,
) -> Value {
    let path = dir.join(format!("source-{rate}-{channels}-{encoding}.wav"));
    let bits = match encoding {
        "pcm16" => 16,
        "pcm24" => 24,
        _ => 32,
    };
    let mut w = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: if encoding == "float32" {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        },
    )
    .unwrap();
    for frame in 0..frames {
        for channel in 0..channels {
            let value = (frame as f64 * (440.0 + channel as f64 * 220.0) * std::f64::consts::TAU
                / rate as f64)
                .sin()
                * 0.5;
            if encoding == "float32" {
                w.write_sample(value as f32).unwrap();
            } else {
                w.write_sample((value * (1_u64 << (bits - 1)) as f64).round() as i32)
                    .unwrap();
            }
        }
    }
    w.finalize().unwrap();
    imported(s, &path)
}
fn target(doc: &Value) -> Value {
    json!({"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]})
}
#[test]
fn wav_exports_roundtrip_all_encodings_and_new_exports_default_to_24_bit() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = source(&mut s, d.path(), 48000, 2, "float32");
    let original = ok(
        &mut s,
        "documents.samples",
        json!({"document_id":doc["id"],"frames":4096}),
    )["channels"]
        .clone();
    for (encoding, tolerance) in [
        ("pcm16", 1.0 / 32768.0),
        ("pcm24", 1.0 / 8388608.0),
        ("pcm32", 1e-9),
        ("float32", 0.0),
    ] {
        let path = d.path().join(format!("output-{encoding}.wav"));
        let started = ok(
            &mut s,
            "audio.export",
            json!({"target":target(&doc),"path":path,"format":{"kind":"wav","encoding":encoding}}),
        );
        let job = wait(&mut s, &started["id"]);
        assert_eq!(job["status"], "succeeded", "{job}");
        assert_eq!(job["output"]["details"]["frames"], doc["frames"]);
        let reimport = imported(&mut s, &path);
        assert_eq!(reimport["encoding"], format!("wav_{encoding}"));
        let actual = ok(
            &mut s,
            "documents.samples",
            json!({"document_id":reimport["id"],"frames":4096}),
        )["channels"]
            .clone();
        for channel in 0..2 {
            for frame in 0..4096 {
                assert!(
                    (actual[channel][frame].as_f64().unwrap()
                        - original[channel][frame].as_f64().unwrap())
                    .abs()
                        <= tolerance,
                    "{encoding}: {channel}/{frame}"
                );
            }
        }
    }
    let started = ok(
        &mut s,
        "audio.export",
        json!({"target":target(&doc),"path":d.path().join("default.wav")}),
    );
    assert_eq!(
        wait(&mut s, &started["id"])["output"]["details"]["format"]["encoding"],
        "pcm24"
    );
    assert_eq!(
        ok(&mut s, "documents.get", json!({"document_id":doc["id"]}))["dirty"],
        false
    );
}
#[test]
fn mp3_all_rates_bitrates_and_channels_have_exact_gapless_duration() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    for channels in [1, 2] {
        let doc = source(&mut s, d.path(), 44100, channels, "float32");
        for rate in [32000, 44100, 48000] {
            for bitrate in [128, 192, 256, 320] {
                let path = d
                    .path()
                    .join(format!("out-{channels}-{rate}-{bitrate}.mp3"));
                let started = ok(
                    &mut s,
                    "audio.export",
                    json!({"target":target(&doc),"path":path,"format":{"kind":"mp3","bitrate_kbps":bitrate},"sample_rate":rate}),
                );
                let job = wait(&mut s, &started["id"]);
                assert_eq!(job["status"], "succeeded", "{job}");
                let summary = &job["output"]["details"];
                assert_eq!(summary["lossy"], true);
                assert!(summary["encoder_delay"].as_u64().unwrap() > 0);
                assert!(summary["encoder_padding"].as_u64().unwrap() > 0);
                let decoded = imported(&mut s, &path);
                assert_eq!(
                    decoded["frames"], summary["frames"],
                    "{channels}/{rate}/{bitrate}"
                );
                assert_eq!(decoded["sample_rate"], rate);
                assert_eq!(decoded["channels"], channels);
                assert_eq!(decoded["source_details"]["bitrate_mode"], "cbr");
                assert_eq!(decoded["source_details"]["bitrate_kbps"], bitrate);
                ok(
                    &mut s,
                    "documents.close",
                    json!({"document_id":decoded["id"]}),
                );
            }
        }
    }
}
#[test]
fn save_tracks_exact_revision_and_undo_redo_recompute_dirty_without_reencoding_working_pcm() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = source(&mut s, d.path(), 48000, 1, "pcm16");
    let edit = ok(
        &mut s,
        "effects.apply",
        json!({"target":target(&doc),"effect":{"kind":"gain","db":-3.0}}),
    );
    let edited = wait(&mut s, &edit["id"])["result"].clone();
    let started = ok(
        &mut s,
        "documents.save",
        json!({"document_id":doc["id"],"revision_id":edited["revision_id"],"overwrite":true}),
    );
    let saved = wait(&mut s, &started["id"]);
    assert_eq!(saved["status"], "succeeded", "{saved}");
    assert_eq!(saved["result"]["dirty"], false);
    assert_eq!(saved["output"]["details"]["format"]["encoding"], "pcm16");
    assert_eq!(saved["result"]["revision_id"], edited["revision_id"]);
    ok(&mut s, "history.undo", json!({}));
    assert_eq!(
        ok(&mut s, "documents.get", json!({"document_id":doc["id"]}))["dirty"],
        true
    );
    ok(&mut s, "history.redo", json!({}));
    assert_eq!(
        ok(&mut s, "documents.get", json!({"document_id":doc["id"]}))["dirty"],
        false
    );
    let before = ok(
        &mut s,
        "documents.samples",
        json!({"document_id":doc["id"],"frames":4096}),
    )["channels"]
        .clone();
    let started = ok(
        &mut s,
        "documents.save",
        json!({"document_id":doc["id"],"revision_id":edited["revision_id"],"path":d.path().join("saved.mp3"),"format":{"kind":"mp3","bitrate_kbps":192}}),
    );
    let saved = wait(&mut s, &started["id"]);
    assert_eq!(saved["status"], "succeeded", "{saved}");
    assert_eq!(saved["result"]["revision_id"], edited["revision_id"]);
    assert_eq!(
        ok(
            &mut s,
            "documents.samples",
            json!({"document_id":doc["id"],"frames":4096})
        )["channels"],
        before
    );
    assert_eq!(
        send(
            &mut s,
            "documents.save",
            json!({"document_id":doc["id"],"revision_id":edited["revision_id"],"overwrite":true})
        )["error"]["code"],
        "invalid_params"
    );
    assert_eq!(
        send(
            &mut s,
            "documents.save",
            json!({"document_id":doc["id"],"revision_id":edited["revision_id"],"overwrite":true,"format":{"kind":"mp3"}})
        )["error"]["code"],
        "invalid_params"
    );
    let started = ok(
        &mut s,
        "documents.save",
        json!({"document_id":doc["id"],"revision_id":edited["revision_id"],"path":d.path().join("converted.wav"),"format":{"kind":"wav","encoding":"pcm24"},"sample_rate":32000}),
    );
    assert_eq!(wait(&mut s, &started["id"])["status"], "succeeded");
    let started = ok(
        &mut s,
        "documents.save",
        json!({"document_id":doc["id"],"revision_id":edited["revision_id"],"overwrite":true}),
    );
    assert_eq!(
        wait(&mut s, &started["id"])["output"]["details"]["sample_rate"],
        32000,
        "Ordinary WAV Save retains its last chosen rate"
    );
}
#[test]
fn cancelled_and_failed_outputs_leave_destinations_and_dirty_state_unchanged() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = source(&mut s, d.path(), 48000, 1, "float32");
    let path = d.path().join("existing.wav");
    std::fs::write(&path, b"keep me").unwrap();
    for (path, overwrite, code) in [
        (path.clone(), false, "output_exists"),
        (d.path().join("missing/output.wav"), true, "io_error"),
    ] {
        let started = ok(
            &mut s,
            "audio.export",
            json!({"target":target(&doc),"path":path,"overwrite":overwrite}),
        );
        let job = wait(&mut s, &started["id"]);
        assert_eq!(job["status"], "failed");
        assert_eq!(job["error"]["code"], code);
    }
    let started = ok(
        &mut s,
        "audio.export",
        json!({"target":target(&doc),"path":path,"overwrite":true}),
    );
    ok(&mut s, "jobs.cancel", json!({"job_id":started["id"]}));
    assert_eq!(wait(&mut s, &started["id"])["status"], "cancelled");
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(std::fs::read(path).unwrap(), b"keep me");
    assert_eq!(
        ok(&mut s, "documents.get", json!({"document_id":doc["id"]}))["revision_id"],
        doc["revision_id"]
    );
}

#[test]
fn superseded_save_is_rejected_before_publication_and_never_cleans_a_newer_revision() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = source_frames(&mut s, d.path(), 48000, 1, "float32", 1_000_000);
    let path = d.path().join("stale.mp3");
    let saving = ok(
        &mut s,
        "documents.save",
        json!({"document_id":doc["id"],"revision_id":doc["revision_id"],"path":path,"format":{"kind":"mp3","bitrate_kbps":192}}),
    );
    let effect = ok(
        &mut s,
        "effects.apply",
        json!({"target":{"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":0,"end_frame":100},"effect":{"kind":"reverse"}}),
    );
    let edited = wait(&mut s, &effect["id"]);
    assert_eq!(edited["status"], "succeeded");
    let saved = wait(&mut s, &saving["id"]);
    assert_eq!(saved["status"], "failed", "{saved}");
    assert_eq!(saved["error"]["code"], "stale_revision");
    assert!(!path.exists());
    let current = ok(&mut s, "documents.get", json!({"document_id":doc["id"]}));
    assert_eq!(current["dirty"], true);
    assert_eq!(current["saved_revision_id"], doc["revision_id"]);
}
#[test]
fn output_overloads_require_explicit_clipping_and_float_wav_preserves_headroom() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("overload.wav");
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
    for sample in [-2.0_f32, 1.5, 0.5] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    let mut s = Session::default();
    let doc = imported(&mut s, &path);
    let started = ok(
        &mut s,
        "audio.export",
        json!({"target":target(&doc),"path":d.path().join("reject.wav")}),
    );
    assert_eq!(
        wait(&mut s, &started["id"])["error"]["code"],
        "invalid_params"
    );
    assert!(!d.path().join("reject.wav").exists());
    for (name, encoding, clip, expected) in [
        (
            "clip.wav",
            "pcm16",
            "clip",
            json!([[-1.0, 32767.0 / 32768.0, 0.5]]),
        ),
        (
            "preserve.wav",
            "float32",
            "reject",
            json!([[-2.0, 1.5, 0.5]]),
        ),
    ] {
        let path = d.path().join(name);
        let started = ok(
            &mut s,
            "audio.export",
            json!({"target":target(&doc),"path":path,"format":{"kind":"wav","encoding":encoding},"overload":clip}),
        );
        let job = wait(&mut s, &started["id"]);
        assert_eq!(job["status"], "succeeded", "{job}");
        assert_eq!(
            job["output"]["details"]["clipped_samples"],
            if encoding == "pcm16" { 2 } else { 0 }
        );
        let decoded = imported(&mut s, &path);
        assert_eq!(
            ok(
                &mut s,
                "documents.samples",
                json!({"document_id":decoded["id"],"frames":3})
            )["channels"],
            expected
        );
    }
}
#[test]
fn one_frame_mp3_export_recovers_one_frame_with_gapless_metadata() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = source_frames(&mut s, d.path(), 48000, 1, "float32", 1);
    let path = d.path().join("one-frame.mp3");
    let started = ok(
        &mut s,
        "audio.export",
        json!({"target":target(&doc),"path":path,"format":{"kind":"mp3"}}),
    );
    let job = wait(&mut s, &started["id"]);
    assert_eq!(job["status"], "succeeded", "{job}");
    assert_eq!(imported(&mut s, &path)["frames"], 1);
}

#[test]
fn dropping_a_session_cancels_uncommitted_output_and_cleans_temporary_files() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = source(&mut s, d.path(), 48000, 1, "float32");
    let output = d.path().join("abandoned.mp3");
    ok(
        &mut s,
        "audio.export",
        json!({"target":target(&doc),"path":output,"format":{"kind":"mp3"}}),
    );
    drop(s);
    assert!(!output.exists());
    let files = std::fs::read_dir(d.path())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        files.len(),
        1,
        "Only the source fixture may remain after the session closes"
    );
}

#[test]
#[ignore = "requires an independent FFmpeg executable in WAVEBOB_QA_FFMPEG"]
fn independent_decoder_checks_every_mp3_setting_and_selected_range_alignment() {
    let ffmpeg = std::env::var_os("WAVEBOB_QA_FFMPEG")
        .expect("Set WAVEBOB_QA_FFMPEG to the independent test decoder");
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let mut records = Vec::new();
    for channels in [1, 2] {
        let doc = source(&mut s, d.path(), 44100, channels, "float32");
        let start = 100;
        let end = doc["frames"].as_u64().unwrap() - 100;
        for rate in [32000, 44100, 48000] {
            for bitrate in [128, 192, 256, 320] {
                let path = d
                    .path()
                    .join(format!("independent-{channels}-{rate}-{bitrate}.mp3"));
                let started = ok(
                    &mut s,
                    "audio.export",
                    json!({"target":{"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":start,"end_frame":end},"path":path,"format":{"kind":"mp3","bitrate_kbps":bitrate},"sample_rate":rate}),
                );
                let job = wait(&mut s, &started["id"]);
                assert_eq!(job["status"], "succeeded", "{job}");
                let summary = &job["output"]["details"];
                let mut command = std::process::Command::new(&ffmpeg);
                command.args(["-v", "error", "-i"]).arg(&path).args([
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
                assert_eq!(output.stdout.len() % 4, 0);
                let values = output
                    .stdout
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| f32::from_le_bytes(*bytes) as f64)
                    .collect::<Vec<_>>();
                let frames = values.len() / channels as usize;
                assert_eq!(
                    frames as u64,
                    summary["frames"].as_u64().unwrap(),
                    "{channels}/{rate}/{bitrate}"
                );
                let mut squared = 0.0;
                let mut max_error = 0.0_f64;
                let mut count = 0;
                for frame in 256..frames - 256 {
                    for channel in 0..channels as usize {
                        let t = start as f64 / 44100.0 + frame as f64 / rate as f64;
                        let expected =
                            (t * (440.0 + channel as f64 * 220.0) * std::f64::consts::TAU).sin()
                                * 0.5;
                        let actual = values[frame * channels as usize + channel];
                        assert!(actual.is_finite());
                        let error = (actual - expected).abs();
                        squared += error * error;
                        max_error = max_error.max(error);
                        count += 1;
                    }
                }
                let rmse = (squared / count as f64).sqrt();
                assert!(
                    rmse < 0.02 && max_error < 0.07,
                    "{channels}/{rate}/{bitrate}: RMSE {rmse}, max {max_error}"
                );
                records.push(json!({"channels":channels,"sample_rate":rate,"bitrate_kbps":bitrate,"frames":frames,"rmse":rmse,"max_error":max_error,"summary":summary}));
            }
        }
    }
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("encoder-qa.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
    }
}
