#![cfg(feature = "gui")]

use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wavebob::ipc::{self, Client};

struct Gui(Child);

impl Gui {
    fn start(directory: &Path, name: &str) -> Self {
        Self::start_options(
            directory,
            name,
            "1440x900",
            &std::env::var("WAVEBOB_EFFECT_REVIEW_SCALE").unwrap_or_else(|_| "1".into()),
        )
    }
    fn start_options(directory: &Path, name: &str, size: &str, scale: &str) -> Self {
        let binary = std::env::var_os("WAVEBOB_REVIEW_EXECUTABLE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_wavebob").into());
        let mut command = Command::new(binary);
        command
            .args([
                "--hidden",
                "--window-size",
                size,
                "--ui-scale",
                scale,
                "--name",
                name,
                "--runtime-dir",
            ])
            .arg(directory)
            .arg("--preferences")
            .arg(directory.join(format!("{name}-layout.json")))
            .stdout(Stdio::null())
            .stderr(fs::File::create(directory.join(format!("{name}.log"))).unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // Avoid a console window for this background test.
        }
        Self(command.spawn().unwrap())
    }
}

impl Drop for Gui {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(client: &Client, command: &str, params: Value) -> Value {
    serde_json::to_value(
        client
            .call(&json!({"id":command,"command":command,"params":params}).to_string())
            .unwrap_or_else(|error| panic!("Attached {command} failed: {error:?}")),
    )
    .unwrap()
}

fn success(response: Value) -> Value {
    assert_eq!(response["status"], "ok", "{response}");
    response["result"].clone()
}

fn pixels(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    data.truncate(info.buffer_size());
    assert!(
        data.windows(8).any(|bytes| bytes[..4] != bytes[4..]),
        "Image must contain rendered content"
    );
    (info.width, info.height, data)
}

fn retain(path: &Path, name: &str) {
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::create_dir_all(&directory).unwrap();
        fs::copy(path, Path::new(&directory).join(name)).unwrap();
    }
}
fn reopen_and_capture(client: &Client, owner: &Value, effect: &Value, runtime: &Path, name: &str) {
    let opened = success(request(
        client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["instance_id"] == *effect)
        .unwrap()["id"]
        .clone();
    let capture = success(request(
        client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.join("reopened.png"),"overwrite":true}),
    ));
    assert_eq!(capture["metadata"]["owner"], *owner);
    assert_eq!(capture["metadata"]["instance_id"], *effect);
    pixels(Path::new(capture["path"].as_str().unwrap()));
    retain(
        Path::new(capture["path"].as_str().unwrap()),
        &format!("{name}.png"),
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(
            Path::new(&directory).join(format!("{name}.json")),
            serde_json::to_vec_pretty(&capture["metadata"]).unwrap(),
        )
        .unwrap();
    }
}

fn capture_narrow_bypass(
    client: &Client,
    owner: &Value,
    effect: &Value,
    editor: &Value,
    runtime: &Path,
    prefix: &str,
    size: [f64; 2],
) {
    success(request(
        client,
        "effects.editor.geometry",
        json!({"editor_id":editor,"geometry":{"width":size[0],"height":size[1],"x":80.0,"y":80.0}}),
    ));
    let capture = |name: &str| {
        let c = success(request(
            client,
            "capture.png",
            json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.join("capture.png"),"overwrite":true}),
        ));
        let (w, h, _) = pixels(Path::new(c["path"].as_str().unwrap()));
        let scale: f64 = std::env::var("WAVEBOB_EFFECT_REVIEW_SCALE")
            .unwrap_or_else(|_| "1".into())
            .parse()
            .unwrap();
        assert_eq!(
            [w, h],
            [
                (size[0] * scale).round() as u32,
                (size[1] * scale).round() as u32
            ]
        );
        retain(
            Path::new(c["path"].as_str().unwrap()),
            &format!("{prefix}-{name}.png"),
        );
        c["metadata"].clone()
    };
    let narrow = capture("narrow");
    let state = success(request(client, "processing.get", json!({"owner":owner})));
    success(request(
        client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"bypass","effect_id":effect,"bypassed":true}}),
    ));
    let bypassed = capture("bypassed");
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(
            Path::new(&directory).join(format!("{prefix}-states.json")),
            serde_json::to_vec_pretty(&json!({"narrow":narrow,"bypassed":bypassed})).unwrap(),
        )
        .unwrap();
    }
}

fn assert_same_pixels(actual: &[u8], expected: &[u8], width: u32) {
    let differences: Vec<_> = actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, (a, b))| (index % width as usize, index / width as usize, a, b))
        .collect();
    assert!(
        actual.len() == expected.len() && differences.is_empty(),
        "Scene differs at {} pixels; first differences: {:?}",
        differences.len(),
        &differences[..differences.len().min(10)]
    );
}

/// This intentionally launches real renderer-backed GUIs. It is opt-in on machines
/// without a desktop; invoke with --ignored on the Windows development workstation.
#[test]
#[ignore = "requires a native GUI renderer; run explicitly on a desktop"]
fn live_gui_attachment_capture_and_revision_fences() {
    let runtime = tempfile::tempdir().unwrap();
    let mut first = Gui::start(runtime.path(), "first");
    let mut second = Gui::start(runtime.path(), "second");
    let deadline = Instant::now() + Duration::from_secs(20);
    let instances = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if instances.len() == 2 {
            break instances;
        }
        for (gui, name) in [(&mut first, "first"), (&mut second, "second")] {
            if let Some(status) = gui.0.try_wait().unwrap() {
                panic!(
                    "{name} exited ({status}): {}",
                    fs::read_to_string(runtime.path().join(format!("{name}.log"))).unwrap()
                );
            }
        }
        assert!(
            Instant::now() < deadline,
            "GUIs did not register their endpoints"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let first_id = &instances
        .iter()
        .find(|info| info.name == "first")
        .unwrap()
        .session_id;
    let second_id = &instances
        .iter()
        .find(|info| info.name == "second")
        .unwrap()
        .session_id;
    let client = Client::attach(runtime.path(), first_id).unwrap();
    let other = Client::attach(runtime.path(), second_id).unwrap();
    assert!(Client::attach(runtime.path(), "not-a-session-id").is_err());
    let catalogue = success(request(&client, "session.capabilities", json!({})));
    assert_eq!(catalogue["commands"].as_array().unwrap().len(), 77);
    assert!(
        catalogue["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("capture.png"))
    );

    success(request(
        &client,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    assert_eq!(
        success(request(&other, "session.get", json!({})))["loop_enabled"],
        false
    );
    assert_eq!(
        success(request(&client, "session.get", json!({})))["session_id"],
        *first_id
    );
    let view = success(request(
        &client,
        "presentation.set_mode",
        json!({"mode":"multitrack","expected_presentation_revision":0}),
    ));
    assert_eq!(view["revision"], 1);
    assert_eq!(
        success(request(&other, "presentation.get", json!({})))["mode"],
        "waveform"
    );
    assert_eq!(
        request(
            &client,
            "presentation.set_mode",
            json!({"mode":"waveform","expected_presentation_revision":0})
        )["error"]["code"],
        "stale_revision"
    );
    assert_eq!(
        request(&client, "presentation.set_mode", json!({"mode":"invalid"}))["error"]["code"],
        "invalid_params"
    );

    let multi_path = runtime.path().join("multitrack.png");
    let multi = success(request(
        &client,
        "capture.png",
        json!({"path":multi_path,"after_revision":1,"after_presentation_revision":1}),
    ));
    assert_eq!(multi["metadata"]["session_id"], *first_id);
    assert_eq!(multi["metadata"]["mode"], "multitrack");
    assert_eq!(multi["metadata"]["rendered_revision"], 1);
    assert_eq!(multi["metadata"]["presentation_revision"], 1);
    assert!(multi["metadata"]["scale"].as_f64().unwrap() > 0.0);
    let (width, height, multi_pixels) = pixels(&multi_path);
    assert_eq!(multi["metadata"]["width"], width);
    assert_eq!(multi["metadata"]["height"], height);
    assert!(width >= 1000 && height >= 650);
    let original = fs::read(&multi_path).unwrap();
    retain(&multi_path, "multitrack.png");
    assert_eq!(
        request(&client, "capture.png", json!({"path":multi_path}))["error"]["code"],
        "output_exists"
    );
    assert_eq!(fs::read(&multi_path).unwrap(), original);

    let wave_path = runtime.path().join("waveform.png");
    success(request(
        &client,
        "presentation.set_mode",
        json!({"mode":"waveform"}),
    ));
    let wave = success(request(
        &client,
        "capture.png",
        json!({"path":wave_path,"after_presentation_revision":2}),
    ));
    assert_eq!(wave["metadata"]["mode"], "waveform");
    assert_eq!(wave["metadata"]["presentation_revision"], 2);
    let (_, _, wave_pixels) = pixels(&wave_path);
    retain(&wave_path, "waveform.png");
    assert_ne!(multi_pixels, wave_pixels);
    let barrier = success(request(
        &client,
        "presentation.barrier",
        json!({"after_revision":1,"after_presentation_revision":2}),
    ));
    assert_eq!(barrier["mode"], "waveform");
    assert!(
        barrier["frame_number"].as_u64().unwrap()
            >= wave["metadata"]["frame_number"].as_u64().unwrap()
    );

    let never_path = runtime.path().join("never.png");
    let timed = request(
        &client,
        "capture.png",
        json!({"path":never_path,"after_revision":999,"timeout_ms":100}),
    );
    assert_eq!(timed["error"]["code"], "timeout");
    assert!(!never_path.exists());
    assert_eq!(
        request(&client, "capture.png", json!({"timeout_ms":0}))["error"]["code"],
        "invalid_params"
    );
    assert_eq!(
        request(
            &client,
            "capture.png",
            json!({"path":runtime.path().join("missing/parent/file.png")})
        )["error"]["code"],
        "io_error"
    );

    // Submit capture and a following view mutation through independent clients.
    // The image must match its claimed state even when their responses race.
    let race_client = Client::attach(runtime.path(), first_id).unwrap();
    let race_path = runtime.path().join("race.png");
    let race_destination = race_path.clone();
    let capture = std::thread::spawn(move || {
        success(request(
            &race_client,
            "capture.png",
            json!({"path":race_destination}),
        ))
    });
    success(request(
        &client,
        "presentation.set_mode",
        json!({"mode":"multitrack"}),
    ));
    let race = capture.join().unwrap();
    let (_, _, race_pixels) = pixels(&race_path);
    retain(&race_path, "race.png");
    match race["metadata"]["mode"].as_str().unwrap() {
        "waveform" => assert_same_pixels(&race_pixels, &wave_pixels, width),
        "multitrack" => assert_same_pixels(&race_pixels, &multi_pixels, width),
        other => panic!("Unexpected mode: {other}"),
    }
    success(request(
        &client,
        "capture.png",
        json!({"path":wave_path,"overwrite":true}),
    ));
    assert_same_pixels(&pixels(&wave_path).2, &multi_pixels, width);
    let default_capture = success(request(&client, "capture.png", json!({})));
    assert!(PathBuf::from(default_capture["path"].as_str().unwrap()).exists());

    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::create_dir_all(&directory).unwrap();
        fs::write(Path::new(&directory).join("multitrack.png"), original).unwrap();
        // waveform.png was deliberately overwritten above; retain the earlier
        // reference through a fresh waveform command/capture.
        success(request(
            &client,
            "presentation.set_mode",
            json!({"mode":"waveform"}),
        ));
        success(request(
            &client,
            "capture.png",
            json!({"path":Path::new(&directory).join("waveform.png"),"overwrite":true}),
        ));
        fs::copy(&race_path, Path::new(&directory).join("race.png")).unwrap();
        fs::write(
            Path::new(&directory).join("metadata.json"),
            serde_json::to_vec_pretty(&json!({"multitrack":multi,"waveform":wave,"race":race}))
                .unwrap(),
        )
        .unwrap();
    }

    first.0.kill().unwrap();
    first.0.wait().unwrap();
    assert_eq!(
        ipc::discover(runtime.path()).unwrap().len(),
        1,
        "Stale discovery records must not count as live instances"
    );
    assert!(Client::attach(runtime.path(), first_id).is_err());
}

#[test]
#[ignore = "requires native effect viewports; run explicitly on the desktop"]
fn native_soft_clipper_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "clipper-native");
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if let Some(instance) = instances
            .into_iter()
            .find(|instance| instance.name == "clipper-native")
        {
            break instance;
        }
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited {status}: {}",
                fs::read_to_string(runtime.path().join("clipper-native.log")).unwrap()
            );
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("Native Soft Clipper.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..48000 {
        let value = (frame as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let state = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if state["status"] != "running" {
            assert_eq!(state["status"], "succeeded", "{state}");
            break state["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    let state = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"soft_clipper"}}),
    ));
    let effect = state["committed"]["effects"][0]["id"].clone();
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"][0]["id"].clone();
    let first_response = request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("soft-capture.png"),"overwrite":true}),
    );
    if first_response["status"] != "ok" {
        retain(
            &runtime.path().join("clipper-native.log"),
            "native-capture-failure.log",
        );
    }
    let first = success(first_response);
    assert_eq!(first["metadata"]["owner"], owner);
    assert_eq!(first["metadata"]["instance_id"], effect);
    assert_eq!(first["metadata"]["editor_id"], editor);
    assert!(first["metadata"]["processing_identity"].is_string());
    let (width, height, before) = pixels(Path::new(first["path"].as_str().unwrap()));
    assert!(width >= 320 && height >= 240);
    retain(
        Path::new(first["path"].as_str().unwrap()),
        "soft-clipper-default.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"threshold","value":32}}),
    ));
    let second = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("soft-capture.png"),"overwrite":true}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and transfer curve"
    );
    assert_ne!(
        first["metadata"]["processing_identity"],
        second["metadata"]["processing_identity"]
    );
    retain(
        Path::new(second["path"].as_str().unwrap()),
        "soft-clipper-threshold-32.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("soft-clipper-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"soft-clipper-default.png","metadata":first["metadata"]},"threshold_32":{"image":"soft-clipper-threshold-32.png","metadata":second["metadata"]}})).unwrap()).unwrap();
    }
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"post","value":160}}),
    ));
    let headroom = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("soft-capture.png"),"overwrite":true}),
    ));
    retain(
        Path::new(headroom["path"].as_str().unwrap()),
        "soft-clipper-headroom.png",
    );
    success(request(
        &client,
        "effects.editor.geometry",
        json!({"editor_id":editor,"geometry":{"width":680.0,"height":424.0,"x":80.0,"y":80.0}}),
    ));
    let narrow = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("soft-capture.png"),"overwrite":true}),
    ));
    let scale = std::env::var("WAVEBOB_EFFECT_REVIEW_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.0);
    assert_eq!(narrow["metadata"]["width"], json!((680.0 * scale) as u32));
    assert_eq!(narrow["metadata"]["height"], json!((424.0 * scale) as u32));
    retain(
        Path::new(narrow["path"].as_str().unwrap()),
        "soft-clipper-narrow.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"bypass","effect_id":effect,"bypassed":true}}),
    ));
    let bypassed = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("soft-capture.png"),"overwrite":true}),
    ));
    retain(
        Path::new(bypassed["path"].as_str().unwrap()),
        "soft-clipper-bypassed.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("soft-clipper-redesign.json"),serde_json::to_vec_pretty(&json!({"headroom":headroom["metadata"],"narrow":narrow["metadata"],"bypassed":bypassed["metadata"]})).unwrap()).unwrap();
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(opened["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(opened["editor_windows"][0]["id"], editor);
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(&client, "capture.png", json!({"editor_id":editor}))["error"]["code"],
        "not_found"
    );
    reopen_and_capture(
        &client,
        &owner,
        &effect,
        runtime.path(),
        "soft-clipper-reopened",
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[test]
#[ignore = "requires native effect viewports; run explicitly on the desktop"]
fn native_reverb_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let scale = std::env::var("WAVEBOB_REVERB_REVIEW_SCALE").unwrap_or_else(|_| "1".into());
    let mut gui = Gui::start_options(runtime.path(), "reverb-native", "1440x900", &scale);
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if let Some(instance) = instances
            .into_iter()
            .find(|instance| instance.name == "reverb-native")
        {
            break instance;
        }
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited {status}: {}",
                fs::read_to_string(runtime.path().join("reverb-native.log")).unwrap()
            );
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("MORTH-4G_shot_17a.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..48000 {
        let value = (frame as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let state = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if state["status"] != "running" {
            assert_eq!(state["status"], "succeeded", "{state}");
            break state["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    let state = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"reverb"}}),
    ));
    let effect = state["committed"]["effects"][0]["id"].clone();
    // Render the approved mockup's illustrative settings through the real DSP.
    for (id, value) in [("size", 70), ("dry", 90), ("early", 38), ("wet", 38)] {
        let state = success(request(&client, "processing.get", json!({"owner":owner})));
        success(request(
            &client,
            "processing.edit",
            json!({"owner":owner,
            "expected_edit_revision":state["committed"]["edit_revision_id"],
            "change":{"action":"set_parameter","effect_id":effect,"parameter_id":id,"value":value}}),
        ));
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"][0]["id"].clone();
    let first_response = request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("reverb-capture.png"),"overwrite":true}),
    );
    if first_response["status"] != "ok" {
        retain(
            &runtime.path().join("reverb-native.log"),
            "native-capture-failure.log",
        );
    }
    let first = success(first_response);
    assert_eq!(first["metadata"]["owner"], owner);
    assert_eq!(first["metadata"]["instance_id"], effect);
    assert_eq!(first["metadata"]["editor_id"], editor);
    assert!(first["metadata"]["processing_identity"].is_string());
    let (width, height, before) = pixels(Path::new(first["path"].as_str().unwrap()));
    assert!(width >= 320 && height >= 240);
    retain(
        Path::new(first["path"].as_str().unwrap()),
        "reverb-default.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"size","value":100}}),
    ));
    let second = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("reverb-capture.png"),"overwrite":true}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and diffusion visualization"
    );
    assert_ne!(
        first["metadata"]["processing_identity"],
        second["metadata"]["processing_identity"]
    );
    retain(
        Path::new(second["path"].as_str().unwrap()),
        "reverb-size-100.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("reverb-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"reverb-default.png","metadata":first["metadata"]},"size_100":{"image":"reverb-size-100.png","metadata":second["metadata"]}})).unwrap()).unwrap();
    }
    success(request(
        &client,
        "processing.tempo.set",
        json!({"bpm":90.0}),
    ));
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"tempo_sync","value":true}}),
    ));
    let synced = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("reverb-capture.png"),"overwrite":true}),
    ));
    pixels(Path::new(synced["path"].as_str().unwrap()));
    retain(
        Path::new(synced["path"].as_str().unwrap()),
        "reverb-tempo-90.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(
            Path::new(&directory).join("reverb-tempo-90.json"),
            serde_json::to_vec_pretty(&synced["metadata"]).unwrap(),
        )
        .unwrap();
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(opened["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(opened["editor_windows"][0]["id"], editor);
    success(request(
        &client,
        "effects.editor.geometry",
        json!({"editor_id":editor,
        "geometry":{"width":680.0,"height":424.0,"x":80.0,"y":80.0}}),
    ));
    let narrow = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,
        "path":runtime.path().join("reverb-narrow.png"),"overwrite":true}),
    ));
    pixels(Path::new(narrow["path"].as_str().unwrap()));
    retain(
        Path::new(narrow["path"].as_str().unwrap()),
        "reverb-narrow.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(
            Path::new(&directory).join("reverb-narrow.json"),
            serde_json::to_vec_pretty(&narrow["metadata"]).unwrap(),
        )
        .unwrap();
    }
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(&client, "capture.png", json!({"editor_id":editor}))["error"]["code"],
        "not_found"
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[test]
#[ignore = "requires native effect viewports; run explicitly on the desktop"]
fn native_fast_distortion_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let scale = std::env::var("WAVEBOB_DISTORTION_REVIEW_SCALE").unwrap_or_else(|_| "1".into());
    let mut gui = Gui::start_options(runtime.path(), "dist-native", "1440x900", &scale);
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if let Some(instance) = instances
            .into_iter()
            .find(|instance| instance.name == "dist-native")
        {
            break instance;
        }
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited {status}: {}",
                fs::read_to_string(runtime.path().join("dist-native.log")).unwrap()
            );
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("MORTH-4G_shot_17a.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..48000 {
        let value = (frame as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let state = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if state["status"] != "running" {
            assert_eq!(state["status"], "succeeded", "{state}");
            break state["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    let state = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"fast_distortion"}}),
    ));
    let effect = state["committed"]["effects"][0]["id"].clone();
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"][0]["id"].clone();
    let first_response = request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("distortion-capture.png"),"overwrite":true}),
    );
    if first_response["status"] != "ok" {
        retain(
            &runtime.path().join("dist-native.log"),
            "native-capture-failure.log",
        );
    }
    let first = success(first_response);
    assert_eq!(first["metadata"]["owner"], owner);
    assert_eq!(first["metadata"]["instance_id"], effect);
    assert_eq!(first["metadata"]["editor_id"], editor);
    assert!(first["metadata"]["processing_identity"].is_string());
    let (width, height, before) = pixels(Path::new(first["path"].as_str().unwrap()));
    assert!(width >= 320 && height >= 240);
    retain(
        Path::new(first["path"].as_str().unwrap()),
        "fast-distortion-default.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"type","value":1}}),
    ));
    let second = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("distortion-capture.png"),"overwrite":true}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and transfer curve"
    );
    assert_ne!(
        first["metadata"]["processing_identity"],
        second["metadata"]["processing_identity"]
    );
    retain(
        Path::new(second["path"].as_str().unwrap()),
        "fast-distortion-type-b.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("fast-distortion-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"fast-distortion-default.png","metadata":first["metadata"]},"type_b":{"image":"fast-distortion-type-b.png","metadata":second["metadata"]}})).unwrap()).unwrap();
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(opened["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(opened["editor_windows"][0]["id"], editor);
    // Representative settings exercise the four calibrated dials and curve.
    for (id, value) in [
        ("pre", 176),
        ("threshold", 7),
        ("post", 96),
        ("mix", 110),
        ("type", 0),
    ] {
        let state = success(request(&client, "processing.get", json!({"owner":owner})));
        success(request(
            &client,
            "processing.edit",
            json!({"owner":owner,
            "expected_edit_revision":state["committed"]["edit_revision_id"],
            "change":{"action":"set_parameter","effect_id":effect,"parameter_id":id,"value":value}}),
        ));
    }
    let styled = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,
        "path":runtime.path().join("distortion-capture.png"),"overwrite":true}),
    ));
    pixels(Path::new(styled["path"].as_str().unwrap()));
    retain(
        Path::new(styled["path"].as_str().unwrap()),
        "distortion-shaped.png",
    );
    success(request(
        &client,
        "effects.editor.geometry",
        json!({"editor_id":editor,
        "geometry":{"width":680.0,"height":424.0,"x":80.0,"y":80.0}}),
    ));
    let narrow = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,
        "path":runtime.path().join("distortion-capture.png"),"overwrite":true}),
    ));
    let (_, _, narrow_pixels) = pixels(Path::new(narrow["path"].as_str().unwrap()));
    retain(
        Path::new(narrow["path"].as_str().unwrap()),
        "distortion-narrow.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,
        "expected_edit_revision":state["committed"]["edit_revision_id"],
        "change":{"action":"bypass","effect_id":effect,"bypassed":true}}),
    ));
    let bypassed = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,
        "path":runtime.path().join("distortion-capture.png"),"overwrite":true}),
    ));
    let (_, _, bypass_pixels) = pixels(Path::new(bypassed["path"].as_str().unwrap()));
    assert_ne!(narrow_pixels, bypass_pixels);
    retain(
        Path::new(bypassed["path"].as_str().unwrap()),
        "distortion-bypassed.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("distortion-redesign-captures.json"),
            serde_json::to_vec_pretty(&json!({"shaped":styled["metadata"],"narrow":narrow["metadata"],"bypassed":bypassed["metadata"]})).unwrap()).unwrap();
    }
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(&client, "capture.png", json!({"editor_id":editor}))["error"]["code"],
        "not_found"
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[test]
#[ignore = "requires native GPU viewports; explicit focused desktop check"]
fn native_bloom_pilots_remain_independent_while_alternating_and_reopening() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "bloom-pilots");
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        if let Some(i) = ipc::discover(runtime.path())
            .unwrap()
            .into_iter()
            .find(|i| i.name == "bloom-pilots")
        {
            break i;
        }
        assert!(gui.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("Independent.wav");
    let mut wav = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for _ in 0..4800 {
        wav.write_sample(0.0f32).unwrap();
        wav.write_sample(0.0f32).unwrap();
    }
    wav.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let j = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if j["status"] != "running" {
            assert_eq!(j["status"], "succeeded");
            break j["result"].clone();
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let mut effects = Vec::new();
    let mut editors = Vec::new();
    for kind in ["reverb", "fast_distortion"] {
        let state = success(request(&client, "processing.get", json!({"owner":owner})));
        let state = success(request(
            &client,
            "processing.edit",
            json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":kind}}),
        ));
        let id = state["committed"]["effects"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["id"]
            .clone();
        let windows = success(request(
            &client,
            "effects.editor.open",
            json!({"owner":owner,"instance_id":id}),
        ));
        let editor = windows["editor_windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["instance_id"] == id)
            .unwrap()["id"]
            .clone();
        effects.push(id);
        editors.push(editor);
    }
    let capture_timings = std::cell::RefCell::new(Vec::new());
    let capture = |index: usize, name: &str| {
        let started = Instant::now();
        let result = request(
            &client,
            "capture.png",
            json!({"editor_id":editors[index],"timeout_ms":5000,"path":runtime.path().join("pilot.png"),"overwrite":true}),
        );
        if result["status"] != "ok" {
            retain(
                &runtime.path().join("bloom-pilots.log"),
                "capture-failure.log",
            );
        }
        let c = success(result);
        capture_timings.borrow_mut().push(json!({"image":name,"request_to_png_ms":started.elapsed().as_secs_f64()*1000.0,"frame_number":c["metadata"]["frame_number"],"timeout_ms":5000}));
        assert_eq!(c["metadata"]["owner"], owner);
        assert_eq!(c["metadata"]["instance_id"], effects[index]);
        let p = pixels(Path::new(c["path"].as_str().unwrap()));
        retain(Path::new(c["path"].as_str().unwrap()), name);
        (c, p)
    };
    let (a, _) = capture(0, "simultaneous-reverb.png");
    let (b, (_, _, before)) = capture(1, "simultaneous-distortion.png");
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"bypass","effect_id":effects[0],"bypassed":true}}),
    ));
    let (bypassed, _) = capture(0, "simultaneous-reverb-bypassed.png");
    #[cfg(windows)]
    {
        // Hide this exact test-owned HWND, then force a complete hidden capture.
        #[link(name = "user32")]
        unsafe extern "system" {
            fn FindWindowW(class: *const u16, name: *const u16) -> *mut std::ffi::c_void;
            fn ShowWindow(window: *mut std::ffi::c_void, command: i32) -> i32;
            fn IsWindowVisible(window: *mut std::ffi::c_void) -> i32;
        }
        // Hide every child so the root must service the command through
        // eframe's logic-only hook, without help from another visible editor.
        let mut hidden_windows = Vec::new();
        for (label, effect) in [("Reverb", &effects[0]), ("Distortion", &effects[1])] {
            let title: Vec<u16> = format!(
                "{label} · Independent.wav · {}",
                &effect.as_str().unwrap()[..8]
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
            assert!(!hwnd.is_null(), "Test-owned {label} window must exist");
            assert_ne!(unsafe { IsWindowVisible(hwnd) }, 0);
            unsafe {
                ShowWindow(hwnd, 0);
            }
            hidden_windows.push(hwnd);
        }
        let (hidden, _) = capture(0, "simultaneous-reverb-hidden.png");
        assert_eq!(hidden["metadata"]["instance_id"], effects[0]);
        for hwnd in hidden_windows {
            assert_eq!(
                unsafe { IsWindowVisible(hwnd) },
                0,
                "Capture must leave every native window hidden"
            );
        }
    }
    let (_, (_, _, after)) = capture(1, "simultaneous-distortion-unchanged.png");
    assert_eq!(
        before, after,
        "Another viewport's bypass must not alter Distortion pixels"
    );
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effects[0]}),
    ));
    let (_, (_, _, after_close)) = capture(1, "simultaneous-distortion-after-close.png");
    assert_eq!(before, after_close);
    let reopened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effects[0]}),
    ));
    let reopened_id = reopened["editor_windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["instance_id"] == effects[0])
        .unwrap()["id"]
        .clone();
    let again = success(request(
        &client,
        "capture.png",
        json!({"editor_id":reopened_id,"timeout_ms":5000,"path":runtime.path().join("reopened.png"),"overwrite":true}),
    ));
    assert_eq!(again["metadata"]["instance_id"], effects[0]);
    retain(
        Path::new(again["path"].as_str().unwrap()),
        "simultaneous-reverb-reopened.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("simultaneous-metadata.json"),serde_json::to_vec_pretty(&json!({"reverb":a["metadata"],"distortion":b["metadata"],"bypassed":bypassed["metadata"],"reopened":again["metadata"]})).unwrap()).unwrap();
        fs::write(
            Path::new(&directory).join("simultaneous-capture-timings.json"),
            serde_json::to_vec_pretty(&*capture_timings.borrow()).unwrap(),
        )
        .unwrap();
    }
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[test]
#[ignore = "requires native effect viewports; run explicitly on the desktop"]
fn native_parametric_eq_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "eq-native");
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if let Some(instance) = instances
            .into_iter()
            .find(|instance| instance.name == "eq-native")
        {
            break instance;
        }
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited {status}: {}",
                fs::read_to_string(runtime.path().join("eq-native.log")).unwrap()
            );
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("Native Parametric EQ.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..48000 {
        let value = (frame as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let state = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if state["status"] != "running" {
            assert_eq!(state["status"], "succeeded", "{state}");
            break state["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    let state = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"parametric_eq"}}),
    ));
    let effect = state["committed"]["effects"][0]["id"].clone();
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"][0]["id"].clone();
    let first_response = request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("eq.png"),"overwrite":true}),
    );
    if first_response["status"] != "ok" {
        retain(
            &runtime.path().join("eq-native.log"),
            "native-capture-failure.log",
        );
    }
    let first = success(first_response);
    assert_eq!(first["metadata"]["owner"], owner);
    assert_eq!(first["metadata"]["instance_id"], effect);
    assert_eq!(first["metadata"]["editor_id"], editor);
    assert!(first["metadata"]["processing_identity"].is_string());
    let (width, height, before) = pixels(Path::new(first["path"].as_str().unwrap()));
    assert!(width >= 320 && height >= 240);
    retain(
        Path::new(first["path"].as_str().unwrap()),
        "parametric-eq-default.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"level4","value":1200}}),
    ));
    let second = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("eq.png"),"overwrite":true}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and response curve"
    );
    assert_ne!(
        first["metadata"]["processing_identity"],
        second["metadata"]["processing_identity"]
    );
    retain(
        Path::new(second["path"].as_str().unwrap()),
        "parametric-eq-band-4.png",
    );
    let mut states = Vec::new();
    for (name, parameters) in [
        (
            "styled",
            vec![
                ("level1", 300),
                ("level3", -400),
                ("level4", 300),
                ("freq4", wavebob::dsp::parametric_eq::frequency_raw(1000.0)),
                ("level6", -300),
            ]
            .into_iter()
            .map(|(id, v)| (id.to_owned(), v))
            .collect::<Vec<_>>(),
        ),
        (
            "shape-order-endpoint",
            vec![("freq4", 65536), ("type4", 7), ("order4", -3)]
                .into_iter()
                .map(|(id, v)| (id.to_owned(), v))
                .collect::<Vec<_>>(),
        ),
        (
            "coincident",
            (1..=7)
                .flat_map(|b| [(format!("freq{b}"), 32768), (format!("level{b}"), 0)])
                .collect(),
        ),
    ] {
        for (id, value) in parameters {
            let state = success(request(&client, "processing.get", json!({"owner":owner})));
            success(request(
                &client,
                "processing.edit",
                json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":id,"value":value}}),
            ));
        }
        let c = success(request(
            &client,
            "capture.png",
            json!({"editor_id":editor,"timeout_ms":5000,"path":runtime.path().join("eq.png"),"overwrite":true}),
        ));
        pixels(Path::new(c["path"].as_str().unwrap()));
        retain(
            Path::new(c["path"].as_str().unwrap()),
            &format!("parametric-eq-{name}.png"),
        );
        states.push(json!({"state":name,"metadata":c["metadata"]}));
    }
    capture_narrow_bypass(
        &client,
        &owner,
        &effect,
        &editor,
        runtime.path(),
        "parametric-eq",
        [680.0, 424.0],
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(
            Path::new(&directory).join("parametric-eq-altered.json"),
            serde_json::to_vec_pretty(&states).unwrap(),
        )
        .unwrap();
    }
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("parametric-eq-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"parametric-eq-default.png","metadata":first["metadata"]},"band_4_boost":{"image":"parametric-eq-band-4.png","metadata":second["metadata"]}})).unwrap()).unwrap();
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(opened["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(opened["editor_windows"][0]["id"], editor);
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(&client, "capture.png", json!({"editor_id":editor}))["error"]["code"],
        "not_found"
    );
    reopen_and_capture(
        &client,
        &owner,
        &effect,
        runtime.path(),
        "parametric-eq-reopened",
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[test]
#[ignore = "requires actual native waveform/arrangement capture at multiple DPI sizes"]
fn native_envelope_waveform_and_clip_captures_at_normal_high_dpi_and_narrow_widths() {
    for (name, size, scale) in [
        ("normal", "1440x900", "1"),
        ("high-dpi", "1440x900", "1.5"),
        ("narrow", "800x600", "1"),
    ] {
        let runtime = tempfile::tempdir().unwrap();
        let mut gui = Gui::start_options(runtime.path(), name, size, scale);
        let deadline = Instant::now() + Duration::from_secs(30);
        let instance = loop {
            let instances = ipc::discover(runtime.path()).unwrap();
            if let Some(instance) = instances.into_iter().find(|instance| instance.name == name) {
                break instance;
            }
            if let Some(status) = gui.0.try_wait().unwrap() {
                panic!(
                    "GUI exited {status}: {}",
                    fs::read_to_string(runtime.path().join(format!("{name}.log"))).unwrap()
                );
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
        let source = runtime.path().join("Envelope stereo.wav");
        let mut writer = hound::WavWriter::create(
            &source,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for frame in 0..48000 {
            let phase = frame as f32 * std::f32::consts::TAU / 48000.0;
            writer.write_sample((phase * 997.0).sin() * 0.7).unwrap();
            writer.write_sample((phase * 1733.0).sin() * 0.5).unwrap();
        }
        writer.finalize().unwrap();
        let started = success(request(&client, "documents.open", json!({"path":source})));
        let document = loop {
            let job = success(request(
                &client,
                "jobs.get",
                json!({"job_id":started["id"]}),
            ));
            if job["status"] != "running" {
                assert_eq!(job["status"], "succeeded", "{job}");
                break job["result"].clone();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let edit = |owner: &Value, gain: f64, fade_in: u64, fade_out: u64| {
            let state = success(request(&client, "processing.get", json!({"owner":owner})));
            success(request(
                &client,
                "processing.edit",
                json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_envelope","envelope":{"gain_db":gain,"fade_in_frames":fade_in,"fade_out_frames":fade_out}}}),
            ))
        };
        let owner = json!({"kind":"document","id":document["id"]});
        edit(&owner, -6.0, 8000, 12000);
        let waveform = success(request(&client, "capture.png", json!({"timeout_ms":10000})));
        assert_eq!(waveform["metadata"]["owner"], owner);
        assert_eq!(waveform["metadata"]["scale"], scale.parse::<f64>().unwrap());
        pixels(Path::new(waveform["path"].as_str().unwrap()));
        retain(
            Path::new(waveform["path"].as_str().unwrap()),
            &format!("waveform-{name}.png"),
        );
        edit(&owner, 0.0, 0, 0);
        let arrangement = success(request(&client, "tracks.add", json!({})));
        let track = arrangement["tracks"][0]["id"].clone();
        let arrangement = success(request(
            &client,
            "clips.insert",
            json!({"track_id":track,"document_id":document["id"],"revision_id":document["revision_id"],"source_start":0,"source_end":48000,"position":0}),
        ));
        let clip = arrangement["tracks"][0]["clips"][0].clone();
        let clip_owner = json!({"kind":"clip","id":clip["id"]});
        edit(&clip_owner, -6.0, 8000, 12000);
        success(request(
            &client,
            "clips.duplicate",
            json!({"clip_id":clip["id"],"position":36000}),
        ));
        success(request(
            &client,
            "arrangement.navigate",
            json!({"cursor":24000,"selection":null,"selected_clip_ids":[clip["id"]]}),
        ));
        success(request(
            &client,
            "presentation.set_mode",
            json!({"mode":"multitrack"}),
        ));
        success(request(
            &client,
            "presentation.timeline.set",
            json!({"start_frame":0,"end_frame":96000}),
        ));
        let arrangement = success(request(&client, "capture.png", json!({"timeout_ms":10000})));
        assert_eq!(arrangement["metadata"]["owner"], clip_owner);
        pixels(Path::new(arrangement["path"].as_str().unwrap()));
        retain(
            Path::new(arrangement["path"].as_str().unwrap()),
            &format!("clips-{name}.png"),
        );
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&directory).join(format!("capture-{name}.json")),
                serde_json::to_vec_pretty(
                    &json!({"waveform":waveform["metadata"],"clips":arrangement["metadata"]}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        success(request(
            &client,
            "presentation.quit",
            json!({"discard_changes":true}),
        ));
    }
}

#[test]
#[ignore = "requires native effect viewports; run explicitly on the desktop"]
fn native_delay_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "delay-native");
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if let Some(instance) = instances
            .into_iter()
            .find(|instance| instance.name == "delay-native")
        {
            break instance;
        }
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited {status}: {}",
                fs::read_to_string(runtime.path().join("delay-native.log")).unwrap()
            );
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("Native Delay.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..48000 {
        let value = (frame as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let state = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if state["status"] != "running" {
            assert_eq!(state["status"], "succeeded", "{state}");
            break state["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    let state = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"delay"}}),
    ));
    let effect = state["committed"]["effects"][0]["id"].clone();
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"][0]["id"].clone();
    let first_response = request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000}),
    );
    if first_response["status"] != "ok" {
        retain(
            &runtime.path().join("delay-native.log"),
            "native-capture-failure.log",
        );
    }
    let first = success(first_response);
    assert_eq!(first["metadata"]["owner"], owner);
    assert_eq!(first["metadata"]["instance_id"], effect);
    assert_eq!(first["metadata"]["editor_id"], editor);
    assert!(first["metadata"]["processing_identity"].is_string());
    let (width, height, before) = pixels(Path::new(first["path"].as_str().unwrap()));
    assert!(width >= 320 && height >= 240);
    retain(
        Path::new(first["path"].as_str().unwrap()),
        "delay-default.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"type","value":2}}),
    ));
    let second = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and transfer curve"
    );
    assert_ne!(
        first["metadata"]["processing_identity"],
        second["metadata"]["processing_identity"]
    );
    retain(
        Path::new(second["path"].as_str().unwrap()),
        "delay-ping-pong.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("delay-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"delay-default.png","metadata":first["metadata"]},"ping_pong":{"image":"delay-ping-pong.png","metadata":second["metadata"]}})).unwrap()).unwrap();
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(opened["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(opened["editor_windows"][0]["id"], editor);
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(&client, "capture.png", json!({"editor_id":editor}))["error"]["code"],
        "not_found"
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[test]
#[ignore = "requires native effect viewports; run explicitly on the desktop"]
fn native_chorus_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "chorus-native");
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if let Some(instance) = instances
            .into_iter()
            .find(|instance| instance.name == "chorus-native")
        {
            break instance;
        }
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited {status}: {}",
                fs::read_to_string(runtime.path().join("chorus-native.log")).unwrap()
            );
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("Native Chorus.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..48000 {
        let value = (frame as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let state = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if state["status"] != "running" {
            assert_eq!(state["status"], "succeeded", "{state}");
            break state["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    let state = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"chorus"}}),
    ));
    let effect = state["committed"]["effects"][0]["id"].clone();
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"][0]["id"].clone();
    let first_response = request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000}),
    );
    if first_response["status"] != "ok" {
        retain(
            &runtime.path().join("chorus-native.log"),
            "native-capture-failure.log",
        );
    }
    let first = success(first_response);
    assert_eq!(first["metadata"]["owner"], owner);
    assert_eq!(first["metadata"]["instance_id"], effect);
    assert_eq!(first["metadata"]["editor_id"], editor);
    assert!(first["metadata"]["processing_identity"].is_string());
    let (width, height, before) = pixels(Path::new(first["path"].as_str().unwrap()));
    assert!(width >= 320 && height >= 240);
    retain(
        Path::new(first["path"].as_str().unwrap()),
        "chorus-default.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"wave1","value":1024}}),
    ));
    let second = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and transfer curve"
    );
    assert_ne!(
        first["metadata"]["processing_identity"],
        second["metadata"]["processing_identity"]
    );
    retain(
        Path::new(second["path"].as_str().unwrap()),
        "chorus-sharp-sines.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("chorus-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"chorus-default.png","metadata":first["metadata"]},"sharp_sines":{"image":"chorus-sharp-sines.png","metadata":second["metadata"]}})).unwrap()).unwrap();
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(opened["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(opened["editor_windows"][0]["id"], editor);
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(&client, "capture.png", json!({"editor_id":editor}))["error"]["code"],
        "not_found"
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[test]
#[ignore = "requires native effect viewports; run explicitly on the desktop"]
fn native_limiter_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "limiter-native");
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        let instances = ipc::discover(runtime.path()).unwrap();
        if let Some(instance) = instances
            .into_iter()
            .find(|instance| instance.name == "limiter-native")
        {
            break instance;
        }
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited {status}: {}",
                fs::read_to_string(runtime.path().join("limiter-native.log")).unwrap()
            );
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("Native Limiter.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..48000 {
        let value = (frame as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let document = loop {
        let state = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if state["status"] != "running" {
            assert_eq!(state["status"], "succeeded", "{state}");
            break state["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":document["id"]});
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    let state = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"limiter"}}),
    ));
    let effect = state["committed"]["effects"][0]["id"].clone();
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    let editor = opened["editor_windows"][0]["id"].clone();
    if std::env::var_os("WAVEBOB_REVIEW_MANUAL_OUTPUT").is_some() {
        success(request(
            &client,
            "transport.loop.set",
            json!({"enabled":true}),
        ));
        success(request(
            &client,
            "transport.play",
            json!({"document_id":document["id"]}),
        ));
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let t = success(request(&client, "transport.get", json!({})));
            if t["effect_meters"][0]["columns"]
                .as_array()
                .is_some_and(|c| c.len() >= 8)
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "No actual manual-output meter columns were published: {t}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let first_response = request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000}),
    );
    if first_response["status"] != "ok" {
        retain(
            &runtime.path().join("limiter-native.log"),
            "native-capture-failure.log",
        );
    }
    let first = success(first_response);
    assert_eq!(first["metadata"]["owner"], owner);
    assert_eq!(first["metadata"]["instance_id"], effect);
    assert_eq!(first["metadata"]["editor_id"], editor);
    assert!(first["metadata"]["processing_identity"].is_string());
    let (width, height, before) = pixels(Path::new(first["path"].as_str().unwrap()));
    assert!(width >= 320 && height >= 240);
    retain(
        Path::new(first["path"].as_str().unwrap()),
        "limiter-default.png",
    );
    let state = success(request(&client, "processing.get", json!({"owner":owner})));
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"c_thres","value":500}}),
    ));
    for (key, value) in [("c_ratio", 1000), ("c_knee", 700)] {
        let state = success(request(&client, "processing.get", json!({"owner":owner})));
        success(request(
            &client,
            "processing.edit",
            json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":key,"value":value}}),
        ));
    }
    if let Some(input) = std::env::var_os("WAVEBOB_REVIEW_INPUT_FILE") {
        // Logical coordinates of the redesigned 850x530 native viewport.
        for (sequence, pressed) in [(1, true), (2, false)] {
            fs::write(&input,serde_json::to_vec(&json!({"editor_id":editor,"pointer":{"x":286.0,"y":75.0,"pressed":pressed,"sequence":sequence}})).unwrap()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let second = success(request(
        &client,
        "capture.png",
        json!({"editor_id":editor,"timeout_ms":5000}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    if std::env::var_os("WAVEBOB_REVIEW_INPUT_FILE").is_some() {
        let (w, h, data) = pixels(Path::new(second["path"].as_str().unwrap()));
        let stride = data.len() / (w * h) as usize;
        let scale = w as f64 / 850.0;
        let point = ((70.0 * scale).round() as usize * w as usize
            + (260.0 * scale).round() as usize)
            * stride;
        assert!(
            data[point] < 60 && data[point + 1] > 80 && data[point + 2] > 200,
            "The real COMP tab did not receive the QA pointer click"
        );
    }

    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and transfer curve"
    );
    assert_ne!(
        first["metadata"]["processing_identity"],
        second["metadata"]["processing_identity"]
    );
    retain(
        Path::new(second["path"].as_str().unwrap()),
        "limiter-compression.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        let transport = success(request(&client, "transport.get", json!({})));
        let meter = transport["effect_meters"]
            .as_array()
            .and_then(|v| v.iter().find(|m| m["instance_id"] == effect))
            .cloned();
        fs::write(Path::new(&directory).join("limiter-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"limiter-default.png","metadata":first["metadata"]},"compression":{"image":"limiter-compression.png","metadata":second["metadata"]},"native_input_injected":std::env::var_os("WAVEBOB_REVIEW_INPUT_FILE").is_some(),"output_kind":transport["output"]["kind"],"actual_instance_meter":meter})).unwrap()).unwrap();
    }
    let opened = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(opened["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(opened["editor_windows"][0]["id"], editor);
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":effect}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(&client, "capture.png", json!({"editor_id":editor}))["error"]["code"],
        "not_found"
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

// Shared actual-viewport capture for compact effects. No synthetic pixel renderer.
fn compact_editor_capture(kind: &str, changes: &[(&str, i32)]) {
    let runtime = tempfile::tempdir().unwrap();
    let name = format!("{kind}-native");
    let mut gui = Gui::start(runtime.path(), &name);
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        if let Some(i) = ipc::discover(runtime.path())
            .unwrap()
            .into_iter()
            .find(|i| i.name == name)
        {
            break i;
        }
        assert!(gui.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let source = runtime.path().join("Utility.wav");
    let mut wav = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for i in 0..1024 {
        wav.write_sample((i as f32 * 0.147).sin()).unwrap();
    }
    wav.finalize().unwrap();
    let job = success(request(&client, "documents.open", json!({"path":source})));
    let doc = loop {
        let j = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
        if j["status"] != "running" {
            assert_eq!(j["status"], "succeeded");
            break j["result"].clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let owner = json!({"kind":"document","id":doc["id"]});
    let p = success(request(&client, "processing.get", json!({"owner":owner})));
    let p = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":p["committed"]["edit_revision_id"],"change":{"action":"add","kind":kind}}),
    ));
    let id = p["committed"]["effects"][0]["id"].clone();
    let w = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":id}),
    ));
    let editor = w["editor_windows"][0]["id"].clone();
    let capture = |label: &str| {
        let c = success(request(
            &client,
            "capture.png",
            json!({"editor_id":editor,"timeout_ms":5000}),
        ));
        assert_eq!(c["metadata"]["owner"], owner);
        assert_eq!(c["metadata"]["instance_id"], id);
        retain(
            Path::new(c["path"].as_str().unwrap()),
            &format!("{kind}-{label}.png"),
        );
        c
    };
    let first = capture("default");
    for &(key, value) in changes {
        let p = success(request(&client, "processing.get", json!({"owner":owner})));
        success(request(
            &client,
            "processing.edit",
            json!({"owner":owner,"expected_edit_revision":p["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":id,"parameter_id":key,"value":value}}),
        ));
    }
    let second = capture("configured");
    assert_ne!(
        pixels(Path::new(first["path"].as_str().unwrap())).2,
        pixels(Path::new(second["path"].as_str().unwrap())).2
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(
            Path::new(&directory).join(format!("{kind}-captures.json")),
            serde_json::to_vec_pretty(
                &json!({"default":first["metadata"],"configured":second["metadata"]}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    let w = success(request(
        &client,
        "effects.editor.open",
        json!({"owner":owner,"instance_id":id}),
    ));
    assert_eq!(w["editor_windows"].as_array().unwrap().len(), 1);
    assert_eq!(w["editor_windows"][0]["id"], editor);
    success(request(
        &client,
        "effects.editor.close",
        json!({"owner":owner,"instance_id":id}),
    ));
    assert_eq!(
        success(request(&client, "processing.get", json!({"owner":owner})))["committed"]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}
#[test]
#[ignore = "requires native effect viewport"]
fn native_balance_editor_capture() {
    compact_editor_capture("balance", &[("balance", -128), ("volume", 320)]);
}

#[test]
#[ignore = "requires native effect viewport"]
fn native_stereo_enhancer_editor_capture() {
    compact_editor_capture(
        "stereo_enhancer",
        &[
            ("separation", -54),
            ("phase", -248),
            ("placement", 0),
            ("invert", 2),
        ],
    );
}

#[test]
#[ignore = "requires native effect viewport"]
fn native_stereo_shaper_editor_capture() {
    compact_editor_capture(
        "stereo_shaper",
        &[
            ("rl", -12800),
            ("left", 8076),
            ("lr", 12800),
            ("delay", 1408),
            ("phase", -2496),
            ("position", 0),
        ],
    );
}

#[test]
#[ignore = "requires native effect viewport"]
fn native_transient_processor_editor_capture() {
    compact_editor_capture(
        "transient_processor",
        &[
            ("attack", 127),
            ("drive", 93),
            ("release", 41),
            ("gain", 113),
            ("attack_shape", 2),
            ("release_shape", 0),
            ("split", 51),
            ("balance", 129),
            ("on", 0),
        ],
    );
}

#[test]
#[ignore = "requires native arrangement renderer"]
fn native_linked_bounce_tail_capture_after_mode_freshness_barrier() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "linked-tail-native");
    let deadline = Instant::now() + Duration::from_secs(30);
    let instance = loop {
        if let Some(i) = ipc::discover(runtime.path())
            .unwrap()
            .into_iter()
            .find(|i| i.name == "linked-tail-native")
        {
            break i;
        }
        assert!(gui.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
    let mut docs = Vec::new();
    for (name, late) in [("Linked tail.wav", true), ("Later body.wav", false)] {
        let path = runtime.path().join(name);
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
        for i in 0..12000 {
            let x = (i as f32 * 0.071).sin() * 0.2;
            wav.write_sample(x).unwrap();
            wav.write_sample(if late {
                if i == 0 { 1.0 } else { 0.0 }
            } else {
                x * 0.7
            })
            .unwrap();
        }
        wav.finalize().unwrap();
        let job = success(request(&client, "documents.open", json!({"path":path})));
        loop {
            let j = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
            if j["status"] != "running" {
                assert_eq!(j["status"], "succeeded", "{j}");
                docs.push(j["result"].clone());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let owner = json!({"kind":"document","id":docs[0]["id"]});
    let p = success(request(&client, "processing.get", json!({"owner":owner})));
    let p = success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":p["committed"]["edit_revision_id"],"change":{"action":"add","kind":"stereo_enhancer"}}),
    ));
    let effect = p["committed"]["effects"][0]["id"].clone();
    success(request(
        &client,
        "processing.edit",
        json!({"owner":owner,"expected_edit_revision":p["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":effect,"parameter_id":"phase","value":512}}),
    ));
    let a = success(request(&client, "tracks.add", json!({})));
    let track = a["tracks"][0]["id"].clone();
    for (d, position) in docs.iter().zip([0, 16000]) {
        success(request(
            &client,
            "clips.insert",
            json!({"track_id":track,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":0,"source_end":12000,"position":position}),
        ));
    }
    success(request(
        &client,
        "presentation.set_mode",
        json!({"mode":"multitrack"}),
    ));
    let mix = loop {
        let m = success(request(&client, "mix.get", json!({})));
        assert!(m["preparation_error"].is_null(), "{m}");
        if m["required_render_identity"] == m["prepared_render_identity"] {
            break m;
        }
        assert!(Instant::now() < deadline, "{m}");
        std::thread::sleep(Duration::from_millis(10));
    };
    let a = success(request(&client, "arrangement.get", json!({})));
    assert!(a["tracks"][0]["clips"][0]["tail_frames"].as_u64().unwrap() > 12000);
    assert_eq!(a["tracks"][0]["clips"][1]["position"], 16000);
    assert_eq!(
        success(request(&client, "arrangement.crossfades", json!({})))["regions"],
        json!([])
    );
    success(request(
        &client,
        "presentation.timeline.set",
        json!({"start_frame":0,"end_frame":48000}),
    ));
    let capture = success(request(&client, "capture.png", json!({"timeout_ms":5000})));
    let (w, h, _) = pixels(Path::new(capture["path"].as_str().unwrap()));
    assert!(w >= 1000 && h >= 600);
    retain(
        Path::new(capture["path"].as_str().unwrap()),
        "linked-body-and-tail.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("linked-body-and-tail.json"),serde_json::to_vec_pretty(&json!({"capture":capture["metadata"],"arrangement":a,"mix":mix,"physical_listening":"unverified"})).unwrap()).unwrap();
    }
    success(request(
        &client,
        "presentation.quit",
        json!({"discard_changes":true}),
    ));
}

#[path = "../examples/support/live_effects_workflow.rs"]
mod live_effects_workflow;
#[path = "../examples/support/workflow.rs"]
#[allow(dead_code)] // This test uses the shared adapter; the original scenario has its own suite.
mod workflow;
#[tokio::test]
#[ignore = "requires actual native GUI for final attached-MCP live effects workflow"]
async fn native_live_effects_portable_workflow_through_attached_mcp() {
    for (name, size, scale) in [
        ("normal", "1440x900", "1"),
        ("narrow", "800x600", "1"),
        ("high-dpi", "1440x900", "1.5"),
    ] {
        let runtime = tempfile::tempdir().unwrap();
        let mut gui = Gui::start_options(runtime.path(), name, size, scale);
        let deadline = Instant::now() + Duration::from_secs(30);
        let instance = loop {
            if let Some(i) = ipc::discover(runtime.path())
                .unwrap()
                .into_iter()
                .find(|i| i.name == name)
            {
                break i;
            }
            assert!(gui.0.try_wait().unwrap().is_none());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut adapter = workflow::Adapter::connect(
            "mcp",
            Some(env!("CARGO_BIN_EXE_wavebob-mcp")),
            Some(&instance.session_id),
            Some(runtime.path().to_str().unwrap()),
        )
        .await;
        let root = runtime.path().join("scenario");
        live_effects_workflow::run(&mut adapter, &root, true).await;
        for file in [
            "waveform.png",
            "multitrack.png",
            "effect-0.png",
            "effect-1.png",
            "effect-2.png",
            "workflow.json",
            "native-playback.json",
        ] {
            retain(&root.join(file), &format!("{name}-{file}"));
        }
        adapter
            .ok("presentation.quit", json!({"discard_changes":true}))
            .await;
        adapter.shutdown().await;
    }
}

#[path = "gui_attachment/redesign.rs"]
mod redesign;
