# WaveBob

A native Rust + egui audio editor for Windows, macOS and Linux.
This initial milestone contains build scaffolding, a demo-data UI shell and a
visual concept for discussion. It is not yet an audio editor.

## Run

Install stable Rust using [rustup](https://rustup.rs/), then:

```sh
cargo run
```

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

Windows needs the MSVC C++ build tools and Windows SDK. macOS needs Xcode command
line tools. For Debian/Ubuntu Linux, install:

```sh
sudo apt-get install build-essential pkg-config libx11-dev libxi-dev libxrandr-dev libxcursor-dev libxinerama-dev libwayland-dev libxkbcommon-dev libgl1-mesa-dev
```

The renderer is eframe's OpenGL backend, with X11 and Wayland enabled for Linux.
The CI workflow checks and builds all three desktop platforms. CI has not been
run as part of this initial scaffold. Rust is not installed in the authoring
environment, so local compilation, linting and formatting are also unverified.
Generate and commit `Cargo.lock` on the first successful Cargo build; the direct
eframe dependency is pinned, but transitive dependencies are not yet locked.

## This milestone

- Native window, dark theme, mode switch, open-file selection and loop toggle.
- Synthetic waveform drawings and static multitrack lanes.
- Library, browser, effects and transport layout placeholders.
- No real file access, audio decoding, playback, editing, export or persistence.
- The generated design sheet is a visual proposal, not a screenshot of the shell.

The approved [editor specification](.scratch/wavebob/spec.md) and
[21 implementation tickets](.scratch/wavebob/README.md) are published in the local
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
