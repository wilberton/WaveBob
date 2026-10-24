use std::process::Command;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation,
    ProtocolVersion,
};
use rmcp::service::{RoleClient, RunningService};
use rmcp::{ServiceExt, transport::TokioChildProcess};
use serde_json::{Value, json};
use wavebob::{Command as EditorCommand, EmptyParams, LoopParams, Request, Session, protocol};
#[cfg(feature = "gui")]
#[path = "../examples/support/workflow.rs"]
mod workflow;

async fn connect(
    version: ProtocolVersion,
    args: &[&std::ffi::OsStr],
) -> RunningService<RoleClient, ClientConfig> {
    let executable = std::env::var_os("WAVEBOB_MCP_TEST_BIN")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_wavebob-mcp").into());
    let mut command = tokio::process::Command::new(executable);
    command.args(args);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let transport = TokioChildProcess::new(command).unwrap();
    let config = ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("wavebob-contract", "1"),
    )
    .with_protocol_version(version);
    tokio::time::timeout(Duration::from_secs(15), config.serve(transport))
        .await
        .unwrap()
        .unwrap()
}

async fn call(
    client: &RunningService<RoleClient, ClientConfig>,
    name: &str,
    arguments: Value,
) -> CallToolResult {
    tokio::time::timeout(
        Duration::from_secs(15),
        client.call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(arguments.as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

fn response(result: &CallToolResult) -> Value {
    let value = result
        .structured_content
        .clone()
        .expect("Structured command response");
    assert_eq!(result.is_error.unwrap_or(false), value["status"] == "error");
    let text = result
        .content
        .iter()
        .find_map(|block| block.as_text())
        .expect("Text fallback for older clients");
    assert_eq!(serde_json::from_str::<Value>(&text.text).unwrap(), value);
    assert!(!value["session_id"].as_str().unwrap().is_empty());
    value
}

fn assert_same_audio(actual: &Value, expected: &Value) {
    let actual = actual.as_array().unwrap();
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    for (channel, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let actual = actual.as_array().unwrap();
        let expected = expected.as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (frame, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(
                actual.as_f64(),
                expected.as_f64(),
                "channel {channel}, frame {frame}"
            );
        }
    }
}

fn finish_sync_job(call: &mut impl FnMut(&str, Value) -> Value, started: Value) -> Value {
    assert_eq!(started["status"], "ok", "{started}");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let job = call("jobs.get", json!({"job_id":started["result"]["id"]}));
        if job["result"]["status"] != "running" {
            assert_eq!(job["result"]["status"], "succeeded", "{job}");
            return job["result"]["result"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn finish_sync_output(call: &mut impl FnMut(&str, Value) -> Value, started: Value) -> Value {
    assert_eq!(started["status"], "ok", "{started}");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let job = call("jobs.get", json!({"job_id":started["result"]["id"]}));
        if job["result"]["status"] != "running" {
            assert_eq!(job["result"]["status"], "succeeded", "{job}");
            return job["result"]["output"]["details"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}
async fn finish_mcp_output(
    client: &RunningService<RoleClient, ClientConfig>,
    started: Value,
) -> Value {
    assert_eq!(started["status"], "ok", "{started}");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let job = response(
            &call(
                client,
                "jobs.get",
                json!({"params":{"job_id":started["result"]["id"]}}),
            )
            .await,
        );
        if job["result"]["status"] != "running" {
            assert_eq!(job["result"]["status"], "succeeded", "{job}");
            return job["result"]["output"]["details"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
async fn finish_mcp_job(
    client: &RunningService<RoleClient, ClientConfig>,
    started: Value,
) -> Value {
    assert_eq!(started["status"], "ok", "{started}");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let job = response(
            &call(
                client,
                "jobs.get",
                json!({"params":{"job_id":started["result"]["id"]}}),
            )
            .await,
        );
        if job["result"]["status"] != "running" {
            assert_eq!(job["result"]["status"], "succeeded", "{job}");
            return job["result"]["result"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

fn assert_local_refs(schema: &Value, root: &Value) {
    match schema {
        Value::Object(fields) => {
            if let Some(Value::String(reference)) = fields.get("$ref") {
                assert!(
                    reference.starts_with('#') && root.pointer(&reference[1..]).is_some(),
                    "Unresolved schema reference {reference}"
                );
            }
            for child in fields.values() {
                assert_local_refs(child, root);
            }
        }
        Value::Array(items) => {
            for child in items {
                assert_local_refs(child, root);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn stdio_contract_matches_direct_and_headless_on_both_protocol_eras() {
    for version in [ProtocolVersion::V_2025_11_25, ProtocolVersion::V_2026_07_28] {
        let client = connect(version, &[]).await;
        let tools = client.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 71);
        let catalogue = response(&call(&client, "session.capabilities", json!({})).await);
        for descriptor in catalogue["result"]["commands"].as_array().unwrap() {
            let tool = tools
                .iter()
                .find(|tool| tool.name == descriptor["name"].as_str().unwrap())
                .unwrap();
            assert_eq!(
                tool.description.as_deref().unwrap(),
                descriptor["description"]
            );
            assert_eq!(
                tool.input_schema["properties"]["params"]["properties"],
                descriptor["input_schema"]["properties"]
            );
            if descriptor["input_schema"]["required"]
                .as_array()
                .is_some_and(|fields| !fields.is_empty())
            {
                assert_eq!(tool.input_schema["required"], json!(["params"]));
            }
            assert_eq!(
                tool.output_schema.as_ref().unwrap().as_ref(),
                catalogue["result"]["response_schema"].as_object().unwrap()
            );
            let schema = Value::Object((*tool.input_schema).clone());
            assert_local_refs(&schema, &schema);
        }
        let session_id = catalogue["session_id"].as_str().unwrap();
        let mut direct = Session::default();
        let mut headless = Session::default();
        for (name, params, command, expected_revision) in [
            (
                "transport.loop.set",
                json!({"enabled":true}),
                EditorCommand::SetLoop(LoopParams { enabled: true }),
                Some(0),
            ),
            (
                "transport.loop.set",
                json!({"enabled":true}),
                EditorCommand::SetLoop(LoopParams { enabled: true }),
                Some(1),
            ),
            (
                "transport.loop.set",
                json!({"enabled":false}),
                EditorCommand::SetLoop(LoopParams { enabled: false }),
                Some(0),
            ),
            (
                "session.get",
                json!({}),
                EditorCommand::GetSession(EmptyParams {}),
                None,
            ),
            (
                "transport.loop.toggle",
                json!({}),
                EditorCommand::ToggleLoop(EmptyParams {}),
                Some(1),
            ),
        ] {
            let mcp = response(&call(&client, name, json!({"params":params, "expected_revision":expected_revision,"session_id":session_id})).await);
            let mut request = Request::new("equivalence", command);
            request.expected_revision = expected_revision;
            let direct = direct.execute(request);
            let headless = protocol::dispatch_line(&mut headless, &json!({"id":"equivalence","command":name,"params":params,"expected_revision":expected_revision}).to_string());
            for expected in [direct, headless] {
                assert_eq!(mcp["revision"], expected.revision);
                let mut expected = serde_json::to_value(expected).unwrap();
                let mut actual = mcp.clone();
                for value in [&mut actual, &mut expected] {
                    value.as_object_mut().unwrap().remove("id");
                    value.as_object_mut().unwrap().remove("session_id");
                    if let Some(object) = value["result"].as_object_mut() {
                        object.remove("session_id");
                        if let Some(project) =
                            object.get_mut("project").and_then(Value::as_object_mut)
                        {
                            project.remove("revision_id");
                            project.remove("saved_revision_id");
                        }
                        if let Some(arrangement) =
                            object.get_mut("arrangement").and_then(Value::as_object_mut)
                        {
                            arrangement.remove("revision_id");
                            arrangement.remove("id");
                            arrangement.remove("saved_revision_id");
                        }
                    }
                }
                assert_eq!(actual, expected);
            }
        }
        assert_eq!(
            response(&call(&client, "capture.png", json!({})).await)["error"]["code"],
            "missing_capability"
        );
        let invalid = response(
            &call(
                &client,
                "transport.loop.set",
                json!({"params":{"enabled":true,"extra":1}}),
            )
            .await,
        );
        assert_eq!(invalid["error"]["code"], "invalid_params");
        assert_eq!(invalid["revision"], 2);
        assert_eq!(
            response(&call(&client, "does.not.exist", json!({})).await)["error"]["code"],
            "unknown_command"
        );
        assert_eq!(
            response(&call(&client, "session.get", json!({})).await)["revision"],
            2
        );
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn target_guards_and_invalid_envelopes_leave_the_session_usable() {
    let client = connect(ProtocolVersion::V_2026_07_28, &[]).await;
    for (arguments, code) in [
        (json!({"session_id":"different-session"}), "invalid_request"),
        (json!({"unexpected":1}), "invalid_params"),
        (json!({"expected_revision":"wrong-type"}), "invalid_params"),
    ] {
        let error = client
            .call_tool(
                CallToolRequestParams::new("transport.loop.toggle")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await
            .unwrap_err();
        let rmcp::ServiceError::McpError(error) = error else {
            panic!("Expected structured protocol error, got {error}");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert_eq!(error.data.unwrap()["code"], code);
    }
    let snapshot = response(&call(&client, "session.get", json!({})).await);
    assert_eq!(snapshot["revision"], 0);
    assert_eq!(snapshot["result"]["loop_enabled"], false);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mp3_edit_and_mix_output_match_direct_headless_and_mcp_adapters() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3");
    let mut session = Session::default();
    let outputs = tempfile::tempdir().unwrap();
    let export_params = |doc: &Value, path: &std::path::Path| json!({"target":{"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":500,"end_frame":4596},"path":path,"format":{"kind":"wav","encoding":"float32"}});
    let mut direct = |name: &str, params: Value| {
        serde_json::to_value(protocol::dispatch_line(
            &mut session,
            &json!({"id":"audio","command":name,"params":params}).to_string(),
        ))
        .unwrap()
    };
    let started = direct("documents.open", json!({"path":path}));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let doc = loop {
        let job = direct("jobs.get", json!({"job_id":started["result"]["id"]}));
        if job["result"]["status"] != "running" {
            assert_eq!(job["result"]["status"], "succeeded");
            break job["result"]["result"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    let started = direct(
        "edit.process",
        json!({"target":{"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":500,"end_frame":4596},"effect":{"kind":"gain","db":6.0}}),
    );
    let edited = finish_sync_job(&mut direct, started);
    let started = direct(
        "audio.export",
        export_params(&edited, &outputs.path().join("direct.wav")),
    );
    finish_sync_output(&mut direct, started);
    let expected = direct(
        "documents.samples",
        json!({"document_id":doc["id"],"start_frame":500,"frames":4096}),
    )["result"]["channels"]
        .clone();
    fn clip_params(doc: &Value, track: &Value, position: u64) -> Value {
        json!({"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":500,"source_end":4596,"position":position})
    }
    fn mix_params(revision: &Value, path: &std::path::Path) -> Value {
        json!({"revision_id":revision,"start_frame":0,"end_frame":7200,"path":path,"format":{"kind":"wav","encoding":"float32"}})
    }
    fn arrange_output(
        call: &mut impl FnMut(&str, Value) -> Value,
        doc: &Value,
        path: &std::path::Path,
    ) {
        let arr = call("tracks.add", json!({"name":"Adapter mix"}));
        let track = &arr["result"]["tracks"][0]["id"];
        let mut revision = Value::Null;
        for position in [0, 2200] {
            let arr = call("clips.insert", clip_params(doc, track, position));
            assert_eq!(arr["status"], "ok", "{arr}");
            revision = arr["result"]["revision_id"].clone();
        }
        let started = call("mix.export", mix_params(&revision, path));
        finish_sync_output(call, started);
    }
    arrange_output(&mut direct, &edited, &outputs.path().join("direct-mix.wav"));
    let mut process = Command::new(env!("CARGO_BIN_EXE_wavebob-headless"));
    process.stdin(Stdio::piped()).stdout(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    let mut child = process.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut wire = |name: &str, params: Value| {
        writeln!(
            input,
            "{}",
            json!({"id":"audio","command":name,"params":params})
        )
        .unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    let started = wire("documents.open", json!({"path":path}));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let doc = loop {
        let job = wire("jobs.get", json!({"job_id":started["result"]["id"]}));
        if job["result"]["status"] != "running" {
            assert_eq!(job["result"]["status"], "succeeded");
            break job["result"]["result"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    let started = wire(
        "edit.process",
        json!({"target":{"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":500,"end_frame":4596},"effect":{"kind":"gain","db":6.0}}),
    );
    let edited = finish_sync_job(&mut wire, started);
    let started = wire(
        "audio.export",
        export_params(&edited, &outputs.path().join("headless.wav")),
    );
    finish_sync_output(&mut wire, started);
    assert_eq!(
        std::fs::read(outputs.path().join("direct.wav")).unwrap(),
        std::fs::read(outputs.path().join("headless.wav")).unwrap()
    );
    assert_same_audio(
        &wire(
            "documents.samples",
            json!({"document_id":doc["id"],"start_frame":500,"frames":4096}),
        )["result"]["channels"],
        &expected,
    );
    arrange_output(&mut wire, &edited, &outputs.path().join("headless-mix.wav"));
    assert_eq!(
        std::fs::read(outputs.path().join("direct-mix.wav")).unwrap(),
        std::fs::read(outputs.path().join("headless-mix.wav")).unwrap()
    );
    drop(wire);
    drop(input);
    assert!(child.wait().unwrap().success());
    let client = connect(ProtocolVersion::V_2026_07_28, &[]).await;
    let started = response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let doc = loop {
        let job = response(
            &call(
                &client,
                "jobs.get",
                json!({"params":{"job_id":started["result"]["id"]}}),
            )
            .await,
        );
        if job["result"]["status"] != "running" {
            assert_eq!(job["result"]["status"], "succeeded");
            break job["result"]["result"].clone();
        }
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    };
    let started=response(&call(&client,"edit.process",json!({"params":{"target":{"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":500,"end_frame":4596},"effect":{"kind":"gain","db":6.0}}})).await);
    let edited = finish_mcp_job(&client, started).await;
    let started = response(
        &call(
            &client,
            "audio.export",
            json!({"params":export_params(&edited,&outputs.path().join("mcp.wav"))}),
        )
        .await,
    );
    finish_mcp_output(&client, started).await;
    assert_eq!(
        std::fs::read(outputs.path().join("direct.wav")).unwrap(),
        std::fs::read(outputs.path().join("mcp.wav")).unwrap()
    );
    assert_same_audio(
        &response(
            &call(
                &client,
                "documents.samples",
                json!({"params":{"document_id":doc["id"],"start_frame":500,"frames":4096}}),
            )
            .await,
        )["result"]["channels"],
        &expected,
    );
    let arr = response(
        &call(
            &client,
            "tracks.add",
            json!({"params":{"name":"Adapter mix"}}),
        )
        .await,
    );
    let track = &arr["result"]["tracks"][0]["id"];
    let mut revision = Value::Null;
    for position in [0, 2200] {
        let arr = response(
            &call(
                &client,
                "clips.insert",
                json!({"params":clip_params(&edited,track,position)}),
            )
            .await,
        );
        assert_eq!(arr["status"], "ok", "{arr}");
        revision = arr["result"]["revision_id"].clone();
    }
    let started = response(
        &call(
            &client,
            "mix.export",
            json!({"params":mix_params(&revision,&outputs.path().join("mcp-mix.wav"))}),
        )
        .await,
    );
    finish_mcp_output(&client, started).await;
    assert_eq!(
        std::fs::read(outputs.path().join("direct-mix.wav")).unwrap(),
        std::fs::read(outputs.path().join("mcp-mix.wav")).unwrap()
    );
    client.cancel().await.unwrap();
}

#[test]
fn cli_rejects_invalid_or_missing_targets_on_stderr_without_starting_a_session() {
    for args in [
        vec!["--unknown"],
        vec!["--attach"],
        vec!["--attach", "not-a-uuid"],
        vec!["--attach", "x", "--list-instances"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_wavebob-mcp"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[cfg(feature = "gui")]
mod live {
    use super::*;
    use base64::prelude::*;
    use rmcp::model::ContentBlock;
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::process::Stdio;
    use std::time::Instant;
    use wavebob::ipc;

    struct Gui(std::process::Child);
    impl Gui {
        fn start(runtime: &Path, name: &str) -> Self {
            Self::with_profile(
                runtime,
                name,
                &runtime.join(format!("{name}-layout.json")),
                "1440x900",
            )
        }
        fn with_profile(runtime: &Path, name: &str, preferences: &Path, size: &str) -> Self {
            Self::with_scale(runtime, name, preferences, size, "1.0")
        }
        fn with_scale(
            runtime: &Path,
            name: &str,
            preferences: &Path,
            size: &str,
            scale: &str,
        ) -> Self {
            let executable = std::env::var_os("WAVEBOB_GUI_TEST_BIN")
                .unwrap_or_else(|| env!("CARGO_BIN_EXE_wavebob").into());
            let mut command = Command::new(executable);
            command
                .args(["--hidden", "--name", name, "--runtime-dir"])
                .arg(runtime)
                .arg("--preferences")
                .arg(preferences)
                .args(["--window-size", size])
                .args(["--ui-scale", scale])
                .env(
                    "WAVEBOB_REVIEW_INPUT_FILE",
                    runtime.join("review-input.json"),
                )
                .stdout(Stdio::null())
                .stderr(fs::File::create(runtime.join(format!("{name}.log"))).unwrap());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            Self(command.spawn().unwrap())
        }
    }

    async fn named_client(runtime: &Path, name: &str) -> RunningService<RoleClient, ClientConfig> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let id = loop {
            if let Some(instance) = ipc::discover(runtime)
                .unwrap()
                .into_iter()
                .find(|instance| instance.name == name)
            {
                break instance.session_id;
            }
            assert!(Instant::now() < deadline, "GUI did not register {name}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        connect(
            ProtocolVersion::V_2026_07_28,
            &[
                "--attach".as_ref(),
                id.as_ref(),
                "--runtime-dir".as_ref(),
                runtime.as_os_str(),
            ],
        )
        .await
    }

    #[tokio::test]
    #[ignore = "requires the native gui-review example for one-shot OS file-event injection"]
    async fn native_file_drop_batch_and_continuous_waveform_captures() {
        assert!(
            std::env::var_os("WAVEBOB_GUI_TEST_BIN").is_some_and(|path| Path::new(&path)
                .file_stem()
                .is_some_and(|stem| stem == "gui-review"))
        );
        let runtime = tempfile::tempdir().unwrap();
        let source = runtime.path().join("Stereo texture.wav");
        let mut writer = hound::WavWriter::create(
            &source,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..48000 {
            let t = frame as f64 / 48000.0;
            for channel in 0..2 {
                let phase = t * std::f64::consts::TAU * (120.0 + channel as f64 * 37.0);
                let value = 0.38 * phase.sin() + 0.12 * (phase * 2.7).sin();
                writer.write_sample((value * 32767.0) as i16).unwrap();
            }
        }
        writer.finalize().unwrap();
        let mut paths = vec![source.clone()];
        for name in ["Second.WAV", "音色 third.wav"] {
            let path = runtime.path().join(name);
            fs::copy(&source, &path).unwrap();
            paths.push(path);
        }
        let mp3 = runtime.path().join("Fourth.MP3");
        fs::copy("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3", &mp3).unwrap();
        paths.push(mp3);
        let input_path = runtime.path().join("review-input.json");
        fs::write(
            &input_path,
            serde_json::to_vec(&json!({"hover_files":paths})).unwrap(),
        )
        .unwrap();
        let scale = std::env::var("WAVEBOB_DROP_TEST_SCALE").unwrap_or("1.0".into());
        let _gui = Gui::with_scale(
            runtime.path(),
            "file-drop-review",
            &runtime.path().join("layout.json"),
            "1440x900",
            &scale,
        );
        let client = named_client(runtime.path(), "file-drop-review").await;
        let mut records = Vec::new();
        async fn capture(
            client: &RunningService<RoleClient, ClientConfig>,
            runtime: &Path,
            label: &str,
            records: &mut Vec<Value>,
        ) {
            let path = runtime.join(format!("{label}.png"));
            let result = call(client, "capture.png", json!({"params":{"path":path}})).await;
            let captured = response(&result);
            assert_eq!(captured["status"], "ok", "{captured}");
            let image = result
                .content
                .iter()
                .find_map(|block| match block {
                    ContentBlock::Image(image) => Some(image),
                    _ => None,
                })
                .unwrap();
            let png = BASE64_STANDARD.decode(&image.data).unwrap();
            assert_eq!(png, fs::read(path).unwrap());
            if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
                fs::create_dir_all(&directory).unwrap();
                fs::write(Path::new(&directory).join(format!("{label}.png")), png).unwrap();
            }
            records.push(json!({"file":format!("{label}.png"),"capture":captured}));
        }
        capture(&client, runtime.path(), "file-drop-hover", &mut records).await;
        let initial = response(&call(&client, "session.get", json!({})).await);
        assert_eq!(initial["result"]["documents"], json!([]));
        assert!(
            records[0]["capture"]["result"]["metadata"]["pending_resources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item == "external-file-hover")
        );
        call(
            &client,
            "presentation.set_mode",
            json!({"params":{"mode":"multitrack"}}),
        )
        .await;
        fs::write(
            &input_path,
            serde_json::to_vec(&json!({
                "drop_sequence":1,"drop_files":paths
            }))
            .unwrap(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let loaded = loop {
            let state = response(&call(&client, "session.get", json!({})).await);
            let jobs = state["result"]["jobs"].as_array().unwrap();
            assert!(jobs.iter().filter(|job| job["status"] == "running").count() <= 2);
            if state["result"]["documents"].as_array().unwrap().len() == 4
                && jobs.iter().all(|job| job["status"] == "succeeded")
            {
                break state;
            }
            assert!(Instant::now() < deadline, "{state}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let documents = loaded["result"]["documents"].as_array().unwrap();
        assert!(
            documents
                .iter()
                .any(|document| document["encoding"] == "mp3")
        );
        assert_eq!(loaded["result"]["arrangement"]["tracks"], json!([]));
        let document = documents
            .iter()
            .find(|doc| doc["name"] == "Stereo texture.wav")
            .unwrap();
        call(
            &client,
            "documents.activate",
            json!({"params":{"document_id":document["id"]}}),
        )
        .await;
        for (label, start, end) in [
            ("waveform-whole", 0, 48000),
            ("waveform-intermediate", 8000, 12096),
            ("waveform-samples", 8000, 8064),
        ] {
            let view = response(&call(&client, "presentation.viewport.set",
                json!({"params":{"document_id":document["id"],"start_frame":start,"end_frame":end}})).await);
            assert_eq!(view["status"], "ok", "{view}");
            capture(&client, runtime.path(), label, &mut records).await;
        }
        let final_state = response(&call(&client, "session.get", json!({})).await);
        assert_eq!(
            final_state["result"]["documents"].as_array().unwrap().len(),
            4
        );
        assert_eq!(final_state["result"]["jobs"].as_array().unwrap().len(), 4);
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&directory).join("file-drop-waveforms.json"),
                serde_json::to_vec_pretty(
                    &json!({"scale":scale,"loaded":loaded,"captures":records}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer and WAVEBOB_GUI_TEST_BIN pointing to the gui-review example"]
    async fn native_status_hover_and_keyboard_focus_capture() {
        assert!(
            std::env::var_os("WAVEBOB_GUI_TEST_BIN").is_some_and(|path| Path::new(&path)
                .file_stem()
                .is_some_and(|stem| stem == "gui-review")),
            "Point WAVEBOB_GUI_TEST_BIN to the QA-only gui-review example"
        );
        let runtime = tempfile::tempdir().unwrap();
        let gui = Gui::start(runtime.path(), "status-help-check");
        let client = named_client(runtime.path(), "status-help-check").await;
        let binary = std::env::var("WAVEBOB_MCP_TEST_BIN")
            .unwrap_or(env!("CARGO_BIN_EXE_wavebob-mcp").into());
        let id = response(&call(&client, "session.get", json!({})).await)["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        client.cancel().await.unwrap();
        let mut adapter =
            workflow::Adapter::connect("mcp", Some(&binary), Some(&id), runtime.path().to_str())
                .await;
        fs::write(
            runtime.path().join("review-input.json"),
            r#"{"hover":[115,18]}"#,
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let hover = runtime.path().join("status-hover.png");
        adapter.capture(&hover).await;
        fs::write(
            runtime.path().join("review-input.json"),
            r#"{"focus_tab":true}"#,
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let focus = runtime.path().join("status-focus.png");
        adapter.capture(&focus).await;
        for path in [&hover, &focus] {
            let mut reader = png::Decoder::new(std::io::Cursor::new(fs::read(path).unwrap()))
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut pixels).unwrap();
            let count = pixels[..info.buffer_size()]
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter(|(index, pixel)| {
                    index / (info.width as usize) >= (info.height - 30) as usize
                        && index % (info.width as usize) > 300
                        && pixel[0] > 100
                        && pixel[1] > 110
                        && pixel[2] > 120
                })
                .count();
            assert!(
                count > 100,
                "Status help text must be present at the bottom of {}",
                path.display()
            );
        }
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::copy(&hover, Path::new(&directory).join("status-hover.png")).unwrap();
            fs::copy(&focus, Path::new(&directory).join("status-focus.png")).unwrap();
        }
        adapter.shutdown().await;
        drop(gui);
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn complete_editing_workflow_and_visual_review_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let size = std::env::var("WAVEBOB_WORKFLOW_SIZE").unwrap_or("1440x900".into());
        let scale = std::env::var("WAVEBOB_WORKFLOW_SCALE").unwrap_or("1.0".into());
        let _gui = Gui::with_scale(
            runtime.path(),
            "complete-workflow",
            &runtime.path().join("preferences.json"),
            &size,
            &scale,
        );
        let client = named_client(runtime.path(), "complete-workflow").await;
        let id = response(&call(&client, "session.get", json!({})).await)["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        client.cancel().await.unwrap();
        let binary = std::env::var("WAVEBOB_MCP_TEST_BIN")
            .unwrap_or(env!("CARGO_BIN_EXE_wavebob-mcp").into());
        let mut adapter =
            workflow::Adapter::connect("mcp", Some(&binary), Some(&id), runtime.path().to_str())
                .await;
        let summary = workflow::run(&mut adapter, &runtime.path().join("workflow"), true).await;
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            for name in [
                "waveform.png",
                "waveform-hidden.png",
                "multitrack.png",
                "multitrack-hidden.png",
                "workflow.json",
                "Mix.wav",
                "Mix.mp3",
                "Sample selection.mp3",
            ] {
                fs::copy(
                    runtime.path().join("workflow").join(name),
                    Path::new(&directory).join(name),
                )
                .unwrap();
            }
        }
        assert_eq!(summary["project_reopen_without_sources"], true);
        adapter.shutdown().await;
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn portable_project_reopens_without_sources_and_captures_both_modes() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "portable-project-check");
        let client = named_client(runtime.path(), "portable-project-check").await;
        let source = runtime.path().join("音色 texture.mp3");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3"),
            &source,
        )
        .unwrap();
        let opened =
            response(&call(&client, "documents.open", json!({"params":{"path":source}})).await);
        let doc = finish_mcp_job(&client, opened).await;
        let edited = response(&call(&client, "edit.process", json!({"params":{"target":{"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]},"effect":{"kind":"gain","db":-3.0}}})).await);
        let doc = finish_mcp_job(&client, edited).await;
        let track = response(
            &call(
                &client,
                "tracks.add",
                json!({"params":{"name":"Portable texture"}}),
            )
            .await,
        )["result"]["tracks"][0]["id"]
            .clone();
        let length = doc["frames"].as_u64().unwrap() / 2;
        for position in [0, 2000] {
            assert_eq!(response(&call(&client, "clips.insert", json!({"params":{"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":length,"position":position}})).await)["status"], "ok");
        }
        let before = response(&call(&client, "session.get", json!({})).await)["result"].clone();
        let revision = &before["arrangement"]["revision_id"];
        let prepared = response(
            &call(
                &client,
                "mix.prepare",
                json!({"params":{"revision_id":revision}}),
            )
            .await,
        );
        finish_mcp_output(&client, prepared).await;
        let samples = response(
            &call(
                &client,
                "mix.samples",
                json!({"params":{"revision_id":revision,"start_frame":2000,"frames":128}}),
            )
            .await,
        )["result"]
            .clone();
        let project_path = runtime.path().join("Portable session.wavebob");
        let started = response(
            &call(
                &client,
                "project.save",
                json!({"params":{"path":project_path}}),
            )
            .await,
        );
        let saved = finish_mcp_output(&client, started).await;
        fs::remove_file(source).unwrap();
        response(
            &call(
                &client,
                "tracks.add",
                json!({"params":{"name":"Discarded change"}}),
            )
            .await,
        );
        let started = response(
            &call(
                &client,
                "project.open",
                json!({"params":{"path":project_path,"discard_changes":true}}),
            )
            .await,
        );
        let reopened = finish_mcp_output(&client, started).await;
        let after = response(&call(&client, "session.get", json!({})).await);
        assert_eq!(after["result"]["documents"], before["documents"]);
        assert_eq!(
            after["result"]["arrangement"]["tracks"],
            before["arrangement"]["tracks"]
        );
        assert_eq!(after["result"]["project"]["dirty"], false);
        let prepared = response(
            &call(
                &client,
                "mix.prepare",
                json!({"params":{"revision_id":revision}}),
            )
            .await,
        );
        finish_mcp_output(&client, prepared).await;
        assert_eq!(
            response(
                &call(
                    &client,
                    "mix.samples",
                    json!({"params":{"revision_id":revision,"start_frame":2000,"frames":128}})
                )
                .await
            )["result"],
            samples
        );
        let mut captures = Vec::new();
        response(
            &call(
                &client,
                "documents.selection.set",
                json!({"params":{"document_id":doc["id"],"start_frame":819,"end_frame":4096}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "presentation.timeline.set",
                json!({"params":{"start_frame":0,"end_frame":8192}}),
            )
            .await,
        );
        for mode in ["waveform", "multitrack"] {
            response(
                &call(
                    &client,
                    "presentation.set_mode",
                    json!({"params":{"mode":mode}}),
                )
                .await,
            );
            let captured = call(
                &client,
                "capture.png",
                json!({"params":{"after_revision":after["revision"]}}),
            )
            .await;
            let metadata = response(&captured);
            assert_eq!(metadata["status"], "ok");
            assert_eq!(metadata["result"]["metadata"]["mode"], mode);
            assert!(
                metadata["result"]["metadata"]["rendered_revision"]
                    .as_u64()
                    .unwrap()
                    >= after["revision"].as_u64().unwrap()
            );
            let image = captured
                .content
                .iter()
                .find_map(|block| {
                    if let ContentBlock::Image(image) = block {
                        Some(image)
                    } else {
                        None
                    }
                })
                .unwrap();
            if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
                fs::create_dir_all(&directory).unwrap();
                fs::write(
                    Path::new(&directory).join(format!("project-{mode}.png")),
                    BASE64_STANDARD.decode(&image.data).unwrap(),
                )
                .unwrap();
            }
            captures.push(metadata);
        }
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(Path::new(&directory).join("project-roundtrip.json"), serde_json::to_vec_pretty(&json!({"saved":saved,"reopened":reopened,"mixed_pcm_equal":true,"captures":captures})).unwrap()).unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn wav_and_mp3_mix_export_reimport_and_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "mix-output-check");
        let client = named_client(runtime.path(), "mix-output-check").await;
        let path = runtime.path().join("Ambience.wav");
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..24000 {
            writer
                .write_sample(
                    ((frame as f64 / 24000.0 * 220.0 * std::f64::consts::TAU).sin() * 5000.0)
                        as i16,
                )
                .unwrap();
        }
        writer.finalize().unwrap();
        let opened =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let wav = finish_mcp_job(&client, opened).await;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3");
        let opened =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let mp3 = finish_mcp_job(&client, opened).await;
        let arr = response(
            &call(
                &client,
                "tracks.add",
                json!({"params":{"name":"Texture mix"}}),
            )
            .await,
        );
        let track = arr["result"]["tracks"][0]["id"].clone();
        for (doc, position) in [(&wav, 0), (&mp3, 9600)] {
            assert_eq!(response(&call(&client,"clips.insert",json!({"params":{"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":position}})).await)["status"],"ok");
        }
        let arr = response(&call(&client, "arrangement.get", json!({})).await)["result"].clone();
        let clip = &arr["tracks"][0]["clips"][1];
        let edited=response(&call(&client,"clips.pcm.process",json!({"params":{"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":{"kind":"reverb","room_size":0.4,"decay_seconds":0.7,"wet":0.2}}})).await);
        finish_mcp_output(&client, edited).await;
        response(
            &call(
                &client,
                "tracks.update",
                json!({"params":{"track_id":track,"gain_db":-3.0,"pan":0.25}}),
            )
            .await,
        );
        let arr = response(&call(&client, "arrangement.get", json!({})).await)["result"].clone();
        let prepared = response(
            &call(
                &client,
                "mix.prepare",
                json!({"params":{"revision_id":arr["revision_id"]}}),
            )
            .await,
        );
        finish_mcp_output(&client, prepared).await;
        let raw_path = runtime.path().join("Rendered mix.wav");
        let mp3_path = runtime.path().join("Rendered mix.mp3");
        let mut summaries = Vec::new();
        for (path, format, rate) in [
            (&raw_path, json!({"kind":"wav","encoding":"float32"}), 48000),
            (&mp3_path, json!({"kind":"mp3","bitrate_kbps":192}), 44100),
        ] {
            let started=response(&call(&client,"mix.export",json!({"params":{"revision_id":arr["revision_id"],"start_frame":4800,"end_frame":33600,"path":path,"format":format,"sample_rate":rate}})).await);
            summaries.push(finish_mcp_output(&client, started).await);
        }
        let oracle=response(&call(&client,"mix.samples",json!({"params":{"revision_id":arr["revision_id"],"start_frame":4800,"frames":128}})).await)["result"]["channels"].clone();
        let decoded = hound::WavReader::open(raw_path)
            .unwrap()
            .samples::<f32>()
            .take(256)
            .map(|value| value.unwrap() as f64)
            .collect::<Vec<_>>();
        for frame in 0..128 {
            for channel in 0..2 {
                assert_eq!(
                    decoded[frame * 2 + channel],
                    oracle[channel][frame].as_f64().unwrap()
                );
            }
        }
        let opened = response(
            &call(
                &client,
                "documents.open",
                json!({"params":{"path":mp3_path}}),
            )
            .await,
        );
        let reimported = finish_mcp_job(&client, opened).await;
        assert_eq!(reimported["frames"], 26460);
        assert_eq!(reimported["sample_rate"], 44100);
        assert_eq!(reimported["channels"], 2);
        assert_eq!(
            response(&call(&client, "arrangement.get", json!({})).await)["result"]["dirty"],
            true
        );
        response(
            &call(
                &client,
                "presentation.set_mode",
                json!({"params":{"mode":"waveform"}}),
            )
            .await,
        );
        response(&call(&client,"documents.selection.set",json!({"params":{"document_id":reimported["id"],"start_frame":2646,"end_frame":13230}})).await);
        let captured = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("mix-reimport.png")}}),
        )
        .await;
        let metadata = response(&captured);
        assert_eq!(metadata["status"], "ok");
        let image = captured
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("mix-reimport.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("mix-reimport.json"),
                serde_json::to_vec_pretty(
                    &json!({"exports":summaries,"reimported":reimported,"capture":metadata}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn automatic_crossfade_audio_and_overlay_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "crossfade-check");
        let client = named_client(runtime.path(), "crossfade-check").await;
        let mut documents = Vec::new();
        for (index, name) in ["Rain ambience.wav", "Footsteps.wav", "Piano texture.wav"]
            .iter()
            .enumerate()
        {
            let frames = [288000, 192000, 144000][index];
            let path = runtime.path().join(name);
            let mut writer = hound::WavWriter::create(
                &path,
                hound::WavSpec {
                    channels: 2,
                    sample_rate: 48000,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for frame in 0..frames {
                let t = frame as f64 / 48000.0;
                let envelope = if index == 1 {
                    (t * 5.1).sin().abs().powi(8)
                } else {
                    0.4 + 0.3 * (t * 1.7).sin().abs()
                };
                for channel in 0..2 {
                    let value = (t
                        * (170.0 + index as f64 * 91.0 + channel as f64 * 23.0)
                        * std::f64::consts::TAU)
                        .sin()
                        * envelope
                        * 0.3;
                    writer.write_sample((value * 8388607.0) as i32).unwrap();
                }
            }
            writer.finalize().unwrap();
            let opened =
                response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
            documents.push(finish_mcp_job(&client, opened).await);
        }
        for (index, name) in ["Ambience", "Footsteps", "Texture", "Track 4"]
            .iter()
            .enumerate()
        {
            let arr = response(&call(&client, "tracks.add", json!({"params":{"name":name}})).await);
            let track = &arr["result"]["tracks"][index]["id"];
            if index < 3 {
                let doc = &documents[index];
                let positions = if index == 1 {
                    vec![48000, 144000]
                } else {
                    vec![if index == 0 { 0 } else { 192000 }]
                };
                for position in positions {
                    assert_eq!(response(&call(&client,"clips.insert",json!({"params":{"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":position}})).await)["status"],"ok");
                }
            }
            if index == 1 {
                response(
                    &call(
                        &client,
                        "tracks.update",
                        json!({"params":{"track_id":track,"solo":true,"gain_db":-6.0}}),
                    )
                    .await,
                );
            }
        }
        let arr = response(&call(&client, "arrangement.get", json!({})).await)["result"].clone();
        let foot = &arr["tracks"][1];
        let a = &foot["clips"][0];
        let b = &foot["clips"][1];
        let regions = response(&call(&client, "arrangement.crossfades", json!({})).await);
        assert_eq!(regions["result"]["regions"][0]["start"], 144000);
        assert_eq!(regions["result"]["regions"][0]["end"], 240000);
        let rejected=response(&call(&client,"clips.insert",json!({"params":{"track_id":foot["id"],"document_id":documents[1]["id"],"revision_id":documents[1]["revision_id"],"source_start":0,"source_end":documents[1]["frames"],"position":96000}})).await);
        assert_eq!(rejected["error"]["code"], "invalid_params");
        assert_eq!(
            response(&call(&client, "arrangement.get", json!({})).await)["result"],
            arr
        );
        let prepared = response(
            &call(
                &client,
                "mix.prepare",
                json!({"params":{"revision_id":arr["revision_id"]}}),
            )
            .await,
        );
        finish_mcp_output(&client, prepared).await;
        let frame = 156345;
        let a_pcm = response(
            &call(
                &client,
                "clips.samples",
                json!({"params":{"clip_id":a["id"],"start_frame":frame-48000,"frames":1}}),
            )
            .await,
        );
        let b_pcm = response(
            &call(
                &client,
                "clips.samples",
                json!({"params":{"clip_id":b["id"],"start_frame":frame-144000,"frames":1}}),
            )
            .await,
        );
        let mixed = response(
            &call(
                &client,
                "mix.samples",
                json!({"params":{"revision_id":arr["revision_id"],"start_frame":frame,"frames":1}}),
            )
            .await,
        );
        let t = (frame - 144000) as f64 / 95999.0;
        for channel in 0..2 {
            let expected = (a_pcm["result"]["channels"][channel][0].as_f64().unwrap()
                * (t * std::f64::consts::FRAC_PI_2).cos()
                + b_pcm["result"]["channels"][channel][0].as_f64().unwrap()
                    * (t * std::f64::consts::FRAC_PI_2).sin())
                * 10.0f64.powf(-6.0 / 20.0);
            assert!(
                (mixed["result"]["channels"][channel][0].as_f64().unwrap() - expected).abs() < 1e-6
            );
        }
        response(
            &call(
                &client,
                "presentation.set_mode",
                json!({"params":{"mode":"multitrack"}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "presentation.timeline.set",
                json!({"params":{"start_frame":0,"end_frame":384000}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "arrangement.navigate",
                json!({"params":{"cursor":192000,"selected_clip_ids":[b["id"]]}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "presentation.effects.set",
                json!({"params":{"effect":{"kind":"gain","db":-3.0}}}),
            )
            .await,
        );
        let captured = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("crossfade.png")}}),
        )
        .await;
        let metadata = response(&captured);
        assert_eq!(metadata["status"], "ok");
        let image = captured
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("crossfade.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(Path::new(&directory).join("crossfade.json"),serde_json::to_vec_pretty(&json!({"crossfades":regions,"mixed":mixed,"capture":metadata,"third_overlap":rejected})).unwrap()).unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn native_multitrack_playback_mix_inspection_and_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "mix-check");
        let client = named_client(runtime.path(), "mix-check").await;
        let path = runtime.path().join("Quiet ambience.wav");
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..120000 {
            let t = frame as f64 / 24000.0;
            writer
                .write_sample(((t * 220.0 * std::f64::consts::TAU).sin() * 1000.0) as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
        let opened =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let mono = finish_mcp_job(&client, opened).await;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3");
        let opened =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let mp3 = finish_mcp_job(&client, opened).await;
        for (index, doc) in [&mono, &mp3].iter().enumerate() {
            let arr = response(
                &call(
                    &client,
                    "tracks.add",
                    json!({"params":{"name":if index==0{"Ambience"}else{"Texture"}}}),
                )
                .await,
            );
            let track = &arr["result"]["tracks"][index]["id"];
            assert_eq!(response(&call(&client,"clips.insert",json!({"params":{"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":index as u64*96000}})).await)["status"],"ok");
            response(&call(&client,"tracks.update",json!({"params":{"track_id":track,"gain_db":-12.0,"pan":if index==0{-0.3}else{0.4}}})).await);
        }
        let arr = response(&call(&client, "arrangement.get", json!({})).await)["result"].clone();
        let prepared = response(
            &call(
                &client,
                "mix.prepare",
                json!({"params":{"revision_id":arr["revision_id"]}}),
            )
            .await,
        );
        finish_mcp_output(&client, prepared).await;
        let offline=response(&call(&client,"mix.samples",json!({"params":{"revision_id":arr["revision_id"],"start_frame":48000,"frames":512}})).await);
        assert!(offline["result"]["peak_level"].as_f64().unwrap() > 0.001);
        response(
            &call(
                &client,
                "arrangement.navigate",
                json!({"params":{"cursor":48000,"selection":{"start":48000,"end":192000}}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "transport.loop.set",
                json!({"params":{"enabled":true}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "presentation.set_mode",
                json!({"params":{"mode":"multitrack"}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "presentation.timeline.set",
                json!({"params":{"start_frame":0,"end_frame":288000}}),
            )
            .await,
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let state = response(&call(&client, "transport.get", json!({})).await);
            if state["result"]["output"]["status"] == "ready" {
                break;
            }
            assert_ne!(state["result"]["output"]["status"], "failed", "{state}");
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            response(&call(&client, "mix.play", json!({})).await)["status"],
            "ok"
        );
        let playing = loop {
            let state =
                response(&call(&client, "transport.get", json!({})).await)["result"].clone();
            if state["applied"]["playing"] == true && state["peak_level"].as_f64().unwrap() > 0.001
            {
                break state;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(playing["applied"]["document_id"], arr["id"]);
        assert!((48000..192000).contains(&playing["applied"]["frame"].as_u64().unwrap()));
        let captured = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("mix.png")}}),
        )
        .await;
        let metadata = response(&captured);
        assert_eq!(metadata["status"], "ok");
        let image = captured
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("mix.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("mix.json"),
                serde_json::to_vec_pretty(
                    &json!({"playing":playing,"offline":offline,"capture":metadata}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        response(&call(&client, "transport.pause", json!({})).await);
        loop {
            let state =
                response(&call(&client, "transport.get", json!({})).await)["result"].clone();
            if state["applied"]["playing"] == false && state["pending"] == false {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn independent_clip_processing_trim_and_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "clip-effects-check");
        let client = named_client(runtime.path(), "clip-effects-check").await;
        let path = runtime.path().join("Piano texture.wav");
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48_000,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..96_000 {
            let t = frame as f64 / 48_000.0;
            for channel in 0..2 {
                let value = (t * (220.0 + channel as f64 * 55.0) * std::f64::consts::TAU).sin()
                    * (0.1 + 0.15 * (t * 7.3).sin().abs());
                writer.write_sample((value * 8388607.0) as i32).unwrap();
            }
        }
        writer.finalize().unwrap();
        let started =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let doc = finish_mcp_job(&client, started).await;
        let arr =
            response(&call(&client, "tracks.add", json!({"params":{"name":"Texture"}})).await);
        let track = &arr["result"]["tracks"][0]["id"];
        for position in [0, 144_000] {
            let inserted=response(&call(&client,"clips.insert",json!({"params":{"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":position}})).await);
            assert_eq!(inserted["status"], "ok");
        }
        let arr = response(&call(&client, "arrangement.get", json!({})).await);
        let first = arr["result"]["tracks"][0]["clips"][0].clone();
        let second = arr["result"]["tracks"][0]["clips"][1].clone();
        let original = response(
            &call(
                &client,
                "clips.samples",
                json!({"params":{"clip_id":second["id"],"frames":256}}),
            )
            .await,
        )["result"]["channels"]
            .clone();
        let effect=response(&call(&client,"clips.pcm.process",json!({"params":{"clip_id":first["id"],"edit_revision_id":first["edit_revision_id"],"effect":{"kind":"gain","db":-6.0}}})).await);
        let edited = finish_mcp_output(&client, effect).await;
        let trim=response(&call(&client,"clips.trim",json!({"params":{"clip_id":first["id"],"edit_revision_id":edited["edit_revision_id"],"source_start":24000,"source_end":72000}})).await);
        let trimmed = finish_mcp_output(&client, trim).await;
        assert_eq!(trimmed["frames"], 48000);
        assert_eq!(trimmed["revision_id"], doc["revision_id"]);
        assert_eq!(
            response(
                &call(
                    &client,
                    "clips.samples",
                    json!({"params":{"clip_id":second["id"],"frames":256}})
                )
                .await
            )["result"]["channels"],
            original
        );
        assert_eq!(
            response(
                &call(
                    &client,
                    "documents.samples",
                    json!({"params":{"document_id":doc["id"],"frames":256}})
                )
                .await
            )["result"]["channels"],
            original
        );
        response(
            &call(
                &client,
                "presentation.set_mode",
                json!({"params":{"mode":"multitrack"}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "arrangement.navigate",
                json!({"params":{"cursor":48000,"selected_clip_ids":[first["id"]]}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "presentation.timeline.set",
                json!({"params":{"start_frame":0,"end_frame":288000}}),
            )
            .await,
        );
        let captured = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("clip-effects.png")}}),
        )
        .await;
        let metadata = response(&captured);
        assert_eq!(metadata["status"], "ok");
        let image = captured
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("clip-effects.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("clip-effects.json"),
                serde_json::to_vec_pretty(
                    &json!({"clip":trimmed,"other_clip":second,"capture":metadata}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn editable_arrangement_and_pinned_waveforms_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "arrangement-check");
        let client = named_client(runtime.path(), "arrangement-check").await;
        let mut documents = Vec::new();
        for (index, name) in ["Rain ambience.wav", "Footsteps.wav", "Piano texture.wav"]
            .iter()
            .enumerate()
        {
            let path = runtime.path().join(name);
            let rate = if index == 1 { 44100 } else { 48000 };
            let mut writer = hound::WavWriter::create(
                &path,
                hound::WavSpec {
                    channels: 2,
                    sample_rate: rate,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for frame in 0..rate * 6 {
                let t = frame as f64 / rate as f64;
                for channel in 0..2 {
                    let envelope = (0.2 + 0.1 * (t * 2.3).sin())
                        * (if index == 1 {
                            (t * 4.0).sin().abs().powi(8)
                        } else {
                            1.0
                        });
                    let value = (t
                        * (150.0 + index as f64 * 91.0 + channel as f64 * 20.0)
                        * std::f64::consts::TAU)
                        .sin()
                        * envelope;
                    writer.write_sample((value * 8388607.0) as i32).unwrap();
                }
            }
            writer.finalize().unwrap();
            let started =
                response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
            documents.push(finish_mcp_job(&client, started).await);
        }
        for (index, name) in ["Ambience", "Footsteps", "Texture", "Track 4"]
            .iter()
            .enumerate()
        {
            let arr = response(&call(&client, "tracks.add", json!({"params":{"name":name}})).await);
            assert_eq!(arr["status"], "ok");
            if index < 3 {
                let doc = &documents[index];
                let track = &arr["result"]["tracks"][index]["id"];
                let result=response(&call(&client,"clips.insert",json!({"params":{"track_id":track,"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":0,"source_end":doc["frames"],"position":index as u64*48_000*2}})).await);
                assert_eq!(result["status"], "ok", "{result}");
            }
        }
        let arrangement = response(&call(&client, "arrangement.get", json!({})).await);
        let first = &arrangement["result"]["tracks"][0]["clips"][0];
        let before = response(
            &call(
                &client,
                "clips.waveform",
                json!({"params":{"clip_id":first["id"],"bins":32}}),
            )
            .await,
        )["result"]
            .clone();
        response(
            &call(
                &client,
                "documents.close",
                json!({"params":{"document_id":documents[0]["id"]}}),
            )
            .await,
        );
        assert_eq!(
            response(
                &call(
                    &client,
                    "clips.waveform",
                    json!({"params":{"clip_id":first["id"],"bins":32}})
                )
                .await
            )["result"],
            before
        );
        response(
            &call(
                &client,
                "presentation.set_mode",
                json!({"params":{"mode":"multitrack"}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "presentation.timeline.set",
                json!({"params":{"start_frame":0,"end_frame":576_000}}),
            )
            .await,
        );
        response(&call(&client,"arrangement.navigate",json!({"params":{"cursor":192_000,"selection":{"start":144_000,"end":240_000},"selected_clip_ids":[arrangement["result"]["tracks"][1]["clips"][0]["id"]]}})).await);
        let captured = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("arrangement.png")}}),
        )
        .await;
        let metadata = response(&captured);
        assert_eq!(metadata["status"], "ok", "{metadata}");
        let image = captured
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("arrangement.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("arrangement.json"),
                serde_json::to_vec_pretty(&json!({"arrangement":arrangement,"capture":metadata}))
                    .unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI and default audio device"]
    async fn native_playback_position_pause_loop_and_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "native-playback");
        let client = named_client(runtime.path(), "native-playback").await;
        let path = runtime.path().join("Quiet tone.wav");
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..240000 {
            writer
                .write_sample(
                    ((frame as f64 * 440.0 * std::f64::consts::TAU / 48000.0).sin() * 1000.0)
                        as i16,
                )
                .unwrap();
        }
        writer.finalize().unwrap();
        let job = response(&call(&client, "documents.open", json!({"params":{"path":path}})).await)
            ["result"]["id"]
            .clone();
        let deadline = Instant::now() + Duration::from_secs(20);
        let id = loop {
            let job = response(&call(&client, "jobs.get", json!({"params":{"job_id":job}})).await)
                ["result"]
                .clone();
            if job["status"] == "succeeded" {
                break job["result"]["id"].clone();
            }
            assert_ne!(job["status"], "failed", "{job}");
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        loop {
            let state =
                response(&call(&client, "transport.get", json!({})).await)["result"].clone();
            assert_ne!(state["output"]["status"], "failed", "{state}");
            if state["output"]["status"] == "ready" {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        response(
            &call(
                &client,
                "transport.play",
                json!({"params":{"document_id":id}}),
            )
            .await,
        );
        let playing = loop {
            let state =
                response(&call(&client, "transport.get", json!({})).await)["result"].clone();
            if state["applied"]["playing"] == true
                && state["applied"]["frame"].as_u64().unwrap() > 0
                && state["peak_level"].as_f64().unwrap() > 0.0
            {
                break state;
            }
            assert!(Instant::now() < deadline, "{state}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        response(&call(&client, "transport.pause", json!({})).await);
        let paused = loop {
            let state =
                response(&call(&client, "transport.get", json!({})).await)["result"].clone();
            if state["pending"] == false && state["applied"]["playing"] == false {
                break state;
            }
            assert!(Instant::now() < deadline);
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            response(&call(&client, "transport.get", json!({})).await)["result"]["applied"]["frame"],
            paused["applied"]["frame"]
        );
        response(
            &call(
                &client,
                "documents.selection.set",
                json!({"params":{"document_id":id,"start_frame":48000,"end_frame":96000}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "transport.loop.set",
                json!({"params":{"enabled":true}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "transport.rewind",
                json!({"params":{"document_id":id}}),
            )
            .await,
        );
        response(
            &call(
                &client,
                "transport.play",
                json!({"params":{"document_id":id}}),
            )
            .await,
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
        let looped = response(&call(&client, "transport.get", json!({})).await)["result"].clone();
        assert!(
            (48000..96000).contains(&looped["applied"]["frame"].as_u64().unwrap()),
            "{looped}"
        );
        response(&call(&client, "transport.pause", json!({})).await);
        while response(&call(&client, "transport.get", json!({})).await)["result"]["pending"]
            == true
        {
            assert!(Instant::now() < deadline);
        }
        let capture = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("playback.png")}}),
        )
        .await;
        let metadata = response(&capture);
        assert_eq!(metadata["status"], "ok");
        let image = capture
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("native-playback.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("native-playback.json"),
                serde_json::to_vec_pretty(
                    &json!({"playing":playing,"paused":paused,"looped":looped,"capture":metadata}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI and default output device"]
    async fn browser_mp3_audition_open_and_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "browser-check");
        let client = named_client(runtime.path(), "browser-check").await;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/cbr-1ch-44100-tagged.mp3");
        response(
            &call(
                &client,
                "browser.navigate",
                json!({"params":{"path":path.parent().unwrap()}}),
            )
            .await,
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let state = loop {
            let state = response(&call(&client, "browser.get", json!({})).await)["result"].clone();
            if state["loading"] == false {
                break state;
            }
            assert!(Instant::now() < deadline);
        };
        let selected = state["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == "cbr-1ch-44100-tagged.mp3")
            .unwrap()["path"]
            .clone();
        response(
            &call(
                &client,
                "browser.select",
                json!({"params":{"path":selected}}),
            )
            .await,
        );
        loop {
            let state =
                response(&call(&client, "transport.get", json!({})).await)["result"].clone();
            if state["output"]["status"] == "ready" {
                break;
            }
            assert_ne!(state["output"]["status"], "failed", "{state}");
            assert!(Instant::now() < deadline);
        }
        let start = response(
            &call(
                &client,
                "audition.start",
                json!({"params":{"path":selected}}),
            )
            .await,
        );
        assert_eq!(start["status"], "ok");
        let preview = loop {
            let state = response(&call(&client, "browser.get", json!({})).await)["result"].clone();
            if state["audition"]["loading"] == false {
                break state;
            }
            assert!(Instant::now() < deadline);
        };
        assert!(preview["audition"]["error"].is_null(), "{preview}");
        assert_eq!(preview["audition"]["frames"], 8192);
        assert!(
            response(&call(&client, "session.get", json!({})).await)["result"]["documents"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let job = response(
            &call(&client, "browser.open", json!({"params":{"path":selected}})).await,
        )["result"]["id"]
            .clone();
        loop {
            let state = response(
                &call(&client, "jobs.get", json!({"params":{"job_id":job}})).await,
            )["result"]
                .clone();
            if state["status"] == "succeeded" {
                break;
            }
            assert_ne!(state["status"], "failed", "{state}");
            assert!(Instant::now() < deadline);
        }
        assert!(
            response(&call(&client, "browser.get", json!({})).await)["result"]["audition"]["id"]
                .is_null()
        );
        let captured = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("browser.png")}}),
        )
        .await;
        let metadata = response(&captured);
        assert_eq!(metadata["status"], "ok");
        let image = captured
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("browser.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("browser.json"),
                serde_json::to_vec_pretty(&json!({"preview":preview,"capture":metadata})).unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn undoable_effects_and_waveform_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "effects-check");
        let client = named_client(runtime.path(), "effects-check").await;
        let path = runtime.path().join("Texture.wav");
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..144000 {
            for frequency in [220.0, 330.0] {
                let t = frame as f64 / 48000.0;
                let amplitude = 0.25 + 0.2 * (t * 3.0).sin().abs();
                writer
                    .write_sample(
                        ((t * frequency * std::f64::consts::TAU).sin() * amplitude * 32767.0)
                            as i16,
                    )
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
        let opened =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let doc = finish_mcp_job(&client, opened).await;
        response(
            &call(
                &client,
                "documents.selection.set",
                json!({"params":{"document_id":doc["id"],"start_frame":24000,"end_frame":120000}}),
            )
            .await,
        );
        let started=response(&call(&client,"edit.process",json!({"params":{"target":{"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":24000,"end_frame":120000},"effect":{"kind":"fade_out"}}})).await);
        let edited = finish_mcp_job(&client, started).await;
        assert_eq!(edited["dirty"], true);
        response(&call(&client, "history.undo", json!({})).await);
        let undone = response(
            &call(
                &client,
                "documents.get",
                json!({"params":{"document_id":doc["id"]}}),
            )
            .await,
        );
        assert_eq!(undone["result"]["revision_id"], doc["revision_id"]);
        response(&call(&client, "history.redo", json!({})).await);
        let redone = response(
            &call(
                &client,
                "documents.get",
                json!({"params":{"document_id":doc["id"]}}),
            )
            .await,
        );
        assert_eq!(redone["result"]["revision_id"], edited["revision_id"]);
        let history = response(&call(&client, "history.get", json!({})).await);
        assert_eq!(history["result"]["undo_count"], 1);
        let captured=call(&client,"capture.png",json!({"params":{"path":runtime.path().join("effects.png"),"after_revision":redone["revision"]}})).await;
        let metadata = response(&captured);
        assert_eq!(metadata["status"], "ok");
        let image = captured
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("effects.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("effects.json"),
                serde_json::to_vec_pretty(
                    &json!({"document":edited,"history":history,"capture":metadata}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn structural_edits_empty_view_and_cross_rate_paste_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let samples = runtime.path().join("Samples");
        fs::create_dir(&samples).unwrap();
        for (name, rate, channels, frames) in [
            ("Source.wav", 48000, 2, 96000),
            ("Destination.wav", 44100, 1, 4410),
        ] {
            let mut writer = hound::WavWriter::create(
                samples.join(name),
                hound::WavSpec {
                    channels,
                    sample_rate: rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for frame in 0..frames {
                for channel in 0..channels {
                    let t = frame as f64 / rate as f64;
                    let value = if name == "Source.wav" {
                        (t * (220.0 + 110.0 * channel as f64) * std::f64::consts::TAU).sin()
                            * (0.4 + 0.2 * (t * 5.0).sin())
                    } else {
                        0.0
                    };
                    writer.write_sample((value * 32767.0) as i16).unwrap();
                }
            }
            writer.finalize().unwrap();
        }
        let _gui = Gui::start(runtime.path(), "structural-check");
        let client = named_client(runtime.path(), "structural-check").await;
        let started = response(
            &call(
                &client,
                "documents.open",
                json!({"params":{"path":samples.join("Source.wav")}}),
            )
            .await,
        );
        let source = finish_mcp_job(&client, started).await;
        response(&call(&client,"edit.copy",json!({"params":{"target":{"scope":"range","document_id":source["id"],"revision_id":source["revision_id"],"start_frame":24000,"end_frame":72000}}})).await);
        let started = response(
            &call(
                &client,
                "documents.open",
                json!({"params":{"path":samples.join("Destination.wav")}}),
            )
            .await,
        );
        let destination = finish_mcp_job(&client, started).await;
        let started=response(&call(&client,"edit.delete",json!({"params":{"target":{"scope":"whole","document_id":destination["id"],"revision_id":destination["revision_id"]}}})).await);
        let empty = finish_mcp_job(&client, started).await;
        assert_eq!(empty["frames"], 0);
        response(
            &call(
                &client,
                "browser.navigate",
                json!({"params":{"path":samples}}),
            )
            .await,
        );
        let mut records = Vec::new();
        for label in ["empty-sample", "cross-rate-paste"] {
            if label == "cross-rate-paste" {
                let started=response(&call(&client,"edit.paste",json!({"params":{"document_id":destination["id"],"revision_id":empty["revision_id"],"start_frame":0,"end_frame":0}})).await);
                let pasted = finish_mcp_job(&client, started).await;
                assert_eq!(pasted["frames"], 44100);
                assert_eq!(pasted["channels"], 1);
                records.push(json!({"pasted":pasted}));
            }
            let captured = call(
                &client,
                "capture.png",
                json!({"params":{"path":runtime.path().join(format!("{label}.png"))}}),
            )
            .await;
            let metadata = response(&captured);
            assert_eq!(metadata["status"], "ok");
            let image = captured
                .content
                .iter()
                .find_map(|block| match block {
                    ContentBlock::Image(image) => Some(image),
                    _ => None,
                })
                .unwrap();
            if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
                fs::create_dir_all(&directory).unwrap();
                fs::write(
                    Path::new(&directory).join(format!("{label}.png")),
                    BASE64_STANDARD.decode(&image.data).unwrap(),
                )
                .unwrap();
            }
            records.push(json!({"file":format!("{label}.png"),"capture":metadata}));
        }
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&directory).join("structural.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn reverb_parameters_apply_and_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "reverb-check");
        let client = named_client(runtime.path(), "reverb-check").await;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/cbr-2ch-48000-tagged.mp3");
        let started =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let doc = finish_mcp_job(&client, started).await;
        let effect = json!({"kind":"reverb","room_size":0.5,"decay_seconds":1.5,"wet":0.35});
        let view = response(
            &call(
                &client,
                "presentation.effects.set",
                json!({"params":{"effect":effect}}),
            )
            .await,
        );
        assert_eq!(view["status"], "ok");
        let started=response(&call(&client,"edit.process",json!({"params":{"target":{"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]},"effect":effect}})).await);
        let edited = finish_mcp_job(&client, started).await;
        assert_ne!(edited["revision_id"], doc["revision_id"]);
        assert_eq!(edited["frames"], doc["frames"]);
        let capture=call(&client,"capture.png",json!({"params":{"path":runtime.path().join("reverb.png"),"after_presentation_revision":view["result"]["revision"]}})).await;
        let metadata = response(&capture);
        assert_eq!(metadata["status"], "ok");
        let image = capture
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("reverb.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(
                Path::new(&directory).join("reverb.json"),
                serde_json::to_vec_pretty(
                    &json!({"document":edited,"effect_view":view,"capture":metadata}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native GUI renderer"]
    async fn mp3_export_save_reimport_capture_and_guarded_quit_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let mut gui = Gui::start(runtime.path(), "save-check");
        let client = named_client(runtime.path(), "save-check").await;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3");
        let started =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let doc = finish_mcp_job(&client, started).await;
        let effect = json!({"kind":"gain","db":-3.0});
        response(
            &call(
                &client,
                "presentation.effects.set",
                json!({"params":{"effect":effect}}),
            )
            .await,
        );
        let started=response(&call(&client,"edit.process",json!({"params":{"target":{"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]},"effect":effect}})).await);
        let edited = finish_mcp_job(&client, started).await;
        let refusal = response(
            &call(
                &client,
                "presentation.quit",
                json!({"params":{"discard_changes":false}}),
            )
            .await,
        );
        assert_eq!(refusal["error"]["code"], "dirty_document");
        let output = runtime.path().join("Edited texture.mp3");
        let started=response(&call(&client,"documents.save",json!({"params":{"document_id":doc["id"],"revision_id":edited["revision_id"],"path":output,"format":{"kind":"mp3","bitrate_kbps":192},"sample_rate":48000}})).await);
        let summary = finish_mcp_output(&client, started).await;
        assert_eq!(summary["sample_rate"], 48000);
        assert_eq!(summary["lossy"], true);
        let saved = response(
            &call(
                &client,
                "documents.get",
                json!({"params":{"document_id":doc["id"]}}),
            )
            .await,
        );
        assert_eq!(saved["result"]["dirty"], false);
        assert_eq!(saved["result"]["revision_id"], edited["revision_id"]);
        let started =
            response(&call(&client, "documents.open", json!({"params":{"path":output}})).await);
        let reimported = finish_mcp_job(&client, started).await;
        assert_eq!(reimported["frames"], summary["frames"]);
        assert_eq!(reimported["sample_rate"], 48000);
        let capture = call(
            &client,
            "capture.png",
            json!({"params":{"path":runtime.path().join("saved-mp3.png")}}),
        )
        .await;
        let metadata = response(&capture);
        assert_eq!(metadata["status"], "ok");
        let image = capture
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("saved-mp3.png"),
                BASE64_STANDARD.decode(&image.data).unwrap(),
            )
            .unwrap();
            fs::write(Path::new(&directory).join("native-save.json"),serde_json::to_vec_pretty(&json!({"export":summary,"saved_document":saved,"reimported":reimported,"capture":metadata})).unwrap()).unwrap();
        }
        let quit = response(
            &call(
                &client,
                "presentation.quit",
                json!({"params":{"discard_changes":false}}),
            )
            .await,
        );
        assert_eq!(quit["status"], "ok");
        assert_eq!(quit["result"]["close_requested"], true);
        let deadline = Instant::now() + Duration::from_secs(10);
        while gui.0.try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "GUI did not close after delivering the quit response"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        client.cancel().await.unwrap();
    }

    async fn saved_layout(client: &RunningService<RoleClient, ClientConfig>) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let view =
                response(&call(client, "presentation.get", json!({})).await)["result"].clone();
            match view["preferences"]["status"].as_str() {
                Some("saved") => return view,
                Some("error") => panic!("Preference worker failed: {view}"),
                _ => {}
            }
            assert!(
                Instant::now() < deadline,
                "Preferences did not finish saving"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn layout_capture(
        client: &RunningService<RoleClient, ClientConfig>,
        runtime: &Path,
        label: &str,
        size: [u32; 2],
        records: &mut Vec<Value>,
    ) {
        let view = saved_layout(client).await;
        let path = runtime.join(format!("{label}.png"));
        let result = call(
            client,
            "capture.png",
            json!({"params":{"path":path,"after_presentation_revision":view["revision"]}}),
        )
        .await;
        let capture = response(&result);
        assert_eq!(capture["status"], "ok");
        assert_eq!(
            capture["revision"], 0,
            "Layout operations must not change core history"
        );
        let metadata = &capture["result"]["metadata"];
        assert_eq!(metadata["mode"], view["mode"]);
        assert_eq!(metadata["mode"], label.split('-').next().unwrap());
        assert_eq!(metadata["presentation_revision"], view["revision"]);
        let image = result
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        let png = BASE64_STANDARD.decode(&image.data).unwrap();
        assert_eq!(png.len(), fs::metadata(&path).unwrap().len() as usize);
        assert_eq!(png, fs::read(&path).unwrap());
        let mut reader = png::Decoder::new(std::io::Cursor::new(&png))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        let scale = metadata["scale"].as_f64().unwrap();
        assert!((info.width as f64 / scale - size[0] as f64).abs() < 2.0);
        assert!((info.height as f64 / scale - size[1] as f64).abs() < 2.0);
        // Status text must still be painted at the bottom with either sidebar state.
        let status_start =
            ((info.height as f64 - 24.0 * scale).max(0.0) as usize) * info.width as usize * 4;
        assert!(
            pixels[status_start..info.buffer_size()]
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[0] > 100 && pixel[1] > 110 && pixel[2] > 120)
                .count()
                > 50,
            "Missing status-bar text"
        );
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(Path::new(&directory).join(format!("{label}.png")), &png).unwrap();
        }
        records.push(json!({"file":format!("{label}.png"),"metadata":metadata,"layout":view["layout"],"delivered_bytes":png.len()}));
    }

    #[tokio::test]
    #[ignore = "requires real native GUI renderers; run explicitly on a desktop"]
    async fn layout_controls_persist_and_both_modes_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let preferences = runtime.path().join("persistent-layout.json");
        let mut gui = Gui::with_profile(runtime.path(), "layout-wide", &preferences, "1440x900");
        let client = named_client(runtime.path(), "layout-wide").await;
        let configured=response(&call(&client,"presentation.layout.set",json!({"params":{"left_width":320,"right_width":280,"library_fraction":0.42,"expected_presentation_revision":0}})).await);
        assert_eq!(configured["result"]["revision"], 1);
        let stale = response(
            &call(
                &client,
                "presentation.layout.set",
                json!({"params":{"left_width":500,"expected_presentation_revision":0}}),
            )
            .await,
        );
        assert_eq!(stale["error"]["code"], "stale_revision");
        let invalid = response(
            &call(
                &client,
                "presentation.layout.set",
                json!({"params":{"left_width":100,"right_visible":false}}),
            )
            .await,
        );
        assert_eq!(invalid["error"]["code"], "invalid_params");
        let expected = saved_layout(&client).await["layout"].clone();
        let mut records = Vec::new();
        for mode in ["waveform", "multitrack"] {
            call(
                &client,
                "presentation.set_mode",
                json!({"params":{"mode":mode}}),
            )
            .await;
            layout_capture(
                &client,
                runtime.path(),
                &format!("{mode}-expanded"),
                [1440, 900],
                &mut records,
            )
            .await;
            call(
                &client,
                "presentation.layout.set",
                json!({"params":{"left_visible":false}}),
            )
            .await;
            if mode == "waveform" {
                layout_capture(
                    &client,
                    runtime.path(),
                    "waveform-left-hidden",
                    [1440, 900],
                    &mut records,
                )
                .await;
            }
            let hidden = response(&call(&client, "presentation.panels.toggle", json!({})).await);
            assert_eq!(hidden["result"]["layout"]["left"]["visible"], false);
            assert_eq!(hidden["result"]["layout"]["right"]["visible"], false);
            layout_capture(
                &client,
                runtime.path(),
                &format!("{mode}-hidden"),
                [1440, 900],
                &mut records,
            )
            .await;
            call(&client, "presentation.panels.toggle", json!({})).await;
            assert_eq!(saved_layout(&client).await["layout"], expected);
        }
        layout_capture(
            &client,
            runtime.path(),
            "multitrack-restored",
            [1440, 900],
            &mut records,
        )
        .await;
        call(
            &client,
            "presentation.layout.set",
            json!({"params":{"left_visible":false,"library_fraction":0.37}}),
        )
        .await;
        let persisted = saved_layout(&client).await["layout"].clone();
        gui.0.kill().unwrap();
        gui.0.wait().unwrap();
        client.cancel().await.unwrap();
        drop(gui);
        let _restarted =
            Gui::with_profile(runtime.path(), "layout-narrow", &preferences, "800x600");
        let narrow = named_client(runtime.path(), "layout-narrow").await;
        let restored = saved_layout(&narrow).await;
        assert_eq!(
            restored["layout"], persisted,
            "Restart must restore visibility, widths and split ratio"
        );
        assert_eq!(restored["revision"], 0);
        call(
            &narrow,
            "presentation.layout.set",
            json!({"params":{"left_visible":true}}),
        )
        .await;
        for mode in ["waveform", "multitrack"] {
            call(
                &narrow,
                "presentation.set_mode",
                json!({"params":{"mode":mode}}),
            )
            .await;
            layout_capture(
                &narrow,
                runtime.path(),
                &format!("{mode}-narrow"),
                [800, 600],
                &mut records,
            )
            .await;
        }
        let narrow_layout = saved_layout(&narrow).await["layout"].clone();
        assert_eq!(
            narrow_layout["left"]["width"], 320.0,
            "Narrow rendering must retain preferred expanded widths"
        );
        assert_eq!(narrow_layout["right"]["width"], 280.0);
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&directory).join("layout-metadata.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        narrow.cancel().await.unwrap();
    }
    #[tokio::test]
    #[ignore = "requires a real native GUI renderer; run explicitly on a desktop"]
    async fn real_audio_import_switching_and_pixels_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let size =
            std::env::var("WAVEBOB_AUDIO_TEST_WINDOW_SIZE").unwrap_or_else(|_| "1440x900".into());
        let scale = std::env::var("WAVEBOB_AUDIO_TEST_UI_SCALE").unwrap_or_else(|_| "1.0".into());
        let _gui = Gui::with_scale(
            runtime.path(),
            "real-audio",
            &runtime.path().join("layout.json"),
            &size,
            &scale,
        );
        let client = named_client(runtime.path(), "real-audio").await;
        let mut documents = Vec::new();
        for (name, channels) in [("Rain ambience.wav", 2), ("Footsteps.wav", 1)] {
            let path = runtime.path().join(name);
            let mut writer = hound::WavWriter::create(
                &path,
                hound::WavSpec {
                    channels,
                    sample_rate: 48000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            let mut noise = 123456789_u32;
            for frame in 0..576000 {
                let t = frame as f64 / 48000.0;
                for channel in 0..channels {
                    let envelope = 0.12
                        + 0.28 * (t * 0.37 + channel as f64 * 0.21).sin().powi(2)
                        + 0.07 * (t * 4.7).sin().abs();
                    noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
                    writer
                        .write_sample(
                            ((0.4
                                * (t * std::f64::consts::TAU * (220.0 + channel as f64 * 111.0))
                                    .sin()
                                + 0.6 * (noise as f64 / u32::MAX as f64 * 2.0 - 1.0))
                                * envelope
                                * 32767.0) as i16,
                        )
                        .unwrap();
                }
            }
            writer.finalize().unwrap();
            let original = fs::read(&path).unwrap();
            let opened =
                response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
            assert_eq!(opened["status"], "ok");
            let id = &opened["result"]["id"];
            let deadline = Instant::now() + Duration::from_secs(10);
            let document = loop {
                let job =
                    response(&call(&client, "jobs.get", json!({"params":{"job_id":id}})).await);
                match job["result"]["status"].as_str() {
                    Some("succeeded") => break job["result"]["result"].clone(),
                    Some("failed") => panic!("Import failed: {job}"),
                    _ => {}
                }
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(20)).await;
            };
            assert_eq!(document["frames"], 576000);
            assert_eq!(document["channels"], channels);
            assert_eq!(fs::read(&path).unwrap(), original);
            let samples = response(
                &call(
                    &client,
                    "documents.samples",
                    json!({"params":{"document_id":document["id"],"frames":32}}),
                )
                .await,
            );
            assert_eq!(
                samples["result"]["channels"].as_array().unwrap().len(),
                channels as usize
            );
            documents.push(document);
        }
        call(
            &client,
            "documents.activate",
            json!({"params":{"document_id":documents[0]["id"]}}),
        )
        .await;
        let state = response(&call(&client, "session.get", json!({})).await);
        assert_eq!(state["result"]["active_document_id"], documents[0]["id"]);
        let revision = state["revision"].clone();
        let mut records = Vec::new();
        for mode in ["waveform", "multitrack"] {
            let view = response(
                &call(
                    &client,
                    "presentation.set_mode",
                    json!({"params":{"mode":mode}}),
                )
                .await,
            );
            let path = runtime.path().join(format!("real-{mode}.png"));
            let result = call(&client,"capture.png",json!({"params":{"path":path,"after_revision":revision,"after_presentation_revision":view["result"]["revision"]}})).await;
            let capture = response(&result);
            assert_eq!(capture["status"], "ok");
            assert_eq!(capture["result"]["metadata"]["rendered_revision"], revision);
            let image = result
                .content
                .iter()
                .find_map(|block| match block {
                    ContentBlock::Image(image) => Some(image),
                    _ => None,
                })
                .unwrap();
            let png = BASE64_STANDARD.decode(&image.data).unwrap();
            assert_eq!(fs::read(&path).unwrap(), png);
            let mut reader = png::Decoder::new(std::io::Cursor::new(&png))
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut pixels).unwrap();
            assert!(
                pixels[..info.buffer_size()]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[2] > 150 && pixel[0] < 130 && pixel[1] > 70)
                    .count()
                    > 1000,
                "Waveform/clip color must be present"
            );
            if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
                fs::create_dir_all(&directory).unwrap();
                fs::write(Path::new(&directory).join(format!("real-{mode}.png")), &png).unwrap();
            }
            records.push(json!({"file":format!("real-{mode}.png"),"metadata":capture["result"]["metadata"],"documents":documents}));
        }
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&directory).join("audio-metadata.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }
    #[tokio::test]
    #[ignore = "requires a real native GUI renderer; run explicitly on a desktop"]
    async fn mp3_document_import_and_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let _gui = Gui::start(runtime.path(), "mp3-audio");
        let client = named_client(runtime.path(), "mp3-audio").await;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/vbr-2ch-44100-tagged.mp3");
        let started =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let id = &started["result"]["id"];
        let deadline = Instant::now() + Duration::from_secs(10);
        let job = loop {
            let job = response(&call(&client, "jobs.get", json!({"params":{"job_id":id}})).await);
            if job["result"]["status"] != "running" {
                break job;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(job["result"]["status"], "succeeded", "{job}");
        assert_eq!(job["result"]["result"]["frames"], 8192);
        assert_eq!(
            job["result"]["result"]["source_details"]["bitrate_mode"],
            "vbr"
        );
        let path = runtime.path().join("mp3.png");
        let result = call(
            &client,
            "capture.png",
            json!({"params":{"path":path,"after_revision":job["revision"]}}),
        )
        .await;
        let capture = response(&result);
        assert_eq!(capture["status"], "ok");
        let image = result
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .unwrap();
        let png = BASE64_STANDARD.decode(&image.data).unwrap();
        assert_eq!(fs::read(&path).unwrap(), png);
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(Path::new(&directory).join("mp3.png"), png).unwrap();
            fs::write(
                Path::new(&directory).join("mp3-metadata.json"),
                serde_json::to_vec_pretty(&json!({"job":job,"capture":capture})).unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[cfg(windows)]
    fn process_memory(pid: u32) -> Value {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile","-NonInteractive","-Command",&format!("$wavebobProcess=Get-Process -Id {pid}; @{{working_set_bytes=$wavebobProcess.WorkingSet64;peak_working_set_bytes=$wavebobProcess.PeakWorkingSet64;private_bytes=$wavebobProcess.PrivateMemorySize64}} | ConvertTo-Json -Compress")]);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let output = command.output().unwrap();
        assert!(output.status.success());
        serde_json::from_slice(&output.stdout).unwrap()
    }

    #[tokio::test]
    #[ignore = "requires a real native GUI renderer; run explicitly on a desktop"]
    async fn exact_selection_zoom_and_sample_detail_capture_through_mcp() {
        let runtime = tempfile::tempdir().unwrap();
        let path = runtime.path().join("Rain ambience.wav");
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        let mut noise = 123456789_u32;
        for frame in 0..576000 {
            let t = frame as f64 / 48000.0;
            for channel in 0..2 {
                noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
                let envelope = 0.15 + 0.35 * (t * 0.3 + channel as f64 * 0.2).sin().powi(2);
                writer
                    .write_sample(
                        ((0.4
                            * (t * std::f64::consts::TAU * (220.0 + channel as f64 * 111.0)).sin()
                            + 0.6 * (noise as f64 / u32::MAX as f64 * 2.0 - 1.0))
                            * envelope
                            * 32767.0) as i16,
                    )
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
        let original = fs::read(&path).unwrap();
        let _gui = Gui::start(runtime.path(), "selection-audio");
        let client = named_client(runtime.path(), "selection-audio").await;
        let opened =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let deadline = Instant::now() + Duration::from_secs(10);
        let doc = loop {
            let job = response(
                &call(
                    &client,
                    "jobs.get",
                    json!({"params":{"job_id":opened["result"]["id"]}}),
                )
                .await,
            );
            if job["result"]["status"] != "running" {
                assert_eq!(job["result"]["status"], "succeeded");
                break job["result"]["result"].clone();
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let selected = response(
            &call(
                &client,
                "documents.selection.set",
                json!({"params":{"document_id":doc["id"],"start_frame":144000,"end_frame":240000}}),
            )
            .await,
        );
        assert_eq!(
            selected["result"]["selection"],
            json!({"start":144000,"end":240000})
        );
        assert_eq!(selected["result"]["cursor"], 144000);
        assert_eq!(selected["result"]["dirty"], false);
        let revision = selected["revision"].clone();
        let mut records = Vec::new();
        for (label, start, end) in [
            ("waveform-zoomed", 96000, 384000),
            ("waveform-samples", 143995, 144020),
            ("waveform-single-frame", 144000, 144001),
        ] {
            let viewport = response(
                &call(
                    &client,
                    "presentation.viewport.set",
                    json!({"params":{"document_id":doc["id"],"start_frame":start,"end_frame":end}}),
                )
                .await,
            );
            assert_eq!(viewport["status"], "ok");
            assert_eq!(viewport["revision"], revision);
            let current = response(
                &call(
                    &client,
                    "documents.get",
                    json!({"params":{"document_id":doc["id"]}}),
                )
                .await,
            );
            assert_eq!(current["result"], selected["result"]);
            if end - start <= 25 {
                let samples=response(&call(&client,"documents.samples",json!({"params":{"document_id":doc["id"],"start_frame":start,"frames":end-start}})).await);
                let peaks=response(&call(&client,"documents.waveform",json!({"params":{"document_id":doc["id"],"start_frame":start,"end_frame":end,"bins":32}})).await);
                for channel in 0..2 {
                    for frame in 0..(end - start) as usize {
                        assert_eq!(
                            peaks["result"]["channels"][channel][frame]["min"],
                            samples["result"]["channels"][channel][frame]
                        );
                        assert_eq!(
                            peaks["result"]["channels"][channel][frame]["max"],
                            samples["result"]["channels"][channel][frame]
                        );
                    }
                }
            }
            let path = runtime.path().join(format!("{label}.png"));
            let result=call(&client,"capture.png",json!({"params":{"path":path,"after_revision":revision,"after_presentation_revision":viewport["result"]["revision"]}})).await;
            let captured = response(&result);
            assert_eq!(captured["status"], "ok");
            assert_eq!(
                captured["result"]["metadata"]["rendered_revision"],
                revision
            );
            assert_eq!(
                captured["result"]["metadata"]["pending_resources"],
                json!([])
            );
            let image = result
                .content
                .iter()
                .find_map(|block| match block {
                    ContentBlock::Image(image) => Some(image),
                    _ => None,
                })
                .unwrap();
            let png = BASE64_STANDARD.decode(&image.data).unwrap();
            assert_eq!(png, fs::read(&path).unwrap());
            if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
                fs::create_dir_all(&directory).unwrap();
                fs::write(Path::new(&directory).join(format!("{label}.png")), png).unwrap();
            }
            records.push(json!({"file":format!("{label}.png"),"capture":captured,"document":current["result"],"view":viewport["result"]}));
        }
        call(
            &client,
            "presentation.set_mode",
            json!({"params":{"mode":"multitrack"}}),
        )
        .await;
        call(
            &client,
            "presentation.set_mode",
            json!({"params":{"mode":"waveform"}}),
        )
        .await;
        assert_eq!(
            response(
                &call(
                    &client,
                    "documents.get",
                    json!({"params":{"document_id":doc["id"]}})
                )
                .await
            )["result"],
            selected["result"]
        );
        assert_eq!(fs::read(path).unwrap(), original);
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&directory).join("selection-metadata.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "measured large-file native GUI benchmark; run explicitly in release mode"]
    async fn large_import_keeps_live_navigation_and_cancellation_responsive() {
        let runtime = tempfile::tempdir().unwrap();
        let path = runtime.path().join("ten-minutes.wav");
        let frames = 48000_u64 * 600;
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..frames {
            let sample = ((frame % 1000) as i16 - 500) * 40;
            writer.write_sample(sample).unwrap();
            writer.write_sample(-sample).unwrap();
        }
        writer.finalize().unwrap();
        let gui = Gui::start(runtime.path(), "large-audio");
        let client = named_client(runtime.path(), "large-audio").await;
        #[cfg(windows)]
        let before = process_memory(gui.0.id());
        #[cfg(not(windows))]
        let before = Value::Null;
        let began = Instant::now();
        let started =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let id = &started["result"]["id"];
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut latency = Vec::<f64>::new();
        let mut progress = Vec::new();
        let job = loop {
            let began = Instant::now();
            let view=response(&call(&client,"presentation.set_mode",json!({"params":{"mode":if latency.len().is_multiple_of(2) {"waveform"} else {"multitrack"}}})).await);
            assert_eq!(view["status"], "ok");
            latency.push(began.elapsed().as_secs_f64() * 1000.0);
            let job = response(&call(&client, "jobs.get", json!({"params":{"job_id":id}})).await);
            progress.push(job["result"]["progress"].clone());
            if job["result"]["status"] != "running" {
                break job;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        let import_ms = began.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(job["result"]["status"], "succeeded", "{job}");
        assert_eq!(job["result"]["result"]["frames"], frames);
        let max_ms = latency.iter().copied().fold(0.0_f64, f64::max);
        assert!(max_ms < 2000.0, "Live navigation stalled for {max_ms} ms");
        let peak_start = Instant::now();
        let wave = response(
            &call(
                &client,
                "documents.waveform",
                json!({"params":{"document_id":job["result"]["result"]["id"],"bins":1024}}),
            )
            .await,
        );
        assert_eq!(wave["status"], "ok");
        let waveform_ms = peak_start.elapsed().as_secs_f64() * 1000.0;
        #[cfg(windows)]
        let loaded = process_memory(gui.0.id());
        #[cfg(not(windows))]
        let loaded = Value::Null;
        let second =
            response(&call(&client, "documents.open", json!({"params":{"path":path}})).await);
        let cancelled = response(
            &call(
                &client,
                "jobs.cancel",
                json!({"params":{"job_id":second["result"]["id"]}}),
            )
            .await,
        );
        assert_eq!(cancelled["result"]["status"], "cancelled", "{cancelled}");
        let state = response(&call(&client, "session.get", json!({})).await);
        assert_eq!(state["result"]["documents"].as_array().unwrap().len(), 1);
        let mut doc = job["result"]["result"].clone();
        let mut background = Vec::new();
        for phase in ["effect", "export"] {
            let began = Instant::now();
            let started = if phase == "effect" {
                response(&call(&client,"edit.process",json!({"params":{"target":{"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]},"effect":{"kind":"gain","db":-6.0}}})).await)
            } else {
                response(&call(&client,"audio.export",json!({"params":{"target":{"scope":"whole","document_id":doc["id"],"revision_id":doc["revision_id"]},"path":runtime.path().join("ten-minute-output.wav"),"format":{"kind":"wav","encoding":"pcm16"}}})).await)
            };
            assert_eq!(started["status"], "ok", "{started}");
            let deadline = Instant::now() + Duration::from_secs(120);
            let mut requests = Vec::new();
            let mut progress = Vec::new();
            let finished = loop {
                let request_start = Instant::now();
                assert_eq!(response(&call(&client,"presentation.set_mode",json!({"params":{"mode":if requests.len().is_multiple_of(2){"waveform"}else{"multitrack"}}})).await)["status"],"ok");
                requests.push(request_start.elapsed().as_secs_f64() * 1000.0);
                let job = response(
                    &call(
                        &client,
                        "jobs.get",
                        json!({"params":{"job_id":started["result"]["id"]}}),
                    )
                    .await,
                )["result"]
                    .clone();
                progress.push(job["progress"].clone());
                if job["status"] != "running" {
                    break job;
                }
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(5)).await;
            };
            assert_eq!(finished["status"], "succeeded", "{finished}");
            assert!(
                requests.iter().all(|ms| *ms < 3000.0),
                "{phase} navigation stalled: {requests:?}"
            );
            if phase == "effect" {
                doc = finished["result"].clone();
            }
            background.push(json!({"phase":phase,"total_ms":began.elapsed().as_secs_f64()*1000.0,"navigation_ms":requests,"progress":progress}));
        }
        #[cfg(windows)]
        let edited = process_memory(gui.0.id());
        #[cfg(not(windows))]
        let edited = Value::Null;
        let record = json!({"frames":frames,"channels":2,"sample_rate":48000,"source_bytes":fs::metadata(path).unwrap().len(),"decoded_bytes":frames*2*4,"import_ms":import_ms,"navigation_ms":latency,"max_navigation_ms":max_ms,"progress_observed":progress,"waveform_query_ms":waveform_ms,"before":before,"loaded":loaded,"edited":edited,"background":background,"cancellation":cancelled["result"]["status"]});
        eprintln!("{}", serde_json::to_string_pretty(&record).unwrap());
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                Path::new(&directory).join("large-import.json"),
                serde_json::to_vec_pretty(&record).unwrap(),
            )
            .unwrap();
        }
        client.cancel().await.unwrap();
        drop(gui);
    }
    impl Drop for Gui {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn attached_headless(runtime: &Path, id: &str, command: &str) -> Value {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wavebob-headless"))
            .args(["--attach", id, "--runtime-dir"])
            .arg(runtime)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"id":"contract-query","command":command})
        )
        .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    #[tokio::test]
    #[ignore = "requires two real native GUI renderers; run explicitly on a desktop"]
    async fn attached_stdio_client_retrieves_fresh_pngs_from_the_running_app() {
        let runtime = tempfile::tempdir().unwrap();
        let mut first = Gui::start(runtime.path(), "mcp-first");
        let mut second = Gui::start(runtime.path(), "mcp-second");
        let deadline = Instant::now() + Duration::from_secs(20);
        let instances = loop {
            let instances = ipc::discover(runtime.path()).unwrap();
            if instances.len() == 2 {
                break instances;
            }
            for gui in [&mut first, &mut second] {
                assert!(
                    gui.0.try_wait().unwrap().is_none(),
                    "GUI exited during startup"
                );
            }
            assert!(Instant::now() < deadline, "GUIs did not register");
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let first_id = &instances
            .iter()
            .find(|instance| instance.name == "mcp-first")
            .unwrap()
            .session_id;
        let second_id = &instances
            .iter()
            .find(|instance| instance.name == "mcp-second")
            .unwrap()
            .session_id;
        let client = connect(
            ProtocolVersion::V_2026_07_28,
            &[
                "--attach".as_ref(),
                first_id.as_ref(),
                "--runtime-dir".as_ref(),
                runtime.path().as_os_str(),
            ],
        )
        .await;
        let direct = ipc::Client::attach(runtime.path(), first_id).unwrap();
        let other = ipc::Client::attach(runtime.path(), second_id).unwrap();
        let tools = client.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 77);
        for tool in &tools {
            let schema = Value::Object((*tool.input_schema).clone());
            assert_local_refs(&schema, &schema);
        }
        let looped = response(
            &call(
                &client,
                "transport.loop.set",
                json!({"params":{"enabled":true},"expected_revision":0,"session_id":first_id}),
            )
            .await,
        );
        assert_eq!(looped["session_id"], *first_id);
        for same in [
            serde_json::to_value(
                direct
                    .call(r#"{"id":"direct","command":"session.get"}"#)
                    .unwrap(),
            )
            .unwrap(),
            attached_headless(runtime.path(), first_id, "session.get"),
        ] {
            assert_eq!(same["result"], looped["result"]);
            assert_eq!(same["revision"], looped["revision"]);
        }
        let untouched = serde_json::to_value(
            other
                .call(r#"{"id":"other","command":"session.get"}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(untouched["revision"], 0);
        assert_eq!(untouched["result"]["loop_enabled"], false);
        let mut records = Vec::new();
        for (mode, presentation_revision) in [("multitrack", 1), ("waveform", 2)] {
            let view = response(&call(&client,"presentation.set_mode",json!({"params":{"mode":mode,"expected_presentation_revision":presentation_revision-1}})).await);
            for same in [
                serde_json::to_value(
                    direct
                        .call(r#"{"id":"direct","command":"presentation.get"}"#)
                        .unwrap(),
                )
                .unwrap(),
                attached_headless(runtime.path(), first_id, "presentation.get"),
            ] {
                assert_eq!(same["result"], view["result"]);
            }
            let barrier = response(&call(&client,"presentation.barrier",json!({"params":{"after_revision":1,"after_presentation_revision":presentation_revision}})).await);
            assert_eq!(barrier["result"]["mode"], mode);
            let path = runtime.path().join(format!("{mode}.png"));
            let result = call(&client,"capture.png",json!({"params":{"path":path,"after_revision":1,"after_presentation_revision":presentation_revision},"session_id":first_id})).await;
            let capture = response(&result);
            assert_eq!(capture["status"], "ok");
            let metadata = &capture["result"]["metadata"];
            assert_eq!(metadata["session_id"], *first_id);
            assert_eq!(metadata["mode"], mode);
            assert_eq!(metadata["rendered_revision"], 1);
            assert_eq!(metadata["presentation_revision"], presentation_revision);
            let image = result
                .content
                .iter()
                .find_map(|block| match block {
                    ContentBlock::Image(image) => Some(image),
                    _ => None,
                })
                .expect("MCP result must deliver image content");
            assert_eq!(image.mime_type, "image/png");
            let png = BASE64_STANDARD.decode(&image.data).unwrap();
            assert_eq!(
                png,
                fs::read(&path).unwrap(),
                "MCP-delivered image must be the captured PNG"
            );
            let mut reader = png::Decoder::new(std::io::Cursor::new(&png))
                .read_info()
                .unwrap();
            let mut data = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut data).unwrap();
            assert_eq!(metadata["width"], info.width);
            assert_eq!(metadata["height"], info.height);
            assert!(
                data.windows(4).any(|window| window != &data[..4]),
                "PNG must contain rendered pixels"
            );
            if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
                fs::create_dir_all(&directory).unwrap();
                fs::write(Path::new(&directory).join(format!("mcp-{mode}.png")), &png).unwrap();
            }
            records.push(json!({"file":format!("mcp-{mode}.png"),"metadata":metadata,"delivered_bytes":png.len()}));
            let refusal = call(&client, "capture.png", json!({"params":{"path":path}})).await;
            assert_eq!(response(&refusal)["error"]["code"], "output_exists");
            assert!(
                !refusal
                    .content
                    .iter()
                    .any(|block| matches!(block, ContentBlock::Image(_)))
            );
        }
        let timed = response(
            &call(
                &client,
                "capture.png",
                json!({"params":{"after_revision":999,"timeout_ms":100}}),
            )
            .await,
        );
        assert_eq!(timed["error"]["code"], "timeout");
        assert_eq!(
            response(&call(&client, "presentation.get", json!({})).await)["result"]["mode"],
            "waveform"
        );
        if let Some(directory) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&directory).join("mcp-metadata.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        first.0.kill().unwrap();
        first.0.wait().unwrap();
        let error = client
            .call_tool(CallToolRequestParams::new("session.get"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("transport failed"));
        assert_eq!(ipc::discover(runtime.path()).unwrap().len(), 1);
        client.cancel().await.unwrap();
    }
}
