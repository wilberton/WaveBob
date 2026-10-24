# 21: Deliver launchable desktop and automation builds

**What to build:** Downloadable Windows, macOS and Linux builds with usable
GUI, headless and MCP launch instructions and documented platform prerequisites.

**Blocked by:** 20 â€” Verify a complete AI editing and visual-review workflow.

**Status:** done

- [x] CI produces launchable release artifacts for the agreed desktop targets;
  headless operation does not acquire a mandatory graphics/device dependency.
- [x] Smoke-check native file paths, DPI, gestures, shortcuts, audio and captures on
  each OS, including Linux X11/Wayland where available.
- [x] Shipped GUI, headless and MCP workflows import and export MP3 on all three
  platforms without separately installed codec executables. Include selected
  codec dependencies and license notices in artifacts and build instructions.
- [x] Document command discovery, GUI attachment, screenshot inspection and a small
  reproducible automation example; publish known limitations and license notices.
- [x] Signing, notarization, installers and auto-updates remain explicitly deferred.

## Specification

[WaveBob editor specification](../spec.md).

## Execution scope

The user originally deferred macOS/Linux CI until a remote exists. The configured
remote is now `https://github.com/wilberton/WaveBob.git` (observed 24 October 2026).
Prepare repeatable packaging and CI artifacts, verify the Windows build locally
and record native platform/CI executions when actually run. A configured remote
alone does not verify artifacts, desktop behavior or MP3 workflows.

## Local delivery and remaining acceptance

Windows delivery is complete: the source-complete 0.1.0 x86-64 development ZIP is
in `dist/`, with SHA-256 sidecar, GUI, device-free headless/MCP runners, the workflow
client, 374 locked dependency sources, encoder/Rust/font notices and offline
rebuilding/relinking instructions. The shipped executables pass direct/headless/
MCP output parity, native GUI attachment/audition/captures and guarded clean quit
from a fresh Unicode-path extraction. An empty-cache offline source rebuild
reproduces the audio outputs; path-patched encoder relinking is also verified.
See [release guide](../../../docs/release.md) and
[verification](../../../docs/verification/ticket21/README.md).

The first three boxes remain open because CI has not run and macOS/Linux native
artifacts, MP3 workflows, gestures/audio and X11/Wayland captures are unavailable
on this Windows host. `.github/workflows/release.yml` is prepared for native
Windows/macOS/Linux packaging and shipped-binary smoke checks. No remote, push,
public release, signing or installer was created. Keep this ticket in-progress
until the deferred platform verification is performed; local work is complete.

## Current execution note â€” 24 October 2026

The repository now has an `origin` remote, so the original no-remote condition is
no longer current. No CI dispatch, remote push or new macOS/Linux native run was
performed during the live-processing foundation implementation. The first three
acceptance boxes remain open. The existing local-delivery verification above is
historical evidence for the baseline build, not evidence for the new processing
work. Rebuild and reverify release artifacts after the new implementation is ready.

## Live-processing release preparation â€” 24 October 2026

Ticket 41's document/clip envelope controls are committed and verified. Rebuild a
separate development kit from a clean checkout of the current source, preserving
the existing baseline ZIP/evidence and the user's uncommitted Cargo profile edit.
The generated START-HERE text now names the repository's selected GNU GPL v3
license and retains third-party/effect provenance. The release guide distinguishes
current live-processing limitations from the older baseline acceptance.

Remote read access was verified noninteractively with `git ls-remote origin`;
`main` was `8742c8bb46bdcb9dba1e17aa2936191a3bb33c7f` at that check.
This is connectivity evidence only, not a CI or native platform verification.

## Active CI and current Windows kit

The committed `codex/live-processing` branch has now been pushed specifically to
run the required CI; remote main remains unchanged. The initial native release
run reproduced the older macOS IPC timeout and produced the Linux artifact.
A clean current Windows source-complete kit passed shipped direct/headless/MCP
and GUI WAV/MP3 workflows, native audition, captures and clean quit. Its exact
commit/hash and inspected captures are retained in
[current release verification](../../../docs/verification/live-processing-release/README.md).

Unix IPC diagnosis and native GUI CI verification are ongoing. The workflow now
includes actual shipped GUI captures on Windows/macOS and Linux X11/Wayland.
No macOS/Linux GUI success, subjective listening, physical gesture or clean-machine
result is claimed before execution. Earlier no-push/no-CI notes above describe
historical checks and are superseded by this active execution note.

## Completed native automated acceptance — 24 October 2026

The user approved automated acceptance with physical listening/device checks
explicitly unverified. [Release run 40125142114](https://github.com/wilberton/WaveBob/actions/runs/40125142114)
and [Rust run 40125120331](https://github.com/wilberton/WaveBob/actions/runs/40125120331)
both pass Windows x86-64, macOS ARM64 and Linux x86-64 at source `04132f9`.
Shipped direct/headless/MCP/GUI WAV/MP3, portable project reopen, Unicode paths,
attachment/captures/clean quit pass. GUI captures at default and explicit 1.25×
scale cover Windows, macOS, Linux X11 and Wayland; visual inspection confirms the
fixed dark palette and usable transport/selection/clip controls. Shared native
Rust suites exercise scripted gestures, shortcuts and audio output comparisons.
Retained final screenshots, exact kit hashes and machine-readable results are in
[release evidence](../../../docs/verification/live-processing-release/README.md).

Physical speaker/listening, hardware audio-device operation and physical
mouse/trackpad behavior on macOS/Linux remain unverified; CI uses software
rendering and reports unavailable audio devices visibly. Those limits are
recorded under the user's acceptance decision. Signing, installers,
notarization and auto-update remain deferred. Ticket 44 will package and verify
the final all-effects implementation again; this closes baseline delivery.
