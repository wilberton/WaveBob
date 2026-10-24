# Live-processing release verification — 24 October 2026

## Clean Windows kit

A separate complete development kit was built from clean commit
`efb1dddbb3d2315e82228928d1c4896fb5e3437d`, preserving the user's uncommitted
Cargo development/test profile changes and the earlier baseline release.
The ZIP is `dist/live-processing-efb1ddd/WaveBob-0.1.0-x86_64-pc-windows-msvc.zip`:
122416069 bytes, SHA-256
`5685f062583fba6c514098d028af0796e593f98c0e58abac32784ad959251751`.
Its [build manifest](windows-build.json) records all 374 locked dependency sources,
exact compiler, binary hashes and bundled source/notices. START-HERE names GNU GPL v3.

The shipped binaries passed `scripts/smoke-release.py --gui --audition` after
fresh extraction at a path containing spaces and Ω. Direct/headless/MCP workflows
produced byte-identical WAV mix, MP3 mix and MP3 selected-sample exports. The GUI
completed the same editing/project/export scenario, native audition, attached MCP
PNG retrieval in both modes/panel layouts and guarded clean quit. The project
reopened without its original source files. See [smoke result](windows-smoke.json).
Both [waveform](windows-waveform.png) and [arrangement](windows-multitrack.png)
were inspected. Native output exercised the actual device; no subjective listening
result is inferred from that success.

This kit includes streaming owners, Soft Clipper, native effect editors and gain/
fade handles. Its manifest predates the subsequent Unix IPC fix. Other effect
constructors and linked bounce/wet export remain pending their tracker tickets;
this intermediate kit does not claim the completed extension.

## Native CI and macOS diagnosis

Committed work is on `codex/live-processing`; remote `main` was not changed.
The existing [baseline CI](https://github.com/wilberton/WaveBob/actions/runs/40090040804)
passed Windows/Linux and failed macOS's 500 ms large-schema IPC regression.
The [initial current release run](https://github.com/wilberton/WaveBob/actions/runs/40115113666)
reproduced it; it produced a Linux source-complete artifact while macOS failed.

The [instrumented run](https://github.com/wilberton/WaveBob/actions/runs/40115371352)
measured a 445691-byte schema serialized in 39 ms, but only 386560 bytes received
before the deadline. Changing Unix writes from 512 bytes to 64 KiB alone still
failed ([comparison run](https://github.com/wilberton/WaveBob/actions/runs/40115602941)).
The next candidate replaces Unix nonblocking sleep polling with OS socket read/
write timeouts, refreshed against the remaining frame deadline. Windows keeps its
existing pipe compatibility path. A Unix-only stalled-peer test checks bounded
reads and writes; the original 500 ms schema deadline is retained.

Current [Rust verification](https://github.com/wilberton/WaveBob/actions/runs/40116005938)
and [native release verification](https://github.com/wilberton/WaveBob/actions/runs/40116010625)
are pending. The release workflow now runs shipped GUI/headless/MCP MP3 scenarios,
retains actual PNGs and uses Xvfb for Linux X11 and headless Weston for Wayland.
Physical trackpad/audio checks remain distinct from those automated checks.
Ticket 21 remains in-progress until its acceptance is actually established.
