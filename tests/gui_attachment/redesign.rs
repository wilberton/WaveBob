//! Opt-in actual native captures, with input injected only by the QA executable.
use super::*;

#[test]
#[ignore = "requires native GPU effect viewports"]
fn native_compact_effect_windows_defaults_narrow_bypass_and_reopen() {
    for kind in [
        "reverb",
        "fast_distortion",
        "soft_clipper",
        "parametric_eq",
        "delay",
        "chorus",
        "limiter",
        "balance",
        "stereo_enhancer",
        "stereo_shaper",
        "transient_processor",
    ] {
        let r = Review::open(kind);
        r.capture("compact-default");
        let (scale, _, _) = r.layout.get();
        assert!(
            (scale - 0.75).abs() < 0.005,
            "{kind} must open at the compact default size"
        );
        r.finish();
    }
}

struct Review {
    runtime: tempfile::TempDir,
    _gui: Gui,
    client: Client,
    owner: Value,
    effect: Value,
    editor: Value,
    prefix: String,
    sequence: u64,
    layout: std::cell::Cell<(f64, f64, f64)>,
}
impl Review {
    fn open(kind: &str) -> Self {
        let runtime = tempfile::tempdir().unwrap();
        let mut gui = Gui::start(runtime.path(), kind);
        let deadline = Instant::now() + Duration::from_secs(30);
        let instance = loop {
            if let Some(i) = ipc::discover(runtime.path())
                .unwrap()
                .into_iter()
                .find(|i| i.name == kind)
            {
                break i;
            }
            assert!(gui.0.try_wait().unwrap().is_none());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        let client = Client::attach(runtime.path(), &instance.session_id).unwrap();
        let source = runtime.path().join("Native design review.wav");
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
        for i in 0..48000 {
            let x = (i as f32 * 997.0 * std::f32::consts::TAU / 48000.0).sin()
                * 0.5
                * (0.15 + 0.85 * (i % 12000) as f32 / 12000.0);
            writer.write_sample(x).unwrap();
            writer.write_sample(x * 0.731).unwrap();
        }
        writer.finalize().unwrap();
        let job = success(request(&client, "documents.open", json!({"path":source})));
        let document = loop {
            let j = success(request(&client, "jobs.get", json!({"job_id":job["id"]})));
            if j["status"] != "running" {
                assert_eq!(j["status"], "succeeded");
                break j["result"].clone();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let owner = json!({"kind":"document","id":document["id"]});
        let state = success(request(&client, "processing.get", json!({"owner":owner})));
        let state = success(request(
            &client,
            "processing.edit",
            json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":kind}}),
        ));
        let effect = state["committed"]["effects"][0]["id"].clone();
        let opened = success(request(
            &client,
            "effects.editor.open",
            json!({"owner":owner,"instance_id":effect}),
        ));
        let editor = opened["editor_windows"][0]["id"].clone();
        Self {
            runtime,
            _gui: gui,
            client,
            owner,
            effect,
            editor,
            prefix: kind.into(),
            sequence: 0,
            layout: std::cell::Cell::new((0.75, 0.0, 0.0)),
        }
    }
    fn edit(&self, id: &str, value: Value) {
        let state = success(request(
            &self.client,
            "processing.get",
            json!({"owner":self.owner}),
        ));
        success(request(
            &self.client,
            "processing.edit",
            json!({"owner":self.owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":self.effect,"parameter_id":id,"value":value}}),
        ));
    }
    fn capture(&self, name: &str) {
        let c = success(request(
            &self.client,
            "capture.png",
            json!({"editor_id":self.editor,"timeout_ms":5000,"path":self.runtime.path().join("review.png"),"overwrite":true}),
        ));
        assert_eq!(c["metadata"]["owner"], self.owner);
        assert_eq!(c["metadata"]["instance_id"], self.effect);
        let dpi = c["metadata"]["scale"].as_f64().unwrap();
        let w = c["metadata"]["width"].as_f64().unwrap() / dpi;
        let h = c["metadata"]["height"].as_f64().unwrap() / dpi;
        let scale = (w / 850.0).min(h / 530.0).max(0.7);
        self.layout.set((
            scale,
            (w.max(595.0) - 850.0 * scale) * 0.5,
            (h.max(371.0) - 530.0 * scale) * 0.5,
        ));
        pixels(Path::new(c["path"].as_str().unwrap()));
        retain(
            Path::new(c["path"].as_str().unwrap()),
            &format!("{}-{name}.png", self.prefix),
        );
        if let Some(dir) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
            fs::write(
                Path::new(&dir).join(format!("{}-{name}.json", self.prefix)),
                serde_json::to_vec_pretty(&c["metadata"]).unwrap(),
            )
            .unwrap();
        }
    }
    fn click(&mut self, x: f64, y: f64) {
        let (scale, ox, oy) = self.layout.get();
        let (x, y) = (ox + x * scale, oy + y * scale);
        let path = std::env::var_os("WAVEBOB_REVIEW_INPUT_FILE")
            .expect("Use the QA gui-review executable for page captures");
        self.sequence += 1;
        fs::write(&path,serde_json::to_vec(&json!({"editor_id":self.editor,"pointer":{"x":x,"y":y,"click":true,"sequence":self.sequence}})).unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut first = None;
        loop {
            if let Ok(bytes) = fs::read(Path::new(&path).with_extension("ack.json"))
                && let Ok(ack) = serde_json::from_slice::<Value>(&bytes)
                && ack["editor_id"] == self.editor
                && ack["pointer_sequence"].as_u64() == Some(self.sequence)
                && let Some(frame) = ack["frame"].as_u64()
            {
                let start = *first.get_or_insert(frame);
                if frame > start {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "QA click did not complete two native frames"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        // Let popup fade-in settle before a caller requests the rendered capture.
        std::thread::sleep(Duration::from_millis(200));
        fs::write(
            path,
            serde_json::to_vec(&json!({"editor_id":self.editor})).unwrap(),
        )
        .unwrap();
    }
    fn finish(self) {
        capture_narrow_bypass(
            &self.client,
            &self.owner,
            &self.effect,
            &self.editor,
            self.runtime.path(),
            &self.prefix,
            [595.0, 371.0],
        );
        let opened = success(request(
            &self.client,
            "effects.editor.open",
            json!({"owner":self.owner,"instance_id":self.effect}),
        ));
        assert_eq!(opened["editor_windows"][0]["id"], self.editor);
        success(request(
            &self.client,
            "effects.editor.close",
            json!({"owner":self.owner,"instance_id":self.effect}),
        ));
        assert_eq!(
            request(
                &self.client,
                "capture.png",
                json!({"editor_id":self.editor})
            )["error"]["code"],
            "not_found"
        );
        reopen_and_capture(
            &self.client,
            &self.owner,
            &self.effect,
            self.runtime.path(),
            &format!("{}-reopened", self.prefix),
        );
        success(request(
            &self.client,
            "presentation.quit",
            json!({"discard_changes":true}),
        ));
    }
}
#[test]
#[ignore = "requires native GPU viewports and QA pointer injector"]
fn native_delay_redesign_pages() {
    let mut r = Review::open("delay");
    r.capture("default");
    for (id, value) in [
        ("type", 2),
        ("time", 144),
        ("feedback", 2160),
        ("wet", 2100),
        ("offset", 3700),
    ] {
        r.edit(id, json!(value));
    }
    r.capture("ping-pong");
    for (x, name) in [
        (370.0, "modulation"),
        (479.0, "diffusion"),
        (586.0, "feedback-shaper"),
    ] {
        r.click(x, 75.0);
        r.capture(name);
    }
    r.click(262.0, 75.0);
    r.click(277.0, 323.0);
    r.capture("timing-filter");
    r.click(277.0, 323.0);
    r.edit("type", json!(3));
    r.capture("off");
    r.edit("type", json!(2));
    success(request(
        &r.client,
        "effects.editor.geometry",
        json!({"editor_id":r.editor,"geometry":{"width":595.0,"height":371.0,"x":80.0,"y":80.0}}),
    ));
    r.capture("narrow-main");
    for (x, name) in [
        (370.0, "narrow-modulation"),
        (479.0, "narrow-diffusion"),
        (586.0, "narrow-feedback-shaper"),
    ] {
        r.click(x, 75.0);
        r.capture(name);
    }
    r.click(262.0, 75.0);
    r.click(277.0, 323.0);
    r.capture("narrow-timing-filter");
    r.click(277.0, 323.0);
    r.finish();
}

#[test]
#[ignore = "requires native GPU effect viewports"]
fn native_chorus_redesign_states() {
    let r = Review::open("chorus");
    r.capture("default");
    for (id, value) in [
        ("wave1", 342),
        ("wave2", 683),
        ("wave3", 1024),
        ("speed1", 1414),
        ("speed2", 1658),
        ("speed3", 1871),
        ("stereo", 512),
        ("depth", 2500),
        ("cutoff", 3280),
    ] {
        r.edit(id, json!(value));
    }
    r.capture("mixed-waves");
    for (id, value) in [
        ("cross", 100),
        ("wet", 1024),
        ("delay", 1024),
        ("depth", 5000),
        ("stereo", 1024),
        ("cutoff", 5000),
    ] {
        r.edit(id, json!(value));
    }
    r.capture("endpoints-wet-only-lows");
    r.finish();
}

#[test]
#[ignore = "requires native GPU viewports, QA input and manual audio output"]
fn native_limiter_redesign_pages_history_and_gate() {
    assert!(std::env::var_os("WAVEBOB_REVIEW_MANUAL_OUTPUT").is_some());
    let mut r = Review::open("limiter");
    r.capture("empty-paused");
    success(request(
        &r.client,
        "transport.loop.set",
        json!({"enabled":true}),
    ));
    success(request(
        &r.client,
        "transport.play",
        json!({"document_id":r.owner["id"]}),
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let t = success(request(&r.client, "transport.get", json!({})));
        if t["effect_meters"].as_array().unwrap().iter().any(|m| {
            m["instance_id"] == r.effect && m["columns"].as_array().is_some_and(|c| c.len() >= 8)
        }) {
            break;
        }
        assert!(Instant::now() < deadline, "{t}");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(2600));
    r.capture("limit-live");
    for (id, value) in [
        ("gain", 1300),
        ("ceil", 850),
        ("c_thres", 500),
        ("c_ratio", 700),
        ("c_knee", 500),
    ] {
        r.edit(id, json!(value));
    }
    r.click(286.0, 75.0);
    r.capture("comp-live");
    let (w, h, data) = pixels(&r.runtime.path().join("review.png"));
    let scale = w as f64 / 850.0;
    let stride = data.len() / (w * h) as usize;
    let offset =
        ((70.0 * scale).round() as usize * w as usize + (260.0 * scale).round() as usize) * stride;
    assert!(
        data[offset] < 60 && data[offset + 1] > 80 && data[offset + 2] > 200,
        "The native Comp tab must receive the injected click"
    );
    r.click(367.0, 75.0);
    r.capture("comp-gate-popup");
    r.click(700.0, 300.0);
    r.click(205.0, 317.0);
    r.click(440.0, 317.0);
    r.capture("layers-input-markers-off");
    success(request(&r.client, "transport.pause", json!({})));
    r.capture("history-paused");
    let meter = success(request(&r.client, "transport.get", json!({})));
    if let Some(dir) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        fs::write(
            Path::new(&dir).join("limiter-actual-meter.json"),
            serde_json::to_vec_pretty(&meter).unwrap(),
        )
        .unwrap();
    }
    success(request(
        &r.client,
        "effects.editor.geometry",
        json!({"editor_id":r.editor,"geometry":{"width":595.0,"height":371.0,"x":80.0,"y":80.0}}),
    ));
    r.capture("narrow-comp");
    r.click(367.0, 75.0);
    r.capture("narrow-gate-popup");
    r.click(700.0, 300.0);
    r.click(210.0, 75.0);
    r.capture("narrow-limit");
    r.finish();
}

#[test]
#[ignore = "requires native GPU effect viewports"]
fn native_balance_redesign_folding_and_mute() {
    let r = Review::open("balance");
    r.capture("centred");
    for (value, name) in [
        (-64, "half-left-fold"),
        (-128, "left-fold"),
        (128, "right-fold"),
    ] {
        r.edit("balance", json!(value));
        r.capture(name);
    }
    r.edit("volume", json!(0));
    r.capture("mute");
    r.edit("volume", json!(320));
    r.capture("maximum-volume");
    r.finish();
}

#[test]
#[ignore = "requires native GPU effect viewports"]
fn native_enhancer_redesign_field_and_selectors() {
    let r = Review::open("stereo_enhancer");
    r.capture("default");
    for (id, value) in [("separation", -32), ("phase", 51)] {
        r.edit(id, json!(value));
    }
    r.capture("separated-right-offset");
    for (id, value) in [("phase", -512), ("placement", 0), ("invert", 1)] {
        r.edit(id, json!(value));
    }
    r.capture("left-offset-pre-inverted");
    for (id, value) in [
        ("separation", 96),
        ("pan", 128),
        ("invert", 2),
        ("placement", 1),
    ] {
        r.edit(id, json!(value));
    }
    r.capture("merged-pan-right-inverted");
    r.edit("volume", json!(0));
    r.capture("mute");
    for (id, value) in [
        ("volume", 320),
        ("pan", 0),
        ("separation", -96),
        ("phase", 512),
        ("invert", 0),
    ] {
        r.edit(id, json!(value));
    }
    r.capture("wide-maximum-volume");
    r.finish();
}

#[test]
#[ignore = "requires native GPU effect viewports"]
fn native_stereo_shaper_redesign_signed_matrix_and_effects() {
    let r = Review::open("stereo_shaper");
    r.capture("unity");
    for (id, v) in [("left", 0), ("right", 0), ("rl", 12800), ("lr", 12800)] {
        r.edit(id, json!(v));
    }
    r.capture("swap");
    for (id, v) in [
        ("left", 12800),
        ("right", 12800),
        ("rl", -3200),
        ("lr", 3200),
        ("delay", 411),
        ("phase", -1487),
    ] {
        r.edit(id, json!(v));
    }
    r.capture("crossfeed-right-delay-left-phase");
    for (id, v) in [
        ("position", 0),
        ("delay", -4096),
        ("phase", 4096),
        ("left", -25600),
        ("lr", 25600),
    ] {
        r.edit(id, json!(v));
    }
    r.capture("pre-inverted-endpoints");
    r.edit("position", json!(-1));
    r.capture("effects-off");
    for id in ["left", "right", "rl", "lr"] {
        r.edit(id, json!(0));
    }
    r.capture("silent-matrix");
    r.finish();
}

#[test]
#[ignore = "requires native GPU effect viewports"]
fn native_transient_redesign_shaping_and_processing() {
    let r = Review::open("transient_processor");
    r.capture("default");
    for (id, v) in [
        ("attack", 155),
        ("release", 44),
        ("drive", 40),
        ("split", 160),
        ("attack_shape", 0),
        ("release_shape", 2),
    ] {
        r.edit(id, json!(v));
    }
    r.capture("boost-attack-cut-release");
    for (id, v) in [
        ("attack", 0),
        ("release", 160),
        ("attack_shape", 2),
        ("release_shape", 0),
        ("balance", 0),
        ("gain", 0),
        ("split", 0),
    ] {
        r.edit(id, json!(v));
    }
    r.capture("low-band-opposite-shapes");
    for (id, v) in [
        ("attack", 160),
        ("release", 0),
        ("drive", 160),
        ("gain", 160),
        ("balance", 160),
        ("attack_shape", 1),
        ("release_shape", 1),
    ] {
        r.edit(id, json!(v));
    }
    r.capture("high-band-extremes");
    r.edit("on", json!(0));
    r.capture("processing-off");
    r.edit("on", json!(1));
    r.capture("processing-on");
    r.finish();
}
