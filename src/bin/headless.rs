use std::io;
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut directory = wavebob::ipc::default_directory()?;
    let mut attach = None;
    let mut list = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help" | "-h") => {
                println!(
                    "wavebob-headless [--list-instances | --attach SESSION_UUID] [--runtime-dir DIRECTORY]\n\nReads newline-delimited JSON from stdin and flushes responses to stdout.\nDefault: an independent GUI-free session. --attach targets an explicit running GUI.\nDiscover commands with: {{\"id\":\"help\",\"command\":\"session.capabilities\"}}"
                );
                return Ok(());
            }
            Some("--runtime-dir") => {
                directory = args.next().ok_or("Missing runtime directory")?.into()
            }
            Some("--attach") => {
                attach = Some(
                    args.next()
                        .ok_or("Missing session UUID")?
                        .into_string()
                        .map_err(|_| "Session UUID must be UTF-8")?,
                )
            }
            Some("--list-instances") => list = true,
            _ => return Err("Invalid arguments; run wavebob-headless --help".into()),
        }
    }
    if list && attach.is_some() {
        return Err("Choose --list-instances or --attach".into());
    }
    if list {
        serde_json::to_writer(io::stdout().lock(), &wavebob::ipc::discover(&directory)?)?;
        println!();
    } else if let Some(id) = attach {
        wavebob::ipc::Client::attach(&directory, &id)?
            .run(io::stdin().lock(), io::stdout().lock())?;
    } else {
        wavebob::protocol::run(
            &mut wavebob::Session::default(),
            io::stdin().lock(),
            io::stdout().lock(),
        )?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
            {
                return ExitCode::SUCCESS;
            }
            eprintln!("{error}");
            if error.downcast_ref::<io::Error>().is_some() {
                ExitCode::FAILURE
            } else {
                ExitCode::from(2)
            }
        }
    }
}
