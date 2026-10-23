use serde_json::{Value, json};
use wavebob::{Command, EmptyParams, LoopParams, Outcome, Request, Session, protocol};

fn send(session: &mut Session, request: Value) -> Value {
    serde_json::to_value(protocol::dispatch_line(session, &request.to_string())).unwrap()
}

#[test]
fn typed_and_wire_commands_have_equivalent_results() {
    let mut typed = Session::default();
    let mut wire = Session::default();
    for (step, enabled) in [true, true, false, true].into_iter().enumerate() {
        let id = format!("step-{step}");
        let response = typed.execute(
            Request::new(&id, Command::SetLoop(LoopParams { enabled }))
                .expecting_revision(typed.snapshot().revision),
        );
        let expected_revision = wire.snapshot().revision;
        let result = send(
            &mut wire,
            json!({
                "id": id, "command": "transport.loop.set", "params": {"enabled": enabled},
                "expected_revision": expected_revision,
            }),
        );
        assert!(matches!(response.outcome, Outcome::Ok { .. }));
        assert_eq!(result["status"], "ok");
        assert_eq!(typed.snapshot().loop_enabled, wire.snapshot().loop_enabled);
        assert_eq!(response.revision, result["revision"].as_u64().unwrap());
        assert_eq!(result["id"], id);
    }
    assert_eq!(
        typed.snapshot().revision,
        3,
        "idempotent sets do not increment"
    );
}

#[test]
fn stale_writes_and_queries_never_change_state() {
    let mut session = Session::default();
    send(
        &mut session,
        json!({"id":"first","command":"transport.loop.toggle"}),
    );
    let before = session.snapshot();
    let stale = send(
        &mut session,
        json!({
            "id":"stale", "command":"transport.loop.set", "params":{"enabled":false},
            "expected_revision":0,
        }),
    );
    assert_eq!(stale["error"]["code"], "stale_revision");
    assert_eq!(
        stale["error"]["details"],
        json!({"expected_revision":0,"actual_revision":1})
    );
    assert_eq!(stale["id"], "stale");
    assert_eq!(session.snapshot(), before);
    let query = send(
        &mut session,
        json!({
            "id":"read", "command":"session.get", "expected_revision":0,
        }),
    );
    assert_eq!(query["result"]["loop_enabled"], true);
    assert_eq!(query["result"]["session_id"], before.session_id);
    assert_eq!(session.snapshot(), before);
}

#[test]
fn rejected_requests_are_correlated_and_atomic() {
    let cases = [
        (
            json!({"id":"bad","command":"audio.play"}),
            "unknown_command",
        ),
        (
            json!({"id":"bad","command":"transport.loop.set"}),
            "invalid_params",
        ),
        (
            json!({"id":"bad","command":"transport.loop.set","params":{"enabled":"yes"}}),
            "invalid_params",
        ),
        (
            json!({"id":"bad","command":"transport.loop.set","params":{"enabled":true,"extra":1}}),
            "invalid_params",
        ),
        (
            json!({"id":"bad","command":"transport.loop.toggle","params":{"extra":1}}),
            "invalid_params",
        ),
        (
            json!({"id":"bad","command":"transport.loop.toggle","params":[]}),
            "invalid_params",
        ),
        (
            json!({"id":"bad","command":"transport.loop.toggle","params":null}),
            "invalid_params",
        ),
        (
            json!({"id":"bad","command":"session.get","extra":1}),
            "invalid_request",
        ),
        (
            json!({"id":"bad","command":"session.get","protocol_version":2}),
            "unsupported_protocol",
        ),
        (
            json!({"id":"bad","command":"transport.loop.toggle","expected_revision":-1}),
            "invalid_request",
        ),
        (
            json!({"id":"bad","command":"transport.loop.toggle","expected_revision":1.5}),
            "invalid_request",
        ),
    ];
    let mut session = Session::default();
    let before = session.snapshot();
    for (request, code) in cases {
        let response = send(&mut session, request);
        assert_eq!(response["status"], "error");
        assert_eq!(response["id"], "bad");
        assert_eq!(response["error"]["code"], code);
        assert_eq!(session.snapshot(), before);
    }
    let blank = send(
        &mut session,
        json!({"id":"  ","command":"transport.loop.toggle"}),
    );
    assert_eq!(blank["error"]["code"], "invalid_request");
    assert_eq!(session.snapshot(), before);
}

#[test]
fn catalogue_advertises_only_implemented_commands_and_versioned_schemas() {
    let mut session = Session::default();
    let response = send(
        &mut session,
        json!({"id":"discover","command":"session.capabilities"}),
    );
    let result = &response["result"];
    assert_eq!(result["protocol_version"], 1);
    assert_eq!(
        result["capabilities"],
        json!(["session", "transport.loop", "documents", "waveform", "jobs"])
    );
    assert_eq!(result["request_schema"]["additionalProperties"], false);
    assert!(result["response_schema"]["properties"]["revision"].is_object());
    let commands = result["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 11);
    for command in commands {
        assert_eq!(command["input_schema"]["type"], "object");
        assert_eq!(command["input_schema"]["additionalProperties"], false);
        assert_eq!(
            command["input_schema"]["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        if command["input_schema"]["required"]
            .as_array()
            .is_some_and(|fields| !fields.is_empty())
            && command["name"] != "transport.loop.set"
        {
            continue;
        }
        let request = json!({
            "id":"probe", "command":command["name"],
            "params": if command["name"] == "transport.loop.set" { json!({"enabled":true}) } else { json!({}) },
        });
        assert_eq!(send(&mut session, request)["status"], "ok");
    }
}

#[test]
fn snapshots_are_owned_and_sessions_have_independent_identity() {
    let session = Session::default();
    let other = Session::default();
    assert_ne!(session.snapshot().session_id, other.snapshot().session_id);
    let mut snapshot = session.snapshot();
    snapshot.loop_enabled = true;
    assert!(!session.snapshot().loop_enabled);
}

#[test]
fn typed_requests_validate_blank_identifiers_too() {
    let mut session = Session::default();
    let response = session.execute(Request::new("", Command::ToggleLoop(EmptyParams {})));
    assert!(matches!(response.outcome, Outcome::Error { .. }));
    assert_eq!(session.snapshot().revision, 0);
}

#[test]
fn gui_free_session_reports_missing_renderer_without_creating_output() {
    let mut session = Session::default();
    let before = session.snapshot();
    for (command, params) in [
        ("presentation.get", json!({})),
        ("presentation.set_mode", json!({"mode":"multitrack"})),
        ("presentation.barrier", json!({})),
        ("capture.png", json!({})),
    ] {
        let response = send(
            &mut session,
            json!({"id":"gui-free", "command":command,"params":params}),
        );
        assert_eq!(response["error"]["code"], "missing_capability");
        assert_eq!(session.snapshot(), before);
    }
}
