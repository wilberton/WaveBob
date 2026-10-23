# Ticket 08 verification

24 October 2026, Windows, release binaries built with the committed dependency
lock. [MCP-delivered screenshot](native-playback.png) was inspected visually;
[recorded telemetry](native-playback.json) includes requested/applied generations,
device configuration, cursor and capture revision metadata.

The native CPAL stream initialized Speaker (Realtek(R) Audio), 48 kHz stereo.
The running callback delivered nonzero PCM (peak 0.030517578125), advanced position,
paused at a stable frame and resumed within the 48000–96000 selection loop.
The capture has no pending resources and shows the applied paused cursor.
This is device/callback verification; no human listening claim is made.

Eight deterministic transport tests cover exact mono/stereo frames, pause,
rewind, EOF/restart, selection and single-frame loops, source-rate conversion,
fractional phase retention, exact rational clock duration, device failure/retry,
document changes and bounded queue pressure. An allocator guard verifies zero
allocations and frees in callback rendering, including state retirement.
An egui interaction test drives transport shortcuts and verifies mode-switch pause.

All 55 ordinary GUI-enabled tests and 37 GUI-free tests pass. Both Clippy
configurations pass with warnings denied; formatting and release builds pass.
The native MCP audio/capture test passes against those release binaries.
Normal Windows permissions were used for native devices and atomic file output;
the restricted sandbox denies tempfile's Windows attribute update during publish.

Native macOS/Linux audio and CI checks remain deferred until a remote exists,
as authorized by the user. ALSA build prerequisites are documented and CI inputs
are prepared. Physical listening on the three platforms remains a release check.
