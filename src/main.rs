#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut directory = wavebob::ipc::default_directory()?;
    let mut name = String::from("WaveBob");
    let mut hidden = false;
    let mut preferences =
        std::env::var_os("WAVEBOB_PREFERENCES_PATH").map(std::path::PathBuf::from);
    let mut window_size = [1440.0, 900.0];
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
            Some("--preferences") => {
                preferences = Some(args.next().ok_or("Missing preferences path")?.into())
            }
            Some("--window-size") => {
                let size = args
                    .next()
                    .ok_or("Missing WIDTHxHEIGHT")?
                    .into_string()
                    .map_err(|_| "Window size must be UTF-8")?;
                let (width, height) = size.split_once('x').ok_or("Expected WIDTHxHEIGHT")?;
                window_size = [width.parse::<u32>()? as f32, height.parse::<u32>()? as f32];
                if !(800.0..=16384.0).contains(&window_size[0])
                    || !(520.0..=16384.0).contains(&window_size[1])
                {
                    return Err(
                        "Window size must be at least 800x520 and at most 16384x16384".into(),
                    );
                }
            }
            _ => {
                return Err(
                    "Usage: wavebob [--name NAME] [--runtime-dir DIRECTORY] [--hidden] [--preferences FILE] [--window-size WIDTHxHEIGHT]".into(),
                );
            }
        }
    }
    let title = format!("WaveBob — {name}");
    let preferences = match preferences {
        Some(path) => path,
        None => directories::ProjectDirs::from("org", "WaveBob", "WaveBob")
            .ok_or("User configuration directory is unavailable; pass --preferences FILE")?
            .config_dir().join("layout.json"),
    };
    eframe::run_native(
        &title,
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_visible(!hidden)
                .with_inner_size(window_size)
                .with_min_inner_size([800.0, 520.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(app::WaveBob::new(
                cc,
                &directory,
                name,
                preferences,
            )?))
        }),
    )?;
    Ok(())
}
