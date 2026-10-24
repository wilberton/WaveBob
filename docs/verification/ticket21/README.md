# Local Windows release verification

Windows 11 x86-64, 24 October 2026. Built from clean commit
`19bf4e8a0a7139edd19d8dc1b8791ea433fbc162` using Rust 1.99.1/MSVC.

The local development kit is
`dist/WaveBob-0.1.0-x86_64-pc-windows-msvc.zip` (113480842 bytes), with SHA-256:

```text
fcbca119f4d18aba65de80f66fa976ceaffac0fee90ec20ced6bae0132d10a64
```

The archive contains the GUI, headless runner, MCP runner and workflow client,
complete application sources, all 374 locked registry dependency source packages,
unmodified LAME/bindings source, original license notices, Inter/OFL, Rust library
sources and exact-compiler-commit notices, offline Cargo configuration and pinned
compiler version. The [manifest](BUILD-MANIFEST.json) records toolchain/platform,
commit, executable hashes, lock hash and dependency archive hashes. The
[third-party inventory](THIRD_PARTY_NOTICES.md) links source locations inside the kit.
No external codec executable is bundled or required. Application licensing for
public redistribution remains unselected; this is the user's local review kit.

`scripts/smoke-release.py` extracted the ZIP to a fresh directory with Unicode
characters and spaces, checked archive/executable hashes, and ran its shipped
direct example, headless executable and MCP executable through the entire
WAV/MP3 editing/arrangement/export/project scenario. All three produced identical
float-WAV mix, MP3 mix and MP3 selected-sample bytes. The shipped GUI then ran the
same scenario through attached MCP, including native browser audition, project
reopen without original files, PNG image-content retrieval and a guarded clean
quit with exit code zero. Both [waveform](waveform.png) and
[arrangement](multitrack.png) screenshots were inspected. See
[smoke result](smoke-result.json) for session/revision metadata and output hashes.

The automation executables were built separately with `--no-default-features`.
`cargo tree --no-default-features --edges normal` contains no eframe, egui, winit
or CPAL. Windows executable dependency inspection confirms no OpenGL or audio-device
DLL import in the headless runner. System runtime imports include VCRUNTIME140
and Universal CRT; those prerequisites are documented in the release guide.

## Offline source and relinking check

From the extracted source at its Unicode path, an empty Cargo home and separate
target directory successfully built the headless/MCP runners and workflow example
with `--locked --offline --release --no-default-features`. The installed stable
toolchain was used explicitly; its version matches the kit's pinned 1.99.1.
No registry source/archive cache was available; dependencies came from the kit's
vendor tree. The rebuilt headless workflow produced byte-identical audio outputs
to the shipped binaries.

Then the packaged encoder crate was copied to `modified-encoder`, and only its
short identification string was changed to the QA marker `3.101`. This is a test
identifier, not an official library upgrade. The documented Cargo path patch
rebuilt/relinked both runners offline. Their complete workflows passed, the MP3
files contain the changed `LAME3.101` tag and their lossless mixed WAV stays
byte-identical. The distribution ZIP and its original LAME 3.100 source/binaries
remain unchanged. [Relink results](offline-relink.json) record the output hashes.
The path dependency exposes one upstream deprecated `cc::Build::shared_flag`
warning; the build succeeds without source changes to that build script.

The [release guide](../../release.md) documents rebuilding, modifying/relinking,
runtime prerequisites, local launch, command/MCP discovery, screenshot retrieval,
the reproducible example and known limits. The packaging script handles registry
timestamps before ZIP's 1980 epoch and stages the complete archive before exclusive
publication; the failed preliminary timestamp attempt was not shipped.

## Deferred verification

The prepared manual release workflow targets native Windows, macOS and Linux,
packages complete source/notices and smoke-checks shipped device-free executables.
It has not run: there is no remote. Native macOS/Linux, X11/Wayland, physical
trackpad and subjective multi-platform listening checks remain unavailable here.
Windows clean-machine runtime installation is also unverified. Signing,
notarization, installers, auto-update and public publishing remain deferred.
Ticket 21 stays in-progress for those required platform/CI checks, consistent with
the user's earlier deferral; Windows implementation and local delivery are complete.
