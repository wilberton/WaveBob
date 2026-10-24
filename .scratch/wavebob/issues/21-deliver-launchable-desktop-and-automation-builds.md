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
- [ ] Document command discovery, GUI attachment, screenshot inspection and a small
  reproducible automation example; publish known limitations and license notices.
- [ ] Signing, notarization, installers and auto-updates remain explicitly deferred.

## Specification

[WaveBob editor specification](../spec.md).

## Execution scope

The user deferred macOS/Linux CI until a remote exists. This local repository
still has no remote. Prepare repeatable packaging and CI artifacts, verify the
Windows build locally and document the remaining native platform checks; do not
claim unavailable macOS/Linux or CI executions as verified.
