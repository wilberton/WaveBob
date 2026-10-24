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
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
        if let Ok(bytes) = std::fs::read(&self.input)
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
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
            if let Some(hover) = value["hover"].as_array() {
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
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
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
            Ok(Box::new(Review {
                app: app::DesktopApp::new(app::WaveBob::new(cc, &directory, name, preferences)?),
                input,
                focus_sent: false,
                drop_sequence: None,
            }))
        }),
    )?;
    Ok(())
}
