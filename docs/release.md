# Native development releases

The local Windows x86-64 kit contains `bin/wavebob.exe`, `wavebob-headless.exe`,
`wavebob-mcp.exe` and the reproducible `wavebob-workflow.exe` example client.
Unzip the complete kit before running it. Launch the GUI by opening its executable.
The command-line runners reserve stdout for their documented protocol; use a
terminal or an MCP client. See [commands](commands.md), [MCP](mcp.md),
[automation](automation.md) and [projects](projects.md).

Windows 10/11 x86-64 needs a working OpenGL driver for the GUI and the Microsoft
Visual C++ x64 runtime (`VCRUNTIME140.dll`) plus Universal CRT. The development
machine already supplies these; a clean-machine installation remains unverified.
Headless/MCP builds omit windowing and audio-device features. WAV/MP3 codecs are
bundled; neither FFmpeg nor a user-installed LAME is needed. Native playback uses
the default audio device and reports failure/retry without preventing editing.

macOS builds need the system window/audio frameworks and an OpenGL-capable desktop.
Linux GUI builds use X11 or Wayland, OpenGL and ALSA. X11 also needs the
`libxkbcommon-x11` runtime (`libxkbcommon-x11-0` on Debian/Ubuntu); the documented
build package `libxkbcommon-x11-dev` installs it. The Debian/Ubuntu native build
packages are in the root README. Prepared release CI uses Windows x86-64,
macOS ARM64 and Linux x86-64 native runners; host identity is recorded by rustc.
No macOS/Linux binary or X11/Wayland execution is claimed locally. Cross-platform
Baseline three-platform CI/native verification is recorded under ticket21; latest
live-effects work follows the local Windows policy recorded below.

## Build and package

Install Rust stable with rustfmt, Clippy and rust-src, Python 3.11+, and the native
C build tools described in the README. From a clean committed checkout:

```sh
rustup component add rust-src
python scripts/package-release.py
```

This builds the GUI, then separately builds headless/MCP and the workflow client
with `--no-default-features`; the automation binaries retain no mandatory GUI/audio
adapter. It vendors the exact Cargo.lock dependency set, includes full sources,
original notices, Inter/OFL, Rust library sources/notices, codec source, application
sources and offline Cargo configuration. `--vendor PATH` can reuse an already
completed `cargo vendor --locked PATH` tree. Every vendor package is checked against
the lockfile's version/archive checksum. No archive is overwritten. The kit's
manifest records host, compiler, commit/dirty state, lock hash and executable hashes;
a SHA-256 sidecar covers the complete ZIP. Vendoring may fetch missing pinned
registry packages and Rust notices are downloaded from the compiler's exact commit.
The packaged toolchain file pins the recorded compiler version. Archive timestamps,
native compiler differences and linker output are not promised to be byte-reproducible.
If using this workspace's ignored local Rust installation, initialize it in
PowerShell before running Cargo:

```powershell
$env:RUSTUP_HOME = Join-Path $PWD '.tools\rustup'
$env:CARGO_HOME = Join-Path $PWD '.tools\cargo'
$env:PATH = "$(Join-Path $PWD '.tools\cargo\bin');$env:PATH"
```

Other checkouts can use a normal rustup installation.

Packaging requires a clean committed tree and rejects source/commit changes while
building. Registry timestamps predating ZIP's 1980 epoch are clamped for archival.
The complete ZIP is staged and published exclusively on the same local volume.

Verify the packaged executables in a new extraction directory with:

```sh
python scripts/smoke-release.py dist/WaveBob-VERSION-HOST.zip --out release-check
python scripts/smoke-release.py dist/WaveBob-VERSION-HOST.zip --out desktop-check --gui --audition
```

The first command is device-free; the second needs a desktop/audio device and
retrieves both modes through attached MCP before a guarded clean quit. The smoke
script checks archive/executable hashes and identical sample/mix output bytes
across the shipped direct example, headless runner and MCP server. To run just the
example from an extracted Windows kit:

```powershell
.\bin\wavebob-workflow.exe --adapter mcp --binary .\bin\wavebob-mcp.exe --out review
```

The GitHub Actions release workflow runs this packaging on all three native
hosts when manually dispatched, and uploads the ZIP plus checksum. Baseline successful workflow runs are recorded under ticket21. Signing, notarization,
installers, auto-updates and public publishing are deferred.

## Offline rebuilding and encoder relinking

Keep the whole source tree and notices with the statically linked executables.
From the kit's `source/` folder, with the recorded Rust version and native tools:

```sh
cargo build --locked --offline --release --bin wavebob
cargo build --locked --offline --release --no-default-features --bin wavebob-headless --bin wavebob-mcp --example workflow
```

The bundled `.cargo/config.toml` resolves dependencies from `vendor/`, without a
registry download. The LAME source is in `vendor/mp3lame-sys/lame-3.100/`, with its
bindings/build script one level above. To relink a modified encoder, copy the
entire `vendor/mp3lame-sys` directory to `modified-encoder`, edit its C source, then:

```sh
cargo build --offline --release --no-default-features --bin wavebob-headless --bin wavebob-mcp --config 'patch.crates-io.mp3lame-sys.path="modified-encoder"'
cargo build --offline --release --bin wavebob --config 'patch.crates-io.mp3lame-sys.path="modified-encoder"'
```

This intentional path patch updates Cargo.lock, so omit `--locked` for the modified
build. Do not edit the checksum-verified registry copy in place. The original
binding build script recompiles and statically relinks the modified C library;
no signing key, network codec, application object secrecy or installation lock
prevents rebuilding. Restore the original lock/source tree to reproduce the
unmodified build. Full source is provided instead of relying on stripped objects.
See [the encoder notices](mp3-encoder.md) and the kit's third-party inventory.

The application is licensed under [GNU GPL v3](../LICENSE). The bundled
third-party source/license texts retain their own terms; this document does not
replace them.

## Current live-processing build

The source includes owner-scoped streaming processing and all eleven verified
processors: Soft Clipper, Reverb, Parametric EQ, Distortion, Delay, Chorus,
Limiter, Balance, Stereo Enhancer, Stereo Shaper and Transient Processor. Each
has a separate native editor; document/clip gain/fade controls remain independent.
These builds retain the baseline WAV/MP3 workflows. Live document/clip processing renders into current shared lossless bounces and
processed WAV/MP3 exports. Schema2 projects retain editable sources, stacks, trim
intent, tempo, policy and current bounce assets. Browser audition remains dry. See
[live processing](live-processing.md) and [envelope controls](envelope-controls.md).

The earlier Windows kit and its original verification describe the baseline
commit recorded in its manifest. New kits have a separate output directory and
record their exact source commit; they do not replace that historical evidence.

## Known limits

- Latest native macOS/Linux, physical trackpad and subjective audio acceptance remain
  deferred. Windows UI, native callback, file/path, scale and MCP capture checks
  are recorded under `docs/verification/`.
- Editing accepts mono/stereo WAV and MP3. MP3 export offers CBR 128/192/256/320
  kbit/s at 32/44.1/48 kHz. Project audio stays lossless.
- Legacy source reverb is bounded to the explicit editing/clip/range interval.
  Live processors use the documented renderer tail policy; linked tail geometry
  and wet export are delivered. At most two clip bodies overlap per track;
  a separate64-concurrent-tail budget preserves late sound without timeline ripple.
- Managed project assets are append-only; keep the manifest and assets together.
  Undo history and clipboard do not survive reopen. Memory budgets are 512 MiB
  per decoded/processed audio and 2 GiB retained PCM; arbitrarily large streaming
  projects are outside this version.
- The bundled font lacks some CJK glyphs, although Unicode native paths and
  persistence work. Desktop output uses the default device; device selection,
  plugin hosting, recording and MIDI are outside this release.

Latest local live-effects verification follows the user's explicit policy: focused
checks per ticket, full local suites at the end, no further GitHub Linux/macOS
runs. Historical ticket21 three-platform release/native evidence remains valid for
its recorded commits; it does not establish latest-effects platform coverage.
Windows local package/smoke and measured manual-clock callback evidence are recorded
[here](verification/live-effects-workflow/README.md). Physical listening, trackpads,
other-device behavior and latest macOS/Linux/X11/Wayland execution are unverified.Development reference modules/FFmpeg are absent from production runtime dependencies; 
all codecs and eleven WaveBob processors are built in.

Final local kit: `dist/live-effects-final-9bcf87f/WaveBob-0.1.0-x86_64-pc-windows-msvc.zip`
(implementation commit9bcf87f). Extracted direct/headless/MCP/GUI and all-eleven-effect
public-seam smoke checks pass. [Build, checksum, smoke and timing evidence](verification/live-effects-workflow/README.md).
