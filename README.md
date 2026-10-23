# WaveBob

A native Rust + egui audio editor for Windows, macOS and Linux.
The first implementation milestone provides a shared session command layer,
GUI-free JSON runner and a demo-data egui shell. Audio editing and playback are
not implemented yet.

## Run

Install stable Rust using [rustup](https://rustup.rs/), then:

```sh
cargo run --locked
```

```sh
cargo run --locked --no-default-features --bin wavebob-headless
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

Windows needs the MSVC C++ build tools and Windows SDK. macOS needs Xcode command
line tools. For Debian/Ubuntu Linux, install:

```sh
sudo apt-get install build-essential pkg-config libx11-dev libxi-dev libxrandr-dev libxcursor-dev libxinerama-dev libwayland-dev libxkbcommon-dev libgl1-mesa-dev
```

The renderer is eframe's OpenGL backend, with X11 and Wayland enabled for Linux.
The committed dependency lock is used by CI, which checks both GUI and headless
configurations on all three platforms. Native macOS/Linux CI requires a GitHub
remote with Actions enabled; the local repository does not have a remote yet.

For the workspace-local Rust installation created in this checkout, initialize
the following environment in PowerShell before running Cargo. These local tools
are ignored by Git; other checkouts can use a normal rustup installation.

```powershell
$env:RUSTUP_HOME = Join-Path $PWD '.tools\rustup'
$env:CARGO_HOME = Join-Path $PWD '.tools\cargo'
$env:PATH = "$(Join-Path $PWD '.tools\cargo\bin');$env:PATH"
```

See [the command protocol](docs/commands.md) for discovery, request examples,
revision/error semantics and GUI-free build instructions.

## This milestone

- Native window, dark theme, demo mode/file selection and a core-backed loop button.
- Typed session commands, snapshots, schema discovery and revision checks.
- Newline-delimited JSON runner with structured responses and recoverable errors.
- Synthetic waveform drawings and static multitrack lanes.
- Library, browser, effects and transport layout placeholders.
- No real file access, audio decoding, playback, editing, export or persistence.
- The generated design sheet is a visual proposal, not a screenshot of the shell.

The approved [editor specification](.scratch/wavebob/spec.md) and
[22 implementation tickets](.scratch/wavebob/README.md) are published in the local
Markdown tracker. Each ticket has acceptance criteria, explicit blockers and
`ready-for-agent` status. Ticket 01 is the initial unblocked ticket; publication
does not mean implementation is complete. The tracker conventions are in
[docs/agents/issue-tracker.md](docs/agents/issue-tracker.md).

Version 1 includes WAV and MP3 import and export, the shared command interface,
headless/MCP access, command-driven GUI screenshots, and both editing modes.

## Visual reference

Match the generated [WaveBob UI mockups](docs/ui-concept-01.png), with the status
bar, hover help and resizable/collapsible panels defined by the spec. The
[design brief](docs/design-brief.md) retains the original interaction context.
Artcraft is architectural background rather than the visual target. The initial
shell uses egui's bundled fonts; helper libraries are optional implementation aids.
