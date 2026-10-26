use serde_json::{Value, json};
use wavebob::{Session, protocol};
fn send(s: &mut Session, name: &str, params: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(
        s,
        &json!({"id":"browser-test","command":name,"params":params}).to_string(),
    ))
    .unwrap()
}
fn ok(s: &mut Session, name: &str, params: Value) -> Value {
    let value = send(s, name, params);
    assert_eq!(value["status"], "ok", "{value}");
    value["result"].clone()
}
fn wait(s: &mut Session, field: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let state = ok(s, "browser.get", json!({}));
        if !state.pointer(field).unwrap().as_bool().unwrap() {
            return state;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn wav(path: &std::path::Path) {
    let mut w = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..4800 {
        w.write_sample(16384_i16).unwrap();
    }
    w.finalize().unwrap();
}
#[test]
fn asynchronous_folder_navigation_retains_usable_state_on_errors_and_marks_formats() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("子 folder")).unwrap();
    wav(&d.path().join("Sound.WAV"));
    std::fs::write(d.path().join("broken.mp3"), b"broken").unwrap();
    std::fs::write(d.path().join("broken.OGG"), b"broken").unwrap();
    std::fs::write(d.path().join("Notes.txt"), b"text").unwrap();
    let mut s = Session::default();
    ok(&mut s, "browser.navigate", json!({"path":d.path()}));
    let state = wait(&mut s, "/loading");
    assert_eq!(state["entries"][0]["directory"], true);
    assert_eq!(
        state["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["supported"] == true)
            .count(),
        3
    );
    let path = state["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "Sound.WAV")
        .unwrap()["path"]
        .clone();
    ok(&mut s, "browser.select", json!({"path":path}));
    assert!(s.snapshot().documents.is_empty());
    ok(
        &mut s,
        "browser.navigate",
        json!({"path":d.path().join("missing")}),
    );
    let failed = wait(&mut s, "/loading");
    assert_eq!(failed["error"]["code"], "io_error");
    assert_eq!(failed["entries"], state["entries"]);
    assert_eq!(
        send(&mut s, "browser.select", json!({"path":"unlisted"}))["error"]["code"],
        "invalid_params"
    );
}
#[test]
fn ogg_audition_decodes_without_opening_a_document() {
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio/vorbis-2ch-48000.ogg");
    ok(&mut s, "audition.start", json!({"path":path}));
    let state = wait(&mut s, "/audition/loading");
    assert!(state["audition"]["error"].is_null(), "{state}");
    assert_eq!(state["audition"]["playing"], true);
    assert_eq!(state["audition"]["channels"], 2);
    assert_eq!(state["audition"]["sample_rate"], 48000);
    assert!(s.snapshot().documents.is_empty());
    let mut pcm = [[0.0_f32; 2]; 4096];
    output.render_into(&mut pcm);
    assert!(pcm.iter().flatten().any(|sample| sample.abs() > 0.01));
    ok(&mut s, "audition.stop", json!({}));
    assert_eq!(
        ok(&mut s, "browser.get", json!({}))["audition"]["playing"],
        false
    );
}
#[test]
fn audition_is_independent_cancellable_and_exclusive_with_editor_playback() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("Preview.wav");
    wav(&path);
    let (mut s, mut output) = Session::with_manual_output(48000).unwrap();
    ok(&mut s, "audition.start", json!({"path":path}));
    let ready = wait(&mut s, "/audition/loading");
    assert_eq!(ready["audition"]["playing"], true);
    assert!(s.snapshot().documents.is_empty());
    let mut pcm = [[0.0; 2]; 1];
    output.render_into(&mut pcm);
    assert_eq!(pcm, [[0.375; 2]]);
    ok(&mut s, "audition.volume.set", json!({"volume":0.5}));
    output.render_into(&mut pcm);
    assert_eq!(pcm, [[0.25; 2]]);
    ok(&mut s, "audition.stop", json!({}));
    output.render_into(&mut pcm);
    assert_eq!(pcm, [[0.0; 2]]);
    ok(&mut s, "audition.start", json!({"path":path}));
    ok(&mut s, "audition.stop", json!({}));
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(
        wait(&mut s, "/audition/loading")["audition"]["playing"],
        false
    );
    let job = ok(&mut s, "browser.open", json!({"path":path}));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let id = loop {
        let job = ok(&mut s, "jobs.get", json!({"job_id":job["id"]}));
        if job["status"] == "succeeded" {
            break job["result"]["id"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    ok(&mut s, "transport.play", json!({"document_id":id}));
    output.render_into(&mut pcm);
    assert_eq!(pcm, [[0.5; 2]]);
    ok(&mut s, "audition.start", json!({"path":path}));
    assert_eq!(
        ok(&mut s, "transport.get", json!({}))["requested"]["playing"],
        false
    );
    ok(&mut s, "transport.play", json!({"document_id":id}));
    std::thread::sleep(std::time::Duration::from_millis(20));
    let state = wait(&mut s, "/audition/loading");
    assert!(state["audition"]["id"].is_null());
    output.render_into(&mut pcm);
    assert_eq!(pcm, [[0.5; 2]]);
    assert_eq!(s.snapshot().documents.len(), 1);
}
#[test]
fn mp3_preview_and_corrupt_supported_files_report_distinct_errors() {
    let (mut s, mut output) = Session::with_manual_output(44100).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio/cbr-1ch-44100-tagged.mp3");
    ok(&mut s, "audition.start", json!({"path":path}));
    let state = wait(&mut s, "/audition/loading");
    assert!(state["audition"]["error"].is_null(), "{state}");
    output.render_into(&mut [[0.0; 2]; 10]);
    assert!(s.snapshot().documents.is_empty());
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("bad.wav"), b"broken").unwrap();
    ok(
        &mut s,
        "audition.start",
        json!({"path":d.path().join("bad.wav")}),
    );
    assert_eq!(
        wait(&mut s, "/audition/loading")["audition"]["error"]["code"],
        "decode_failed"
    );
    assert_eq!(
        send(&mut s, "audition.start", json!({"path":"not-audio.txt"}))["error"]["code"],
        "unsupported_format"
    );
}
