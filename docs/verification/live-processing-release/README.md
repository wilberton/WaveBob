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

### Confirmed Unix IPC result

The original 500 ms large-schema regression and the new 40 ms stalled-peer read/
write checks pass on both native Linux and native macOS in the
[formatted native release run](https://github.com/wilberton/WaveBob/actions/runs/40116010625).
The Linux full Rust job also passed. The causal fix is replacing sleep polling
with Unix socket readiness waiting under the remaining deadline; larger writes
alone were insufficient. Temporary test-only byte/timing probes were removed,
and local GUI/GUI-free IPC tests and both all-target Clippy configurations pass.

That release run found a missing Linux `libxkbcommon-x11` runtime (now included in
CI/build prerequisites) and an attached macOS GUI workflow failure. The smoke
harness now exposes child stderr on failure to make the latter diagnosable.
The artifact uploader explicitly includes the selected hidden-directory evidence
paths; it does not upload runtime instance/authentication records. Native GUI
acceptance is still pending those reruns.

### Native Linux shipped workflows

The [Linux release job](https://github.com/wilberton/WaveBob/actions/runs/40116617784/job/120939112485)
passed shipped GUI/headless/MCP WAV/MP3 workflows on both X11 (Xvfb) and Wayland
(headless Weston). The retained native artifact's SHA-256 is
`6290a488cf5ef5f65145f0671026d1e54c26176146128ab45e0d8a9777a026c6`.
Its source commit is `028377fc4af874ff684d58718ff58da19e673f67`.
The source-complete Linux kit artifact is available in that run.

The actual [X11 waveform](linux-x11-waveform.png),
[X11 arrangement](linux-x11-multitrack.png),
[Wayland waveform](linux-wayland-waveform.png) and
[Wayland arrangement](linux-wayland-multitrack.png) captures were inspected.
The device-free CI host reports unavailable audio output while editing/export and
capture remain operational. That error state exposed timing text clipped by the
wide Retry output button; the retry control now retains the Play button's width.
All 50 GUI application tests pass after that layout correction.

[X11 results](linux-x11-smoke.json) and [Wayland results](linux-wayland-smoke.json)
record byte-identical per-host direct/headless/MCP exports, source-free project
reopen and clean GUI quit. Physical audio and gestures remain unverified under
the user's automated-acceptance decision.

Windows hosted CI lacks OpenGL 2.0 support. The next run supplies an installed
[MSYS2 Mesa runtime](https://packages.msys2.org/packages/mingw-w64-x86_64-mesa)
through the [MSYS2 setup action](https://github.com/msys2/setup-msys2).
The extraction records renderer DLL hashes; package ZIP contents remain as built.
Mesa's [documented software-renderer settings](https://docs.mesa3d.org/envvars.html#gallium-driver)
select llvmpipe. Native macOS workflow diagnostics remain in progress.

### Confirmed Windows hosted GUI result

The [Windows native release job](https://github.com/wilberton/WaveBob/actions/runs/40117429327/job/120941531066)
passed the shipped GUI/headless/MCP WAV/MP3 workflow using the installed Mesa
llvmpipe runtime. The source-complete Windows kit was uploaded in that run.
The native evidence artifact has SHA-256
`0a623015b443933b056cf3275a66bf47ea0af82783b4ca60e2b7860222b09ce7`;
source commit `b4af502d4f2daa15eef9f4a029883f46d4fcb43d`.
[Workflow results](windows-ci-smoke.json), [waveform](windows-ci-waveform.png)
and [arrangement](windows-ci-multitrack.png) are retained here. Both actual
captures were inspected. No physical audio or device interaction is claimed.
The full Windows/macOS/Linux Rust run at this commit also passed.

The macOS shipped headless/MCP workflows pass; native GUI attachment still fails
before MCP initialization. The contextual run at `ccc4a36` identifies
`MCP GUI attachment: GUI IPC receive: Invalid argument (os error 22)`.
Connection and send succeeded. Deadline-setup versus read diagnostics and the
public 12-second schema deadline regression are being checked next. The GUI-free
500 ms schema and stalled-peer deadline tests already pass on native macOS.

### macOS buffered-reply shutdown regression

The minimal Unix socket-pair regression at `0e91bd9` fails on
[native macOS](https://github.com/wilberton/WaveBob/actions/runs/40121574262/job/120954085279)
with the exact shipped-workflow error: `set receive timeout: Invalid argument
(os error 22)`. The writer buffers a complete newline-terminated reply and shuts
down before readback. No renderer, MCP or large schema is needed to reproduce it.
The original 16 KiB fixture could block before readback on a small platform send
buffer; reducing it to 1 KiB makes the test independent of that capacity.

[Apple's socket implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/uipc_socket.c)
rejects socket-option changes after shutdown in `sosetoptlock`. The frame reader
previously reset SO_RCVTIMEO before every read, racing the server's completed
reply and close. The candidate configures a receive timeout before sending the
request, then reads without further option changes. At most 100 ms readiness
slices plus an absolute deadline retain bounded waits for intermittent/stalled
peers. Windows keeps its existing named-pipe handling. Local handshake and
500 ms/public startup schema tests pass; the native red regression and shipped
macOS workflow are being rerun for final confirmation.

### Confirmed cross-platform native release result

The [native release run](https://github.com/wilberton/WaveBob/actions/runs/40121932636)
now passes **Windows x86-64, macOS ARM64 and Linux x86-64**, with Linux X11 and
Wayland shipped GUI workflows. All three source-complete kit artifacts are
uploaded, including GUI/headless/MCP/workflow executables, locked dependency
sources, notices and rebuild instructions. Source commit is
`02e4b850dcbcd9bafd2e9f6127d067de66f3ec03` (Reverb complete; EQ remains newer work).

The exact minimal macOS shutdown regression is green after configuring receive
readiness before request transmission. The causal diagnosis is confirmed by the
native shipped workflow passing after the fix; no timeout increase or skipped
acceptance check was used. The source pointer above documents Darwin's shutdown
socket-option rule. Temporary byte/timing probes remain removed.

Actual [macOS waveform](macos-waveform.png) and
[macOS arrangement](macos-multitrack.png) captures were inspected at 1024×642.
[macOS workflow evidence](macos-smoke.json) records direct/headless/MCP WAV/MP3
byte parity, source-free project reopen, GUI attachment/captures and clean quit.
Native evidence artifact SHA-256:
`2ffafdda21fe4b3eca5b0d82fd4017af2474f5c3ec937824548ffecada1671eb`.
The source-complete macOS kit artifact is
[available in the successful run](https://github.com/wilberton/WaveBob/actions/runs/40121932636/artifacts/12571512919).
Its inner ZIP SHA-256 is `9adab8777bddd1219addd4ec1e47d414276fc49098cf0c9dd1178476df491c8e`.
The Linux source-complete kit artifact is
[also available](https://github.com/wilberton/WaveBob/actions/runs/40121932636/artifacts/12571837215);
inner ZIP SHA-256 `ec07713c856d44ada808184c0f846cc4d05805432cd29bc5a86fa0b2874441ee`.

A further explicit 1.25× native GUI scale pass is now part of every platform's
smoke harness (and both Linux backends). It is already verified locally on the
preserved Windows kit from a fresh Unicode extraction. This complements shared
automated DPI/gesture/shortcut tests; physical device/listening checks stay
unverified under the user's accepted automated acceptance. The next native run
will retain both default- and raised-scale captures.

### Native light-theme contrast regression

The raised-scale macOS capture at source `49112a4` passed functional checks but
visual inspection found black text/white default widgets on the fixed dark
background. `cargo test --offline --bin wavebob native_light_system_theme`
reproduces the exact palette switch using a real egui system-theme event, with
no stored preferences or second viewport. It fails before the fix and passes
after explicitly selecting WaveBob's approved dark theme before installing its
visuals. The test also alternates light/dark/light system events and checks text
and panel colors. This prevents the one-time custom dark style from being
replaced by an unstyled light theme bank. `.tools/test21-theme-{red,green}.log`
retain the local red/green result. Native recapture will verify the final result.

### Final baseline delivery acceptance (`04132f9`)

[Release run 40125142114](https://github.com/wilberton/WaveBob/actions/runs/40125142114)
and [Rust run 40125120331](https://github.com/wilberton/WaveBob/actions/runs/40125120331)
pass all three native targets. All eight shipped GUI executions pass (default
and 1.25� on Windows/macOS and both Linux backends), alongside direct/headless/MCP
WAV/MP3 parity, portable reopen and clean quit. Actual default/raised-scale
captures were inspected: the late OS-light-theme event no longer changes the
approved dark palette; white text, controls, selection handles, crossfade and
Retry fit their containers. Final screenshots/results use `*-final-*` names in
this directory, preserving earlier diagnosis evidence.

| Target | Native evidence SHA-256 | Source-complete kit artifact |
| --- | --- | --- |
| Windows x86-64 | `4eb921d0e8010fbfa36fb821b852fbfca490c469be8db12f603ef8eded0f7b9b` | [12572677407](https://github.com/wilberton/WaveBob/actions/runs/40125142114/artifacts/12572677407) |
| macOS ARM64 | `6b666a07c7018c5311aa48c309c7b89729c01d6fe664cd02f00bc9b2c7850dd3` | [12573475900](https://github.com/wilberton/WaveBob/actions/runs/40125142114/artifacts/12573475900) |
| Linux x86-64 | `e2e50efeff74aaba8006947cf5840eb2ca6875488b78f5718f63048f843fbc6b` | [12573550887](https://github.com/wilberton/WaveBob/actions/runs/40125142114/artifacts/12573550887) |

The retained JSON files record each inner kit ZIP SHA-256, build commit,
platform, capture dimensions/scales, source-free reopen and output hashes.
The user explicitly accepted automated verification with physical checks
unverified. CI software graphics/no physical audio device do not establish
speaker listening, physical hardware or physical trackpad behavior. Windows
local native audition remains separately recorded above. Ticket 21 is complete
under that acceptance; ticket 44 will repeat delivery with all eleven effects.
