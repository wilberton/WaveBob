//! QA-only native renderer for deterministic pointer/focus captures of the real UI.
//! This input injector is absent from the shipped application and command seam.
#[path = "../src/app.rs"]
mod app;
use eframe::egui;
struct Review {
    app: app::DesktopApp,
    input: std::path::PathBuf,
    focus_sent: bool,
    drop_sequence: Option<u64>,
    pointer_sequence: Option<u64>,
    input_frame: u64,
    _manual_clock: Option<ManualClock>,
}
struct ManualClock {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl ManualClock {
    fn start(mut output: wavebob::transport::ManualOutput) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut frames = [[0.0; 2]; 800];
            while !stopped.load(std::sync::atomic::Ordering::Relaxed) {
                output.render_into(&mut frames);
                std::thread::sleep(std::time::Duration::from_micros(16667));
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for ManualClock {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[derive(Debug)]
struct ReviewFile(std::path::PathBuf);
impl egui::DroppedFile for ReviewFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        panic!("The shared background importer must read the audio file")
    }
}
impl eframe::App for Review {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
        if let Ok(bytes) = std::fs::read(&self.input)
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            if let Some(editor) = value["editor_id"].as_str()
                && input.viewport_id != egui::ViewportId::from_hash_of(editor)
            {
                return;
            }
            input.focused = true;
            input.events.clear();
            input.hovered_files = value["hover_files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|path| path.as_str())
                .map(|path| egui::HoveredFile {
                    path: Some(path.into()),
                    ..Default::default()
                })
                .collect();
            if let Some(sequence) = value["drop_sequence"].as_u64()
                && self.drop_sequence != Some(sequence)
            {
                self.drop_sequence = Some(sequence);
                input.dropped_files.extend(
                    value["drop_files"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|path| path.as_str())
                        .map(|path| {
                            std::sync::Arc::new(ReviewFile(path.into())) as egui::DroppedFileHandle
                        }),
                );
            }
            if let Some(pointer) = value["pointer"].as_object() {
                let position = egui::pos2(
                    pointer["x"].as_f64().unwrap() as f32,
                    pointer["y"].as_f64().unwrap() as f32,
                );
                input.events.push(egui::Event::PointerMoved(position));
                if let Some(sequence) = pointer["sequence"].as_u64()
                    && self.pointer_sequence != Some(sequence)
                {
                    self.pointer_sequence = Some(sequence);
                    let buttons = if pointer["click"] == true {
                        vec![true, false]
                    } else {
                        vec![pointer["pressed"].as_bool().unwrap()]
                    };
                    for pressed in buttons {
                        input.events.push(egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
            } else if let Some(hover) = value["hover"].as_array() {
                input.events.push(egui::Event::PointerMoved(egui::pos2(
                    hover[0].as_f64().unwrap() as f32,
                    hover[1].as_f64().unwrap() as f32,
                )));
                self.focus_sent = false;
            } else {
                input.events.push(egui::Event::PointerGone);
                if value["focus_tab"] == true && !self.focus_sent {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                    self.focus_sent = true;
                }
            }
            self.input_frame += 1;
            let _=std::fs::write(self.input.with_extension("ack.json"),serde_json::to_vec(&serde_json::json!({"editor_id":value["editor_id"],"pointer_sequence":self.pointer_sequence,"frame":self.input_frame})).unwrap());
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        if let Ok(bytes) = std::fs::read(&self.input)
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(editor) = value["editor_id"].as_str()
        {
            ui.ctx()
                .request_repaint_of(egui::ViewportId::from_hash_of(editor));
        }
        self.app.ui(ui, frame);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut directory = None;
    let mut preferences = None;
    let mut name = "GUI review".to_owned();
    let mut size = [1440.0, 900.0];
    let mut scale = 1.0;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--hidden" => {}
            "--runtime-dir" => directory = args.next(),
            "--preferences" => preferences = args.next(),
            "--name" => name = args.next().unwrap(),
            "--window-size" => {
                let value = args.next().unwrap();
                let (w, h) = value.split_once('x').unwrap();
                size = [w.parse()?, h.parse()?];
            }
            "--ui-scale" => scale = args.next().unwrap().parse()?,
            _ => return Err("Unsupported review argument".into()),
        }
    }
    let directory = std::path::PathBuf::from(directory.ok_or("--runtime-dir required")?);
    let preferences = std::path::PathBuf::from(preferences.ok_or("--preferences required")?);
    let input = std::env::var_os("WAVEBOB_REVIEW_INPUT_FILE")
        .ok_or("WAVEBOB_REVIEW_INPUT_FILE required")?
        .into();
    eframe::run_native(
        "WaveBob UI review",
        eframe::NativeOptions {
            clipboard_shortcuts: false,
            viewport: egui::ViewportBuilder::default()
                .with_visible(false)
                .with_drag_and_drop(true)
                .with_inner_size(size),
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_zoom_factor(scale);
            let (app, manual_output) = if std::env::var_os("WAVEBOB_REVIEW_MANUAL_OUTPUT").is_some()
            {
                let (session, output) = wavebob::Session::with_manual_output(48000)
                    .map_err(|e| std::io::Error::other(e.message))?;
                let host = wavebob::desktop::DesktopHost::with_session(session);
                (
                    app::WaveBob::with_host(cc, &directory, name, preferences, host)?,
                    Some(output),
                )
            } else {
                (app::WaveBob::new(cc, &directory, name, preferences)?, None)
            };
            Ok(Box::new(Review {
                app: app::DesktopApp::new(app),
                input,
                focus_sent: false,
                drop_sequence: None,
                pointer_sequence: None,
                input_frame: 0,
                _manual_clock: manual_output.map(ManualClock::start),
            }))
        }),
    )?;
    Ok(())
}
