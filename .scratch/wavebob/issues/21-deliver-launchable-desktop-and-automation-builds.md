# 21: Deliver launchable desktop and automation builds

**What to build:** Downloadable Windows, macOS and Linux builds with usable
GUI, headless and MCP launch instructions and documented platform prerequisites.

**Blocked by:** 20 — Verify a complete AI editing and visual-review workflow.

**Status:** in-progress

- [ ] CI produces launchable release artifacts for the agreed desktop targets;
  headless operation does not acquire a mandatory graphics/device dependency.
- [ ] Smoke-check native file paths, DPI, gestures, shortcuts, audio and captures on
  each OS, including Linux X11/Wayland where available.
- [ ] Shipped GUI, headless and MCP workflows import and export MP3 on all three
  platforms without separately installed codec executables. Include selected
  codec dependencies and license notices in artifacts and build instructions.
- [x] Document command discovery, GUI attachment, screenshot inspection and a small
  reproducible automation example; publish known limitations and license notices.
- [x] Signing, notarization, installers and auto-updates remain explicitly deferred.

## Specification

[WaveBob editor specification](../spec.md).

## Execution scope

The user deferred macOS/Linux CI until a remote exists. This local repository
still has no remote. Prepare repeatable packaging and CI artifacts, verify the
Windows build locally and document the remaining native platform checks; do not
claim unavailable macOS/Linux or CI executions as verified.

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
