#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

fn main() -> eframe::Result {
    eframe::run_native(
        "WaveBob — UI concept",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 900.0])
                .with_min_inner_size([1000.0, 650.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(app::WaveBob::new(cc)))),
    )
}
