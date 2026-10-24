use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    service::{RoleClient, RunningService},
    transport::TokioChildProcess,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

pub enum Adapter {
    Direct(Box<wavebob::Session>),
    Json {
        child: Child,
        input: ChildStdin,
        output: BufReader<ChildStdout>,
    },
    Mcp(RunningService<RoleClient, ClientConfig>),
}
impl Adapter {
    pub async fn connect(
        kind: &str,
        binary: Option<&str>,
        attach: Option<&str>,
        runtime: Option<&str>,
    ) -> Self {
        if kind == "direct" {
            assert!(attach.is_none());
            return Self::Direct(Box::default());
        }
        let binary = binary.expect("--binary is required for headless or mcp");
        let args: Vec<&str> = attach
            .into_iter()
            .flat_map(|id| ["--attach", id])
            .chain(runtime.into_iter().flat_map(|path| ["--runtime-dir", path]))
            .collect();
        if kind == "headless" {
            let mut command = Command::new(binary);
            command
                .args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let mut child = command.spawn().unwrap();
            let input = child.stdin.take().unwrap();
            let output = BufReader::new(child.stdout.take().unwrap());
            Self::Json {
                child,
                input,
                output,
            }
        } else {
            assert_eq!(kind, "mcp");
            let mut command = tokio::process::Command::new(binary);
            command.args(args);
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            Self::Mcp(
                ClientConfig::default()
                    .serve(TokioChildProcess::new(command).unwrap())
                    .await
                    .unwrap(),
            )
        }
    }
    pub async fn raw(&mut self, name: &str, params: Value) -> Value {
        let request = json!({"id":"workflow","command":name,"params":params});
        match self {
            Self::Direct(session) => serde_json::to_value(wavebob::protocol::dispatch_line(
                session,
                &request.to_string(),
            ))
            .unwrap(),
            Self::Json { input, output, .. } => {
                writeln!(input, "{request}").unwrap();
                input.flush().unwrap();
                let mut line = String::new();
                assert!(output.read_line(&mut line).unwrap() > 0);
                serde_json::from_str(&line).unwrap()
            }
            Self::Mcp(client) => {
                let result = client
                    .call_tool(
                        CallToolRequestParams::new(name.to_owned())
                            .with_arguments(json!({"params":params}).as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                result.structured_content.unwrap()
            }
        }
    }
    pub async fn ok(&mut self, name: &str, params: Value) -> Value {
        let response = self.raw(name, params).await;
        assert_eq!(response["status"], "ok", "{name}: {response}");
        response["result"].clone()
    }
    pub async fn job(&mut self, name: &str, params: Value) -> Value {
        eprintln!("{name}");
        let started = self.ok(name, params).await;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let job = self.ok("jobs.get", json!({"job_id":started["id"]})).await;
            if job["status"] != "running" {
                assert_eq!(job["status"], "succeeded", "{name}: {job}");
                return job;
            }
            assert!(Instant::now() < deadline, "{name} timed out");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    pub async fn shutdown(self) {
        match self {
            Self::Json {
                mut child, input, ..
            } => {
                drop(input);
                assert!(child.wait().unwrap().success());
            }
            Self::Mcp(client) => {
                client.cancel().await.unwrap();
            }
            Self::Direct(_) => {}
        }
    }
    pub async fn capture(&mut self, path: &Path) -> Value {
        let state = self.raw("session.get", json!({})).await;
        let view = self.ok("presentation.get", json!({})).await;
        let params = json!({"path":path,"after_revision":state["revision"],"after_presentation_revision":view["revision"]});
        if let Self::Mcp(client) = self {
            use base64::Engine;
            let result = client
                .call_tool(
                    CallToolRequestParams::new("capture.png")
                        .with_arguments(json!({"params":params}).as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            let value = result.structured_content.unwrap();
            assert_eq!(value["status"], "ok", "{value}");
            let image = result
                .content
                .iter()
                .find_map(|item| item.as_image())
                .expect("MCP must deliver PNG image content");
            assert_eq!(image.mime_type, "image/png");
            assert_eq!(
                base64::prelude::BASE64_STANDARD
                    .decode(&image.data)
                    .unwrap(),
                fs::read(path).unwrap()
            );
            assert_eq!(value["session_id"], state["session_id"]);
            assert!(
                value["result"]["metadata"]["rendered_revision"]
                    .as_u64()
                    .unwrap()
                    >= state["revision"].as_u64().unwrap()
            );
            value["result"].clone()
        } else {
            self.ok("capture.png", params).await
        }
    }
}
fn target(doc: &Value, start: u64, end: u64) -> Value {
    json!({"scope":"range","document_id":doc["id"],"revision_id":doc["revision_id"],"start_frame":start,"end_frame":end})
}
fn tone(path: &Path, rate: u32, channels: u16, frequency: f64) {
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for i in 0..rate as u64 * 12 {
        let t = i as f64 / rate as f64;
        for channel in 0..channels {
            let envelope = 0.22 + 0.12 * (t * 1.7).sin() + 0.06 * (t * 4.3).cos();
            let signal =
                (t * frequency * (1.0 + channel as f64 * 0.09) * std::f64::consts::TAU).sin() * 0.7
                    + (t * frequency * 1.93 * std::f64::consts::TAU).sin() * 0.3;
            writer
                .write_sample((signal * envelope * 32767.0) as i16)
                .unwrap();
        }
    }
    writer.finalize().unwrap();
}
pub async fn run(a: &mut Adapter, output: &Path, audition: bool) -> Value {
    assert!(
        !output.exists(),
        "Use a new output directory; the workflow never overwrites existing work"
    );
    fs::create_dir_all(output).unwrap();
    let output = fs::canonicalize(output).unwrap();
    let sources = output.join("Sources");
    fs::create_dir(&sources).unwrap();
    let wav_path = sources.join("Rain ambience.wav");
    tone(&wav_path, 48000, 2, 191.0);
    let piano_path = sources.join("Piano texture.wav");
    tone(&piano_path, 32000, 1, 261.63);
    let mut wav = a.job("documents.open", json!({"path":wav_path})).await["result"].clone();
    let mp3_path = sources.join("Footsteps.mp3");
    a.job("audio.export", json!({"target":{"scope":"whole","document_id":wav["id"],"revision_id":wav["revision_id"]},"path":mp3_path,"format":{"kind":"mp3","bitrate_kbps":192}})).await;
    a.ok("browser.navigate", json!({"path":sources})).await;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let browser = a.ok("browser.get", json!({})).await;
        if browser["loading"] == false {
            assert!(browser["error"].is_null());
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    a.ok("browser.select", json!({"path":mp3_path})).await;
    if audition {
        a.ok("audition.start", json!({"path":mp3_path})).await;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let browser = a.ok("browser.get", json!({})).await;
            assert!(browser["audition"]["error"].is_null(), "{browser}");
            if browser["audition"]["playing"] == true {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        a.ok("audition.stop", json!({})).await;
    }
    let mp3 = a.job("browser.open", json!({"path":mp3_path})).await["result"].clone();
    let piano = a.job("documents.open", json!({"path":piano_path})).await["result"].clone();
    a.ok("documents.activate", json!({"document_id":wav["id"]}))
        .await;
    a.ok(
        "documents.selection.set",
        json!({"document_id":wav["id"],"start_frame":144000,"end_frame":336000}),
    )
    .await;
    wav = a
        .job(
            "effects.apply",
            json!({"target":target(&wav,144000,336000),"effect":{"kind":"gain","db":-3.0}}),
        )
        .await["result"]
        .clone();
    a.ok("history.undo", json!({})).await;
    a.ok("history.redo", json!({})).await;
    a.ok("edit.copy", json!({"target":target(&wav,48000,96000)}))
        .await;
    let pasted = a.job("edit.paste", json!({"document_id":piano["id"],"revision_id":piano["revision_id"],"start_frame":32000,"end_frame":32000})).await["result"].clone();
    assert_eq!(pasted["frames"], 416000);
    a.ok("history.undo", json!({})).await;
    let stale = a.raw("effects.apply", json!({"target":{"scope":"whole","document_id":wav["id"],"revision_id":"stale"},"effect":{"kind":"reverse"}})).await;
    assert_eq!(stale["status"], "error");
    let tracks = ["Ambience", "Footsteps", "Texture", "Track 4"];
    for name in tracks {
        a.ok("tracks.add", json!({"name":name})).await;
    }
    let arr = a.ok("arrangement.get", json!({})).await;
    for (track, doc, start, end, position) in [
        (0, &wav, 0, 576000, 0),
        (1, &mp3, 0, 192000, 48000),
        (1, &mp3, 0, 192000, 192000),
        (2, &piano, 0, 128000, 128000),
        (2, &piano, 0, 64000, 448000),
    ] {
        a.ok("clips.insert", json!({"track_id":arr["tracks"][track]["id"],"document_id":doc["id"],"revision_id":doc["revision_id"],"source_start":start,"source_end":end,"position":position})).await;
    }
    a.ok(
        "tracks.update",
        json!({"track_id":arr["tracks"][1]["id"],"gain_db":-6.0,"pan":0.15}),
    )
    .await;
    a.ok(
        "tracks.update",
        json!({"track_id":arr["tracks"][2]["id"],"gain_db":-3.0,"pan":-0.2}),
    )
    .await;
    let arr = a.ok("arrangement.get", json!({})).await;
    let clip = &arr["tracks"][2]["clips"][0];
    a.job("clips.effects.apply", json!({"clip_id":clip["id"],"edit_revision_id":clip["edit_revision_id"],"effect":{"kind":"reverb","room_size":0.4,"decay_seconds":0.8,"wet":0.2}})).await;
    let arr = a.ok("arrangement.get", json!({})).await;
    assert_eq!(
        a.ok("arrangement.crossfades", json!({})).await["regions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    a.job("audio.export", json!({"target":target(&wav,144000,336000),"path":output.join("Sample selection.mp3"),"format":{"kind":"mp3","bitrate_kbps":192},"sample_rate":44100})).await;
    for (name, format) in [
        ("Mix.wav", json!({"kind":"wav","encoding":"float32"})),
        ("Mix.mp3", json!({"kind":"mp3","bitrate_kbps":192})),
    ] {
        a.job("mix.export", json!({"revision_id":arr["revision_id"],"start_frame":0,"end_frame":576000,"path":output.join(name),"format":format})).await;
    }
    let sample = a
        .job(
            "documents.open",
            json!({"path":output.join("Sample selection.mp3")}),
        )
        .await["result"]
        .clone();
    assert_eq!(sample["frames"], 176400);
    assert_eq!(sample["sample_rate"], 44100);
    a.ok("documents.close", json!({"document_id":sample["id"]}))
        .await;
    let imported = a
        .job("documents.open", json!({"path":output.join("Mix.mp3")}))
        .await["result"]
        .clone();
    assert_eq!(imported["frames"], 576000);
    assert_eq!(imported["channels"], 2);
    a.ok("documents.close", json!({"document_id":imported["id"]}))
        .await;
    a.job("mix.prepare", json!({"revision_id":arr["revision_id"]}))
        .await;
    let pcm = a
        .ok(
            "mix.samples",
            json!({"revision_id":arr["revision_id"],"start_frame":192000,"frames":4096}),
        )
        .await["channels"]
        .clone();
    let project = output.join("Editing session.wavebob");
    a.job("project.save", json!({"path":project})).await;
    for path in [wav_path, mp3_path, piano_path] {
        fs::remove_file(path).unwrap();
    }
    a.job("project.open", json!({"path":project})).await;
    assert_eq!(a.ok("project.get", json!({})).await["dirty"], false);
    a.job("mix.prepare", json!({"revision_id":arr["revision_id"]}))
        .await;
    assert_eq!(
        a.ok(
            "mix.samples",
            json!({"revision_id":arr["revision_id"],"start_frame":192000,"frames":4096})
        )
        .await["channels"],
        pcm
    );
    let gui = a.ok("session.capabilities", json!({})).await["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "capture.png" && command["requires_gui"] == true);
    // Headless catalogues include unavailable GUI descriptors too: presentation.get
    // reports the explicit renderer boundary without creating a window/device.
    let rendered = gui && a.raw("presentation.get", json!({})).await["status"] == "ok";
    let mut captures = Vec::new();
    if rendered {
        a.ok("documents.activate", json!({"document_id":wav["id"]}))
            .await;
        a.ok(
            "presentation.viewport.set",
            json!({"document_id":wav["id"],"start_frame":96000,"end_frame":480000}),
        )
        .await;
        a.ok(
            "presentation.timeline.set",
            json!({"start_frame":0,"end_frame":624000}),
        )
        .await;
        a.ok("arrangement.navigate",json!({"cursor":240000,"selection":{"start":192000,"end":336000},"selected_clip_ids":[arr["tracks"][1]["clips"][1]["id"]]})).await;
        for mode in ["waveform", "multitrack"] {
            a.ok("presentation.set_mode", json!({"mode":mode})).await;
            captures.push(a.capture(&output.join(format!("{mode}.png"))).await);
            a.ok("presentation.panels.toggle", json!({})).await;
            captures.push(a.capture(&output.join(format!("{mode}-hidden.png"))).await);
            a.ok("presentation.panels.toggle", json!({})).await;
        }
    } else {
        assert_eq!(a.raw("capture.png", json!({})).await["status"], "error");
    }
    let summary = json!({"documents":3,"tracks":4,"clips":5,"crossfades":1,"mix_frames":576000,"mix_channels":2,"clipboard_converted_frames":32000,"project_reopen_without_sources":true,"mixed_pcm":pcm,"captures":captures,"audition":audition});
    fs::write(
        output.join("workflow.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    summary
}
