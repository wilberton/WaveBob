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
        Self::start_options(directory, name, "1440x900", "1")
    }
    fn start_options(directory: &Path, name: &str, size: &str, scale: &str) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_wavebob"));
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
        json!({"editor_id":editor,"timeout_ms":5000}),
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
        "soft-clipper-threshold-32.png",
    );
    if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(Path::new(&directory).join("soft-clipper-captures.json"),serde_json::to_vec_pretty(&json!({"default":{"image":"soft-clipper-default.png","metadata":first["metadata"]},"threshold_32":{"image":"soft-clipper-threshold-32.png","metadata":second["metadata"]}})).unwrap()).unwrap();
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
fn native_reverb_editor_capture_and_stable_window_lifecycle() {
    let runtime = tempfile::tempdir().unwrap();
    let mut gui = Gui::start(runtime.path(), "reverb-native");
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
    let source = runtime.path().join("Native Reverb.wav");
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
        json!({"editor_id":editor,"timeout_ms":5000}),
    ));
    let (_, _, after) = pixels(Path::new(second["path"].as_str().unwrap()));
    assert_ne!(
        before, after,
        "The real editor must redraw committed controls and room visualization"
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
        json!({"editor_id":editor,"timeout_ms":5000}),
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
    let mut gui = Gui::start(runtime.path(), "dist-native");
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
    let source = runtime.path().join("Native Fast Distortion.wav");
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
        json!({"editor_id":editor,"timeout_ms":5000}),
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
        json!({"editor_id":editor,"timeout_ms":5000}),
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
        json!({"editor_id":editor,"timeout_ms":5000}),
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
