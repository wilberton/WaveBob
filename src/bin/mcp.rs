use std::io;
use std::process::ExitCode;

use rmcp::{ServiceExt, transport::stdio};
use wavebob::{ipc, mcp::McpServer};

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut directory = ipc::default_directory()?;
    let mut attach = None;
    let mut list = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help" | "-h") => {
                println!(
                    "wavebob-mcp [--attach SESSION_UUID | --list-instances] [--runtime-dir DIRECTORY]\n\nStdio MCP server. Default: an independent GUI-free session.\n--attach fixes the target to an explicit running GUI session.\nUse --list-instances before configuring the server in your MCP client."
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
            _ => return Err("Invalid arguments; run wavebob-mcp --help".into()),
        }
    }
    if list && attach.is_some() {
        return Err("Choose --list-instances or --attach".into());
    }
    if list {
        serde_json::to_writer(io::stdout().lock(), &ipc::discover(&directory)?)?;
        println!();
        return Ok(());
    }
    let server = match attach {
        Some(id) => McpServer::attach(&directory, &id)?,
        None => McpServer::headless()?,
    };
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
