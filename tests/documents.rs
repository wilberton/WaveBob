use serde_json::{Value, json};
use wavebob::{Session, protocol};

fn send(session: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        session,
        &json!({"id":name,"command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn success(value: Value) -> Value {
    assert_eq!(value["status"], "ok", "{value}");
    value["result"].clone()
}
fn fixture(path: &std::path::Path, channels: u16, values: &[i16]) {
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for value in values {
        writer.write_sample(*value).unwrap();
    }
    writer.finalize().unwrap();
}
fn wait(session: &mut Session, id: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let job = success(send(session, "jobs.get", json!({"job_id":id})));
        if job["status"] != "running" {
            return job;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Import did not finish: {job}"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn open(session: &mut Session, path: &std::path::Path) -> Value {
    let started = success(send(session, "documents.open", json!({"path":path})));
    wait(session, started["id"].as_str().unwrap())
}

#[test]
fn stereo_import_has_stable_ids_exact_samples_and_real_waveform_bounds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stereo.wav");
    fixture(&path, 2, &[-32768, 32767, -16384, 16384, 0, 1]);
    let source = std::fs::read(&path).unwrap();
    let mut session = Session::default();
    let job = open(&mut session, &path);
    assert_eq!(job["status"], "succeeded");
    let document = &job["result"];
    let id = document["id"].as_str().unwrap();
    assert!(uuid::Uuid::parse_str(id).is_ok());
    assert!(uuid::Uuid::parse_str(document["revision_id"].as_str().unwrap()).is_ok());
    assert_eq!(document["frames"], 3);
    assert_eq!(document["channels"], 2);
    assert_eq!(document["sample_rate"], 48000);
    assert_eq!(document["encoding"], "wav_pcm16");
    assert_eq!(
        success(send(
            &mut session,
            "documents.get",
            json!({"document_id":id})
        )),
        *document
    );
    let samples = success(send(
        &mut session,
        "documents.samples",
        json!({"document_id":id,"frames":3}),
    ));
    assert_eq!(
        samples["channels"],
        json!([[-1.0, -0.5, 0.0], [32767.0 / 32768.0, 0.5, 1.0 / 32768.0]])
    );
    let peaks = success(send(
        &mut session,
        "documents.waveform",
        json!({"document_id":id,"bins":2}),
    ));
    assert_eq!(
        peaks["channels"][0][0],
        json!({"start_frame":0,"end_frame":1,"min":-1.0,"max":-1.0})
    );
    assert_eq!(
        peaks["channels"][0][1],
        json!({"start_frame":1,"end_frame":3,"min":-0.5,"max":0.0})
    );
    assert_eq!(session.snapshot().revision, 1);
    assert!(!session.snapshot().documents[0].dirty);
    assert_eq!(std::fs::read(&path).unwrap(), source);
    assert_eq!(
        send(
            &mut session,
            "documents.samples",
            json!({"document_id":id,"frames":4})
        )["error"]["code"],
        "invalid_params"
    );
    assert_eq!(
        send(
            &mut session,
            "documents.get",
            json!({"document_id":"unknown"})
        )["error"]["code"],
        "not_found"
    );
    assert_eq!(
        send(
            &mut session,
            "documents.waveform",
            json!({"document_id":id,"revision_id":"stale"})
        )["error"]["code"],
        "stale_revision"
    );
}

#[test]
fn multiple_imports_and_activation_preserve_distinct_audio_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let a = directory.path().join("a.wav");
    let b = directory.path().join("b.wav");
    fixture(&a, 1, &[0, 8192, -8192, 16384]);
    fixture(&b, 1, &[32767, 0, -32768]);
    let mut session = Session::default();
    let first = open(&mut session, &a)["result"].clone();
    let second = open(&mut session, &b)["result"].clone();
    assert_ne!(first["id"], second["id"]);
    assert_ne!(first["revision_id"], second["revision_id"]);
    assert_eq!(session.snapshot().documents.len(), 2);
    assert_eq!(
        session.snapshot().active_document_id.as_deref(),
        second["id"].as_str()
    );
    success(send(
        &mut session,
        "documents.activate",
        json!({"document_id":first["id"]}),
    ));
    assert_eq!(
        session.snapshot().active_document_id.as_deref(),
        first["id"].as_str()
    );
    let revision = session.snapshot().revision;
    success(send(
        &mut session,
        "documents.activate",
        json!({"document_id":first["id"]}),
    ));
    assert_eq!(session.snapshot().revision, revision);
    let samples = success(send(
        &mut session,
        "documents.samples",
        json!({"document_id":first["id"],"frames":4}),
    ));
    assert_eq!(samples["channels"], json!([[0.0, 0.25, -0.25, 0.5]]));
}

#[test]
fn malformed_missing_empty_unsupported_and_truncated_imports_publish_no_documents() {
    let directory = tempfile::tempdir().unwrap();
    let bad = directory.path().join("bad.wav");
    std::fs::write(&bad, "not a WAV").unwrap();
    let empty = directory.path().join("empty.wav");
    fixture(&empty, 1, &[]);
    let surround = directory.path().join("surround.wav");
    fixture(&surround, 3, &[0, 0, 0]);
    let truncated = directory.path().join("truncated.wav");
    fixture(&truncated, 1, &[0, 1, 2]);
    let mut bytes = std::fs::read(&truncated).unwrap();
    bytes.pop();
    std::fs::write(&truncated, bytes).unwrap();
    let float = directory.path().join("float.wav");
    let mut writer = hound::WavWriter::create(
        &float,
        hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    writer.write_sample(f32::NAN).unwrap();
    writer.finalize().unwrap();
    for (path, code) in [
        (directory.path().join("missing.wav"), "io_error"),
        (bad, "decode_failed"),
        (empty, "decode_failed"),
        (surround, "unsupported_format"),
        (float, "decode_failed"),
        (truncated, "decode_failed"),
    ] {
        let mut session = Session::default();
        let job = open(&mut session, &path);
        assert_eq!(job["status"], "failed", "{job}");
        assert_eq!(job["error"]["code"], code, "{job}");
        assert!(session.snapshot().documents.is_empty());
        assert_eq!(session.snapshot().revision, 0);
        assert_eq!(
            send(&mut session, "transport.loop.toggle", json!({}))["status"],
            "ok"
        );
    }
}

#[test]
fn cancellation_before_publication_never_creates_partial_documents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cancel.wav");
    fixture(&path, 1, &[0, 100, -100]);
    let mut session = Session::default();
    let job = success(send(&mut session, "documents.open", json!({"path":path})));
    let cancelled = success(send(
        &mut session,
        "jobs.cancel",
        json!({"job_id":job["id"]}),
    ));
    assert_eq!(cancelled["status"], "cancelled");
    std::thread::sleep(std::time::Duration::from_millis(10));
    let job = wait(&mut session, job["id"].as_str().unwrap());
    assert_eq!(job["status"], "cancelled");
    assert!(session.snapshot().documents.is_empty());
    assert_eq!(session.snapshot().revision, 0);
}

#[test]
fn multiresolution_peaks_match_exact_samples_across_unaligned_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ramp.wav");
    let samples: Vec<i16> = (0..1037)
        .map(|frame| ((frame * 193) % 65536 - 32768) as i16)
        .collect();
    fixture(&path, 1, &samples);
    let mut session = Session::default();
    let document = open(&mut session, &path)["result"].clone();
    for (start, end, bins) in [
        (0, 1037, 1),
        (17, 999, 7),
        (255, 1030, 43),
        (1000, 1037, 100),
    ] {
        let waveform = success(send(
            &mut session,
            "documents.waveform",
            json!({"document_id":document["id"],"start_frame":start,"end_frame":end,"bins":bins}),
        ));
        for peak in waveform["channels"][0].as_array().unwrap() {
            let first = peak["start_frame"].as_u64().unwrap() as usize;
            let last = peak["end_frame"].as_u64().unwrap() as usize;
            let min = *samples[first..last].iter().min().unwrap() as f64 / 32768.0;
            let max = *samples[first..last].iter().max().unwrap() as f64 / 32768.0;
            assert_eq!(peak["min"], min);
            assert_eq!(peak["max"], max);
        }
    }
}

#[cfg(windows)]
#[test]
fn inaccessible_audio_is_an_observable_failed_job() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("locked.wav");
    fixture(&path, 1, &[0, 1]);
    let _lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let mut session = Session::default();
    let job = open(&mut session, &path);
    assert_eq!(job["error"]["code"], "io_error");
    assert!(session.snapshot().documents.is_empty());
}

#[test]
fn all_supported_wav_encodings_preserve_levels_and_metadata() {
    let directory = tempfile::tempdir().unwrap();
    for channels in [1, 2] {
        for (kind, bits) in [
            (hound::SampleFormat::Int, 16),
            (hound::SampleFormat::Int, 24),
            (hound::SampleFormat::Int, 32),
            (hound::SampleFormat::Float, 32),
        ] {
            let path = directory
                .path()
                .join(format!("{channels}-{kind:?}-{bits}.wav"));
            let mut writer = hound::WavWriter::create(
                &path,
                hound::WavSpec {
                    channels,
                    sample_rate: 44100,
                    bits_per_sample: bits,
                    sample_format: kind,
                },
            )
            .unwrap();
            for value in [-1.0_f64, -0.5, 0.0, 0.5] {
                for _ in 0..channels {
                    if kind == hound::SampleFormat::Float {
                        writer.write_sample(value as f32).unwrap();
                    } else {
                        writer
                            .write_sample((value * 2.0_f64.powi(bits as i32 - 1)) as i32)
                            .unwrap();
                    }
                }
            }
            writer.finalize().unwrap();
            let mut session = Session::default();
            let job = open(&mut session, &path);
            assert_eq!(job["status"], "succeeded", "{job}");
            assert_eq!(job["result"]["frames"], 4);
            let encoding = format!(
                "wav_{}{bits}",
                if kind == hound::SampleFormat::Int {
                    "pcm"
                } else {
                    "float"
                }
            );
            assert_eq!(job["result"]["encoding"], encoding);
            let samples = success(send(
                &mut session,
                "documents.samples",
                json!({"document_id":job["result"]["id"],"frames":4}),
            ));
            for channel in samples["channels"].as_array().unwrap() {
                assert_eq!(*channel, json!([-1.0, -0.5, 0.0, 0.5]));
            }
        }
    }
}

#[test]
fn mp3_cbr_vbr_and_metadata_free_streams_match_independent_ffmpeg_pcm() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for fixture in manifest["fixtures"].as_array().unwrap() {
        let name = fixture["name"].as_str().unwrap();
        let mut session = Session::default();
        let job = open(&mut session, &root.join(format!("{name}.mp3")));
        assert_eq!(job["status"], "succeeded", "{name}: {job}");
        let doc = &job["result"];
        for field in ["channels", "sample_rate"] {
            assert_eq!(doc[field], fixture[field], "{name}");
        }
        assert_eq!(doc["frames"], fixture["decoded_frames"], "{name}");
        assert_eq!(doc["encoding"], "mp3");
        assert_eq!(doc["source_details"]["bitrate_mode"], fixture["mode"]);
        assert_eq!(
            !doc["source_details"]["encoder_delay"].is_null(),
            fixture["tags"].as_bool().unwrap()
        );
        let frames = doc["frames"].as_u64().unwrap();
        let channels = doc["channels"].as_u64().unwrap() as usize;
        let expected = std::fs::read(root.join(format!("{name}.f32"))).unwrap();
        let expected: Vec<f64> = expected
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk) as f64)
            .collect();
        let mut square = 0.0;
        let mut max = 0.0_f64;
        for start in (0..frames).step_by(4096) {
            let count = 4096.min(frames - start);
            let read = success(send(
                &mut session,
                "documents.samples",
                json!({"document_id":doc["id"],"start_frame":start,"frames":count}),
            ));
            for channel in 0..channels {
                for frame in 0..count as usize {
                    let difference = read["channels"][channel][frame].as_f64().unwrap()
                        - expected[(start as usize + frame) * channels + channel];
                    square += difference * difference;
                    max = max.max(difference.abs());
                }
            }
        }
        let rmse = (square / expected.len() as f64).sqrt();
        eprintln!("{name}: independent decode max={max}, RMSE={rmse}");
        assert!(
            max < 0.002 && rmse < 0.0001,
            "{name}: independent decode max={max}, RMSE={rmse}"
        );
    }
}

#[test]
fn malformed_and_truncated_mp3s_never_publish_partial_audio() {
    let original = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/cbr-1ch-44100-tagged.mp3"),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("bad.mp3", b"not audio".to_vec()),
        ("truncated.mp3", original[..original.len() - 1].to_vec()),
        ("short.mp3", original[..original.len() / 2].to_vec()),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        let mut session = Session::default();
        let job = open(&mut session, &path);
        assert_eq!(job["status"], "failed", "{job}");
        assert_eq!(job["error"]["code"], "decode_failed");
        assert!(session.snapshot().documents.is_empty());
    }
}

#[test]
fn close_releases_document_identity_and_reopening_cannot_reuse_stale_peaks() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("changing.wav");
    fixture(&path, 1, &[0, 16384, 0]);
    let mut session = Session::default();
    let first = open(&mut session, &path)["result"].clone();
    success(send(
        &mut session,
        "documents.close",
        json!({"document_id":first["id"]}),
    ));
    assert!(session.snapshot().documents.is_empty());
    assert_eq!(
        send(
            &mut session,
            "documents.waveform",
            json!({"document_id":first["id"]})
        )["error"]["code"],
        "not_found"
    );
    fixture(&path, 1, &[-16384, 0]);
    let second = open(&mut session, &path)["result"].clone();
    assert_ne!(first["id"], second["id"]);
    assert_ne!(first["revision_id"], second["revision_id"]);
    let wave = success(send(
        &mut session,
        "documents.waveform",
        json!({"document_id":second["id"],"bins":1}),
    ));
    assert_eq!(wave["channels"][0][0]["min"], -0.5);
    assert_eq!(wave["channels"][0][0]["max"], 0.0);
}

#[test]
fn oversized_declared_audio_fails_before_allocating_or_publishing_a_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("too-large.wav");
    fixture(&path, 1, &[0]);
    let mut bytes = std::fs::read(&path).unwrap();
    let data = 600 * 1024 * 1024_u32;
    bytes[4..8].copy_from_slice(&(data + 36).to_le_bytes());
    bytes[40..44].copy_from_slice(&data.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    let mut session = Session::default();
    let job = open(&mut session, &path);
    assert_eq!(job["status"], "failed", "{job}");
    assert_eq!(job["error"]["code"], "resource_limit");
    assert!(session.snapshot().documents.is_empty());
}

#[test]
fn exact_cursor_selection_all_clear_and_stale_navigation_preserve_audio() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection.wav");
    fixture(&path, 2, &[0; 24]);
    let mut session = Session::default();
    let document = open(&mut session, &path)["result"].clone();
    let id = document["id"].clone();
    let range = success(send(
        &mut session,
        "documents.selection.set",
        json!({"document_id":id,"start_frame":9,"end_frame":3}),
    ));
    assert_eq!(range["selection"], json!({"start":3,"end":9}));
    assert_eq!(range["cursor"], 3);
    assert_eq!(range["revision_id"], document["revision_id"]);
    assert_eq!(range["dirty"], false);
    let cleared = success(send(
        &mut session,
        "documents.selection.clear",
        json!({"document_id":id}),
    ));
    assert!(cleared["selection"].is_null());
    assert_eq!(cleared["cursor"], 3);
    let before = session.snapshot();
    success(send(
        &mut session,
        "documents.selection.clear",
        json!({"document_id":id}),
    ));
    assert_eq!(session.snapshot(), before);
    let all = success(send(
        &mut session,
        "documents.selection.all",
        json!({"document_id":id}),
    ));
    assert_eq!(all["selection"], json!({"start":0,"end":12}));
    assert_eq!(all["cursor"], 0);
    let one = success(send(
        &mut session,
        "documents.selection.set",
        json!({"document_id":id,"start_frame":11,"end_frame":12}),
    ));
    assert_eq!(one["selection"], json!({"start":11,"end":12}));
    let empty = success(send(
        &mut session,
        "documents.selection.set",
        json!({"document_id":id,"start_frame":5,"end_frame":5}),
    ));
    assert!(empty["selection"].is_null());
    assert_eq!(empty["cursor"], 5);
    success(send(
        &mut session,
        "documents.cursor.set",
        json!({"document_id":id,"frame":12}),
    ));
    let before = session.snapshot();
    for (name, params, code) in [
        (
            "documents.cursor.set",
            json!({"document_id":id,"frame":13}),
            "invalid_params",
        ),
        (
            "documents.selection.set",
            json!({"document_id":id,"start_frame":13,"end_frame":0}),
            "invalid_params",
        ),
        (
            "documents.selection.all",
            json!({"document_id":"missing"}),
            "not_found",
        ),
    ] {
        assert_eq!(send(&mut session, name, params)["error"]["code"], code);
        assert_eq!(session.snapshot(), before);
    }
    let stale=protocol::dispatch_line(&mut session,&json!({"id":"stale","command":"documents.selection.all","params":{"document_id":id},"expected_revision":0}).to_string());
    assert_eq!(
        serde_json::to_value(stale).unwrap()["error"]["code"],
        "stale_revision"
    );
    assert_eq!(session.snapshot(), before);
}
