use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use wavebob::{Session, protocol};

#[test]
fn framed_stream_recovers_after_bad_frames_and_accepts_eof_without_newline() {
    let input = concat!(
        "\r\n",
        "not-json\n",
        "{\"id\":\"missing\"}\n",
        "{\"id\":\"toggle\",\"command\":\"transport.loop.toggle\"}\r\n",
        "{\"id\":\"get\",\"command\":\"session.get\"}",
    );
    let mut output = Vec::new();
    protocol::run(&mut Session::default(), input.as_bytes(), &mut output).unwrap();
    let frames: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 4);
    assert_eq!(frames[0]["id"], Value::Null);
    assert_eq!(frames[0]["error"]["code"], "invalid_json");
    assert_eq!(frames[1]["id"], "missing");
    assert_eq!(frames[1]["error"]["code"], "invalid_request");
    assert_eq!(frames[2]["id"], "toggle");
    assert_eq!(frames[3]["result"]["loop_enabled"], true);
    assert_eq!(frames[3]["revision"], 1);
}

#[test]
fn process_flushes_responses_before_eof_and_keeps_stdout_machine_readable() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wavebob-headless"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let output = BufReader::new(child.stdout.take().unwrap());
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for frame in output.lines() {
            if sender.send(frame).is_err() {
                break;
            }
        }
    });
    for (id, command, revision, enabled) in [
        ("first", "transport.loop.toggle", 1, true),
        ("read", "session.get", 1, true),
        ("second", "transport.loop.toggle", 2, false),
    ] {
        writeln!(input, "{}", json!({"id":id,"command":command})).unwrap();
        input.flush().unwrap();
        let frame = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("response must be flushed before stdin closes")
            .unwrap();
        let response: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(response["id"], id);
        assert_eq!(response["revision"], revision);
        assert_eq!(response["result"]["loop_enabled"], enabled);
    }
    drop(input);
    let result = child.wait_with_output().unwrap();
    reader.join().unwrap();
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
}

#[test]
fn invalid_utf8_is_a_frame_error_and_does_not_end_the_session() {
    let mut input = vec![0xff, b'\n'];
    input.extend_from_slice(b"{\"id\":\"get\",\"command\":\"session.get\"}\n");
    let mut output = Vec::new();
    protocol::run(&mut Session::default(), input.as_slice(), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["error"]["code"], "invalid_json");
    assert_eq!(responses[1]["status"], "ok");
    assert_eq!(responses[1]["revision"], 0);
}

#[test]
fn unknown_cli_arguments_fail_on_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_wavebob-headless"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}
