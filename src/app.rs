use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

const BLUE: Color32 = Color32::from_rgb(85, 150, 245);
const FILES: [&str; 3] = ["Rain ambience.wav", "Footsteps.wav", "Piano texture.wav"];

#[derive(Default)]
pub struct WaveBob {
    multitrack: bool,
    selected: usize,
    looping: bool,
}

impl WaveBob {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(20, 21, 24);
        visuals.window_fill = visuals.panel_fill;
        visuals.selection.bg_fill = Color32::from_rgb(35, 65, 105);
        visuals.selection.stroke = Stroke::new(1.0, BLUE);
        cc.egui_ctx.set_visuals(visuals);
        Self::default()
    }
}

impl eframe::App for WaveBob {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("WaveBob");
                ui.separator();
                ui.label("UI CONCEPT / DEMO DATA");
                ui.add_space(30.0);
                ui.selectable_value(&mut self.multitrack, false, "Waveform");
                ui.selectable_value(&mut self.multitrack, true, "Multitrack");
            });
        });
        egui::Panel::bottom("transport").show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label("00:04.250   /   00:24.000");
                ui.horizontal_centered(|ui| {
                    ui.add_enabled(false, egui::Button::new("|<  Start"));
                    ui.add_enabled(false, egui::Button::new("Play"));
                    ui.toggle_value(&mut self.looping, "Loop");
                    ui.label("Space: play/pause   •   Enter or ,: start   •   /: loop");
                });
                ui.small("Transport is a layout placeholder. Audio playback is not implemented.");
            });
        });
        egui::Panel::left("library").default_size(235.0).show(ui, |ui| {
            ui.heading("Open files");
            for (index, name) in FILES.iter().enumerate() {
                if ui.selectable_label(self.selected == index, *name).clicked() {
                    self.selected = index;
                    self.multitrack = false;
                }
            }
            ui.add_space(35.0);
            ui.separator();
            ui.heading("File browser");
            ui.small("Samples / Field recordings");
            for name in ["Ambiences/", "Foley/", "Music/", "Rain ambience.wav", "Wind.wav"] {
                ui.label(name);
            }
            ui.add_space(20.0);
            ui.add_enabled(false, egui::Button::new("Audition"));
            ui.small("Filesystem browsing and auditioning are planned.");
        });
        egui::Panel::right("effects").default_size(225.0).show(ui, |ui| {
            ui.heading("Edit & effects");
            ui.label(if self.multitrack { "Target: selected clip" } else { "Target: selection / whole sample" });
            ui.separator();
            ui.add_enabled_ui(false, |ui| {
                ui.label("Gain");
                let mut gain = -3.0_f32;
                ui.add(egui::Slider::new(&mut gain, -24.0..=12.0).suffix(" dB"));
                for name in ["Fade in", "Fade out", "Reverse", "Reverb", "Apply"] {
                    ui.button(name).on_hover_text("Planned audio operation");
                }
            });
            ui.small("Effects are placeholders pending design approval.");
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.multitrack {
                ui.heading("Untitled arrangement");
                ui.small("SECONDS     0           4           8           12           16           20           24");
                for (index, name) in ["Ambience", "Footsteps", "Texture", "Track 4"].iter().enumerate() {
                    ui.separator();
                    ui.label(*name);
                    ui.horizontal(|ui| {
                        ui.add_enabled(false, egui::Button::new("M"));
                        ui.add_enabled(false, egui::Button::new("S"));
                        ui.small("Volume 0 dB     Pan C");
                    });
                    if index < 3 {
                        waveform(ui, 90.0, index, false);
                    } else {
                        ui.label("Drag audio here — planned");
                    }
                }
            } else {
                ui.heading(FILES[self.selected]);
                ui.small("48 kHz  /  Stereo  /  24-bit  /  24.000 s  — synthetic preview");
                ui.add_space(18.0);
                ui.label("OVERVIEW");
                waveform(ui, 65.0, self.selected, true);
                ui.add_space(18.0);
                ui.small("00:04            00:06            00:08            00:10            00:12");
                let height = ((ui.available_height() - 55.0) / 2.0).max(80.0);
                waveform(ui, height, self.selected, false);
                waveform(ui, height, self.selected + 1, false);
                ui.label("Selection  00:06.000 — 00:09.500    |    3.500 s");
                ui.small("Static preview: selection, overview handles, zoom and pan are planned.");
            }
        });
    }
}

fn waveform(ui: &mut egui::Ui, height: f32, seed: usize, overview: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, Color32::from_rgb(14, 16, 19));
    let middle = rect.center().y;
    painter.line_segment([Pos2::new(rect.left(), middle), Pos2::new(rect.right(), middle)], Stroke::new(1.0, Color32::from_gray(50)));
    for column in 0..(rect.width() as usize / 3) {
        let x = rect.left() + column as f32 * 3.0;
        let phase = column as f32 * 0.31 + seed as f32 * 2.0;
        let amplitude = (phase.sin() * (phase * 0.17).cos()).abs() * height * 0.4 + 2.0;
        painter.line_segment([Pos2::new(x, middle - amplitude), Pos2::new(x, middle + amplitude)], Stroke::new(1.0, BLUE));
    }
    let selection = Rect::from_min_max(Pos2::new(rect.left() + rect.width() * 0.25, rect.top()), Pos2::new(rect.left() + rect.width() * 0.6, rect.bottom()));
    painter.rect_filled(selection, 0.0, Color32::from_rgba_unmultiplied(80, 145, 240, 35));
    if overview {
        for x in [selection.left(), selection.right()] {
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(3.0, BLUE));
        }
    }
}
