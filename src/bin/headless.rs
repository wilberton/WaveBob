use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if let Some(argument) = args.next() {
        if (argument == "--help" || argument == "-h") && args.next().is_none() {
            println!(
                "wavebob-headless\n\nReads newline-delimited JSON requests from stdin and writes responses to stdout.\nStart discovery with: {{\"id\":\"help\",\"command\":\"session.capabilities\"}}"
            );
            return ExitCode::SUCCESS;
        }
        eprintln!("Unknown arguments. Run wavebob-headless --help for usage.");
        return ExitCode::from(2);
    }
    match wavebob::protocol::run(
        &mut wavebob::Session::default(),
        io::stdin().lock(),
        io::stdout().lock(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Headless I/O error: {error}");
            ExitCode::FAILURE
        }
    }
}
