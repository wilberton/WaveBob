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
        let mut command = Command::new(env!("CARGO_BIN_EXE_wavebob"));
        command
            .args(["--hidden", "--name", name, "--runtime-dir"])
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
    assert_eq!(catalogue["commands"].as_array().unwrap().len(), 41);
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
