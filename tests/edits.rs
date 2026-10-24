use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn ok(s: &mut Session, name: &str, params: Value) -> Value {
    let value = serde_json::to_value(protocol::dispatch_line(
        s,
        &json!({"id":"structure","command":name,"params":params}).to_string(),
    ))
    .unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    value["result"].clone()
}
fn job(s: &mut Session, started: Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let job = ok(s, "jobs.get", json!({"job_id":started["id"]}));
        if job["status"] != "running" {
            assert_eq!(job["status"], "succeeded", "{job}");
            return job["result"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn open(
    s: &mut Session,
    dir: &std::path::Path,
    name: &str,
    rate: u32,
    channels: u16,
    values: &[f32],
) -> Value {
    let path = dir.join(name);
    let mut w = hound::WavWriter::create(
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
        w.write_sample(*value).unwrap();
    }
    w.finalize().unwrap();
    let started = ok(s, "documents.open", json!({"path":path}));
    job(s, started)
}
fn target(doc: &Value, start: u64, end: u64) -> Value {
    json!({"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":start,"end_frame":end})
}
fn pcm(s: &mut Session, doc: &Value, frames: u64) -> Value {
    ok(
        s,
        "documents.samples",
        json!({"document_id":doc["id"],"frames":frames}),
    )["channels"]
        .clone()
}
#[test]
fn cut_delete_trim_remap_boundaries_and_restore_in_one_undo() {
    for (name, start, end, expected, cursor, selection) in [
        ("edit.cut", 1, 3, json!([[0.125, 0.5]]), 1, Value::Null),
        (
            "edit.delete",
            0,
            1,
            json!([[0.25, 0.375, 0.5]]),
            0,
            Value::Null,
        ),
        (
            "edit.delete",
            3,
            4,
            json!([[0.125, 0.25, 0.375]]),
            3,
            Value::Null,
        ),
        (
            "edit.trim",
            1,
            3,
            json!([[0.25, 0.375]]),
            0,
            json!({"start":0,"end":2}),
        ),
    ] {
        let d = tempfile::tempdir().unwrap();
        let mut s = Session::default();
        let doc = open(
            &mut s,
            d.path(),
            "source.wav",
            48000,
            1,
            &[0.125, 0.25, 0.375, 0.5],
        );
        let original = std::fs::read(d.path().join("source.wav")).unwrap();
        ok(
            &mut s,
            "documents.selection.set",
            json!({"document_id":doc["id"],"start_frame":start,"end_frame":end}),
        );
        let started = ok(&mut s, name, json!({"target":target(&doc,start,end)}));
        let edited = job(&mut s, started);
        let frames = expected[0].as_array().unwrap().len() as u64;
        assert_eq!(pcm(&mut s, &doc, frames), expected);
        assert_eq!(edited["cursor"], cursor);
        assert_eq!(edited["selection"], selection);
        assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 1);
        if name == "edit.cut" {
            assert_eq!(ok(&mut s, "edit.clipboard.get", json!({}))["frames"], 2);
        }
        ok(&mut s, "history.undo", json!({}));
        assert_eq!(pcm(&mut s, &doc, 4), json!([[0.125, 0.25, 0.375, 0.5]]));
        ok(&mut s, "history.redo", json!({}));
        assert_eq!(pcm(&mut s, &doc, frames), expected);
        assert_eq!(
            std::fs::read(d.path().join("source.wav")).unwrap(),
            original
        );
    }
}
#[test]
fn copy_survives_close_and_paste_inserts_or_replaces_with_explicit_channel_mapping() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let source = open(&mut s, d.path(), "mono.wav", 48000, 1, &[0.25, 0.5]);
    ok(&mut s, "edit.copy", json!({"target":target(&source,0,2)}));
    assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 0);
    assert!(!s.snapshot().documents[0].dirty);
    ok(
        &mut s,
        "documents.close",
        json!({"document_id":source["id"]}),
    );
    std::fs::remove_file(d.path().join("mono.wav")).unwrap();
    let destination = open(
        &mut s,
        d.path(),
        "stereo.wav",
        48000,
        2,
        &[0.0, 0.125, 0.75, 0.875],
    );
    let started = ok(
        &mut s,
        "edit.paste",
        json!({"document_id":destination["id"],"revision_id":destination["revision_id"],"start_frame":1,"end_frame":1}),
    );
    let pasted = job(&mut s, started);
    assert_eq!(
        pcm(&mut s, &destination, 4),
        json!([[0.0, 0.25, 0.5, 0.75], [0.125, 0.25, 0.5, 0.875]])
    );
    assert_eq!(pasted["selection"], json!({"start":1,"end":3}));
    assert_eq!(pasted["cursor"], 3);
    ok(&mut s, "history.undo", json!({}));
    assert_eq!(
        pcm(&mut s, &destination, 2),
        json!([[0.0, 0.75], [0.125, 0.875]])
    );
    let started = ok(
        &mut s,
        "edit.paste",
        json!({"document_id":destination["id"],"revision_id":destination["revision_id"],"start_frame":0,"end_frame":2}),
    );
    let result = job(&mut s, started);
    assert_eq!(result["frames"], 2);
    assert_eq!(
        pcm(&mut s, &destination, 2),
        json!([[0.25, 0.5], [0.25, 0.5]])
    );
    ok(&mut s, "edit.copy", json!({"target":target(&result,0,2)}));
}
#[test]
fn whole_deletion_keeps_a_valid_empty_document_and_allows_paste_and_undo() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = open(&mut s, d.path(), "all.wav", 48000, 1, &[0.25, 0.5]);
    ok(&mut s, "edit.copy", json!({"target":target(&doc,0,2)}));
    let started = ok(&mut s, "edit.delete", json!({"target":target(&doc,0,2)}));
    let empty = job(&mut s, started);
    assert_eq!(empty["frames"], 0);
    assert_eq!(empty["cursor"], 0);
    assert!(empty["selection"].is_null());
    assert_eq!(
        ok(
            &mut s,
            "documents.waveform",
            json!({"document_id":doc["id"]})
        )["channels"],
        json!([[]])
    );
    let started = ok(
        &mut s,
        "edit.paste",
        json!({"document_id":doc["id"],"revision_id":empty["revision_id"],"start_frame":0,"end_frame":0}),
    );
    job(&mut s, started);
    assert_eq!(pcm(&mut s, &doc, 2), json!([[0.25, 0.5]]));
    ok(&mut s, "history.undo", json!({}));
    assert_eq!(s.snapshot().documents[0].frames, 0);
    ok(&mut s, "history.undo", json!({}));
    assert_eq!(pcm(&mut s, &doc, 2), json!([[0.25, 0.5]]));
}
#[test]
fn cross_rate_paste_preserves_dc_duration_and_stereo_downmix() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let source = open(
        &mut s,
        d.path(),
        "source.wav",
        44100,
        2,
        &[0.75, -0.25].repeat(441),
    );
    ok(&mut s, "edit.copy", json!({"target":target(&source,0,441)}));
    let destination = open(&mut s, d.path(), "destination.wav", 48000, 1, &[0.0]);
    let started = ok(
        &mut s,
        "edit.paste",
        json!({"document_id":destination["id"],"revision_id":destination["revision_id"],"start_frame":0,"end_frame":1}),
    );
    let result = job(&mut s, started);
    assert_eq!(result["frames"], 480);
    let values = pcm(&mut s, &destination, 480);
    for sample in values[0].as_array().unwrap() {
        assert!((sample.as_f64().unwrap() - 0.25).abs() < 1e-6);
    }
}

#[test]
fn resampling_preserves_passband_and_suppresses_downsampling_aliases() {
    for (frequency, max_error) in [(1000.0, 0.001), (20000.0, 0.002)] {
        let d = tempfile::tempdir().unwrap();
        let mut s = Session::default();
        let values = (0..4800)
            .map(|frame| {
                (0.5 * (frame as f64 * frequency * std::f64::consts::TAU / 48000.0).sin()) as f32
            })
            .collect::<Vec<_>>();
        let source = open(&mut s, d.path(), "tone.wav", 48000, 1, &values);
        ok(
            &mut s,
            "edit.copy",
            json!({"target":target(&source,0,4800)}),
        );
        let destination = open(&mut s, d.path(), "converted.wav", 32000, 1, &[0.0]);
        let started = ok(
            &mut s,
            "edit.paste",
            json!({"document_id":destination["id"],"revision_id":destination["revision_id"],"start_frame":0,"end_frame":1}),
        );
        let converted = job(&mut s, started);
        assert_eq!(converted["frames"], 3200);
        let pcm = pcm(&mut s, &destination, 3200);
        let mut squared = 0.0;
        for frame in 64..3136 {
            let expected = if frequency == 1000.0 {
                0.5 * (frame as f64 * frequency * std::f64::consts::TAU / 32000.0).sin()
            } else {
                0.0
            };
            squared += (pcm[0][frame].as_f64().unwrap() - expected).powi(2);
        }
        assert!(
            (squared / 3072.0).sqrt() < max_error,
            "frequency {frequency}, RMS {}",
            (squared / 3072.0).sqrt()
        );
    }
}
#[test]
fn invalid_empty_clipboard_and_cancelled_cut_preserve_audio_and_clipboard() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Session::default();
    let doc = open(
        &mut s,
        d.path(),
        "source.wav",
        48000,
        1,
        &[0.125, 0.25, 0.5],
    );
    let before = s.snapshot();
    let response=protocol::dispatch_line(&mut s,&json!({"id":"empty","command":"edit.paste","params":{"document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":0,"end_frame":0}}).to_string());
    assert_eq!(
        serde_json::to_value(response).unwrap()["error"]["code"],
        "invalid_params"
    );
    assert_eq!(s.snapshot(), before);
    ok(&mut s, "edit.copy", json!({"target":target(&doc,0,1)}));
    let clipboard = ok(&mut s, "edit.clipboard.get", json!({}));
    let revision = s.snapshot().revision;
    ok(&mut s, "edit.copy", json!({"target":target(&doc,0,1)}));
    assert_eq!(s.snapshot().revision, revision);
    let started = ok(&mut s, "edit.cut", json!({"target":target(&doc,1,3)}));
    ok(&mut s, "jobs.cancel", json!({"job_id":started["id"]}));
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(ok(&mut s, "edit.clipboard.get", json!({})), clipboard);
    assert_eq!(pcm(&mut s, &doc, 3), json!([[0.125, 0.25, 0.5]]));
    assert_eq!(ok(&mut s, "history.get", json!({}))["undo_count"], 0);
}
