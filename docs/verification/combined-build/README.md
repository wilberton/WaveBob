# Combined Windows GUI build

24 October 2026. The current source at `24ebe9b` includes all four additions:

- External WAV/MP3 file drops and queued import jobs (ticket 23).
- Continuous antialiased waveform envelopes (ticket 24).
- Draggable waveform selection edges (ticket 25).
- A draggable crossfade endpoint that trims the ending clip (ticket 26).

The combined source passes formatting, all 39 GUI binary tests, all seven mixer
integration tests and Clippy with warnings denied. This also resolves the mixer
test build blockage recorded during ticket 26's original validation.

The release executable is `target\release\wavebob.exe`. It was freshly compiled
from the current checkout, rather than the earlier isolated source snapshot:

```powershell
cargo rustc --locked --offline --release --bin wavebob -- -C opt-level=3
```

The explicit release optimization argument forced recompilation after the
shared target directory reused the prior isolated executable. It retains the
normal release optimization level.

The native MCP exact-selection/zoom and automatic-crossfade/audio/overlay tests
both pass against this production executable. Its SHA-256 is
`aa8b2f1f73c53ff1dd2b284968f7afa1cd3f115611b6ab4d551377159ff5039c`.
Their captures and metadata are in
[waveform](waveform/) and [crossfade](crossfade/). The versioned distribution ZIP
has not been rebuilt; use the local executable for these additions.
