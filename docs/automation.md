# Reproducible editing workflow

The example client uses the public command seam and can target the library,
the real NDJSON runner or a stdio MCP server. It creates deterministic owned WAV
signals, generates/imports MP3 using WaveBob, selects exact frames, applies gain,
undoes/redoes, copies/pastes with rate conversion, creates four tracks and five
clips, applies instance reverb, derives a crossfade, exports WAV/MP3, reimports
MP3 and saves/reopens a project after deleting its original source files.
Assertions verify dimensions, exact mixed PCM preservation and error boundaries.
The output directory must be new; existing work is never overwritten.

```sh
cargo run --locked --release --no-default-features --example workflow -- --adapter direct --out review-direct
cargo build --locked --release --no-default-features --bin wavebob-headless --bin wavebob-mcp
cargo run --locked --release --no-default-features --example workflow -- --adapter headless --binary target/release/wavebob-headless --out review-headless
cargo run --locked --release --no-default-features --example workflow -- --adapter mcp --binary target/release/wavebob-mcp --out review-mcp
```

On Windows append `.exe` to binary paths. The three standalone routes require no
window or audio output. `tests/workflow.rs` runs this same scenario through all
three and compares public results, mixed PCM and complete exported WAV/MP3 bytes.

For visual review, launch `wavebob --name Review`, discover its UUID with
`wavebob-mcp --list-instances`, then target it explicitly:

```sh
cargo run --locked --release --example workflow -- --adapter mcp --binary target/release/wavebob-mcp --attach SESSION_UUID --out review-gui --audition
```

`--audition` additionally checks the actual browser preview voice and requires an
audio device. Omit it on machines without one. Use the same `--runtime-dir PATH`
as the GUI when using an isolated registry. The attached workspace is edited by
the scenario: start a fresh GUI session. It does not implicitly discard dirty
work. PNGs show both modes with panels shown/hidden/restored. MCP image bytes
are checked against the written PNG, with session and revision fences recorded
in `workflow.json`. Its large `mixed_pcm` array is intentional numerical evidence;
the console summary omits that array.

Copy `Editing session.wavebob` with `Editing session.assets/` to reopen it. The
generated standalone outputs remain in the review directory. The example uses
only WaveBob's bundled encoder/decoder; FFmpeg is used separately for QA.

The delivered live-effects scenario is selected with --live-effects on the same
workflow client. It imports WAV and bundled-encoder MP3, inserts shared full/trim
links, commits an envelope preview, reorders/bypasses/undoes a three-effect stack,
checks separate late tails and later positions, adds clip Limiter processing,
saves/reopens schema2 after deleting originals and exports identical WAV/MP3 across
adapters. With --attach it additionally opens three simultaneous native editors
and captures both main modes through attached MCP. Use a fresh GUI/workspace and
new output directory. The test runs normal, narrow and high-DPI native sizes.

```sh
cargo run --offline --release --no-default-features --example workflow -- --live-effects --adapter mcp --binary target/release/wavebob-mcp --out live-review
```

[Final verification](verification/live-effects-workflow/README.md) distinguishes
fixed-setting reference comparisons, live transition/loop state, codec tolerances,
manual callback measurements and physical listening/platform limits.
