# Ticket 02 verification

Verified on 23 October 2026 on Windows x86-64 with Rust 1.99.1, MSVC and eframe's
OpenGL renderer. The implementation is commit `b6d4961`; these artifacts document
the completed GUI attachment and capture slice.

## Running application captures

The release GUI was launched with `--hidden --name release-smoke` and a dedicated
runtime directory. This starts the actual egui renderer without showing a window.
The separately built GUI-free release runner discovered its UUID, attached to it,
enabled loop, switched modes and requested PNGs through stdin/stdout JSON. All
seven requests succeeded against the same session. An independent GUI-free
session returned `missing_capability` for capture as expected.

Both images contain the complete 1440×900 client area at scale 1.0, including the
side panels and bottom transport. Both were inspected visually. These are actual
framebuffer captures of the demo-data shell: real audio, status-bar/panel behavior
and the close match to the accepted mockups arrive in later tickets. Ticket 22
performs the requested UI refinement immediately after ticket 05.

| Image | Core revision | Presentation revision | Frame | Mode |
| --- | --- | --- | --- | --- |
| [Waveform](waveform.png) | 1 | 2 | 17 | waveform |
| [Multitrack](multitrack.png) | 1 | 1 | 12 | multitrack |
| [Concurrent capture](race.png) | 1 | 3 | 38 | multitrack |

The concurrent capture comes from the separate two-instance native integration
test. Its decoded pixels matched the multitrack reference for the mode reported
in its metadata. Session IDs, dimensions, scale and render revisions are retained
in [metadata.json](metadata.json); original temporary output locations are omitted.

## Automated checks

The following passed with the committed dependency lock (`--offline` was also
used locally after fetching dependencies):

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --no-default-features --all-targets
cargo test --locked --test gui_attachment -- --ignored --nocapture
cargo build --locked --release --bin wavebob
cargo build --locked --release --no-default-features --bin wavebob-headless
```

The default configuration passed 15 ordinary tests and the separately invoked
native integration test. The GUI-free configuration passed all 13 applicable
tests. The native test starts two real GUI instances and verifies explicit
attachment, isolation, capability discovery, strict parameters, stale presentation
revisions, both modes, actual PNG dimensions/content, barriers, timeouts and
recovery, missing output directories, overwrite refusal/preservation, explicit
overwrite, automatic output paths, stale instances and a concurrent mode/capture
request. The core suite verifies GUI-only commands report missing capability
without a renderer. Set `WAVEBOB_TEST_ARTIFACT_DIR` to retain the native test images.

Two Windows IPC issues found during this work have regression coverage: idle
nonblocking named-pipe reads can return zero bytes, and large schema replies need
bounded writes for the pipe's small default buffer. The screenshot checks also
exposed a growing transport panel; giving it a fixed height stabilized rendering.

Windows tests ran with normal file permissions. The restricted sandbox denied
the file-attribute update used by atomic PNG publication; the same focused test
passed outside that restriction. Native macOS/Linux checks and remote CI remain
deferred at the user's request until a remote exists.

See the [command protocol](../../commands.md) for attachment and capture examples,
including timeout bounds and explicit output/overwrite parameters.
