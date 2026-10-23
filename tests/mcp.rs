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
        assert_eq!(tools.len(), 4);
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
            let executable = std::env::var_os("WAVEBOB_GUI_TEST_BIN")
                .unwrap_or_else(|| env!("CARGO_BIN_EXE_wavebob").into());
            let mut command = Command::new(executable);
            command
                .args(["--hidden", "--name", name, "--runtime-dir"])
                .arg(runtime)
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
        assert_eq!(tools.len(), 8);
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
