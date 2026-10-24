# WaveBob

A native Rust + egui audio editor for Windows, macOS and Linux.
The first seven implementation milestones provide shared session commands, a
GUI-free JSON runner, attachment to named GUI instances, revision-fenced PNG
screenshots, stdio MCP support, persistent panel layout and status-bar help.
WAV and MP3 files open as background jobs and display real mono/stereo waveforms.
The [filesystem browser](docs/browser.md) supports independent WAV/MP3 audition.
Selection/whole-sample [gain, fades, reverse and reverb](docs/editing.md) support undo/redo.
[Cut/copy/paste/delete/trim](docs/structural-edits.md) share history, with rate/channel conversion.
[WAV/MP3 save and export](docs/export.md) use atomic jobs and protect dirty work on close.
[Editable tracks and pinned clips](docs/arrangement.md) support drag placement,
track naming/order, precise timeline selection and shared undo/redo.
Clip edges and exact source bounds trim individual instances; the effects panel
processes one selected clip without altering its source or other instances.
[Multitrack playback and offline mix inspection](docs/mixing.md) share resampling,
track/master gain, mute/solo and mono pan/stereo balance, with output metering.
Two overlapping clips on a track now derive [automatic equal-power crossfades](docs/crossfades.md),
including contained clips; third simultaneous overlaps are rejected.
[WAV/MP3 arrangement export](docs/mix-export.md) renders an immutable mix snapshot,
including selected time ranges, processing, track controls and silent gaps.
[Portable projects](docs/projects.md) preserve edited audio, pinned sources and
the arrangement with managed lossless assets, independent of original files.
Native sample playback supports pause, seek and selection/whole-sample loops;
see [transport controls and commands](docs/transport.md).
Precise cursor/selection controls and overview/mouse/trackpad zoom and pan are
available; see [waveform controls](docs/waveform.md).

## Run

Install stable Rust using [rustup](https://rustup.rs/), then:

```sh
cargo run --locked
```

```sh
cargo run --locked --no-default-features --bin wavebob-headless
cargo run --locked --no-default-features --bin wavebob-mcp
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

Windows needs the MSVC C++ build tools and Windows SDK. macOS needs Xcode command
line tools. For Debian/Ubuntu Linux, install:

```sh
sudo apt-get install build-essential pkg-config libx11-dev libxi-dev libxrandr-dev libxcursor-dev libxinerama-dev libwayland-dev libxkbcommon-dev libgl1-mesa-dev libasound2-dev
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
revision/error semantics and GUI-free build instructions. The
[ticket 02 verification record](docs/verification/ticket02/README.md) includes
captures from the running release app.
See [MCP setup and tools](docs/mcp.md) to configure an AI client and retrieve
screenshots as image content. The [MCP verification record](docs/verification/ticket03/README.md)
includes images retrieved from the release app.
See [panels and status-bar help](docs/layout.md) and the
[ticket 04 captures](docs/verification/ticket04/README.md) for layout controls.
The [mockup comparison](docs/verification/ticket22/README.md) records refined
waveform and arrangement captures at wide, compact, narrow and 150% scale.

## This milestone

- Native window, dark theme, real document selection and a core-backed loop button.
- Typed session commands, snapshots, schema discovery and revision checks.
- Newline-delimited JSON runner with structured responses and recoverable errors.
- Explicit GUI instance discovery/attachment, mode commands and actual PNG captures.
- Stdio MCP tools generated from the command registry, including PNG image delivery.
- Background PCM/float WAV and CBR/VBR MP3 import, exact sample queries and cached waveform peaks.
- Native Open action, multiple documents and real overview/channel waveforms.
- Resizable/collapsible panels, a remembered library/browser split and bottom status help.
- Filesystem navigation and independent preview, plus undoable gain/fades/reverse.
- Native sample playback with requested/applied position and recoverable device errors.
- Structural editing and an internal audio clipboard with deterministic resampling.
- Bounded, deterministic reverb with an explicit truncated-tail policy.
- WAV/MP3 sample save/export, exact saved revisions and guarded close/quit.
- Editable tracks, individual clip processing, automatic crossfades and mix export.
- Portable project save/reopen with managed lossless audio assets.
- The generated design sheet is a visual proposal, not a screenshot of the shell.

The approved [editor specification](.scratch/wavebob/spec.md) and
[22 implementation tickets](.scratch/wavebob/README.md) are published in the local
Markdown tracker. Each ticket has acceptance criteria, explicit blockers and its
current execution status. Tickets 01–19 and 22 are complete; derive subsequent
availability from the individual tickets. The tracker conventions are in
[docs/agents/issue-tracker.md](docs/agents/issue-tracker.md).

Version 1 includes WAV and MP3 import and export, the shared command interface,
headless/MCP access, command-driven GUI screenshots, and both editing modes.

## Visual reference

Match the generated [WaveBob UI mockups](docs/ui-concept-01.png), with the status
bar, hover help and resizable/collapsible panels defined by the spec. The
[design brief](docs/design-brief.md) retains the original interaction context.
Artcraft is architectural background rather than the visual target. The initial
shell bundles Inter with its OFL license; helper libraries are optional implementation aids.
