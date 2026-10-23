#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut directory = wavebob::ipc::default_directory()?;
    let mut name = String::from("WaveBob");
    let mut hidden = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--runtime-dir") => {
                directory = args.next().ok_or("Missing runtime directory")?.into()
            }
            Some("--name") => {
                name = args
                    .next()
                    .ok_or("Missing instance name")?
                    .into_string()
                    .map_err(|_| "Instance name must be UTF-8")?
            }
            Some("--hidden") => hidden = true,
            _ => {
                return Err(
                    "Usage: wavebob [--name NAME] [--runtime-dir DIRECTORY] [--hidden]".into(),
                );
            }
        }
    }
    let title = format!("WaveBob — {name}");
    eframe::run_native(
        &title,
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_visible(!hidden)
                .with_inner_size([1440.0, 900.0])
                .with_min_inner_size([1000.0, 650.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(app::WaveBob::new(cc, &directory, name)?))),
    )?;
    Ok(())
}
