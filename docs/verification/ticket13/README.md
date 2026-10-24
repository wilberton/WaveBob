# Ticket 13 verification

Windows, 24 October 2026. All sample-output acceptance checks pass.

- 93 ordinary GUI-enabled tests and 62 GUI-free tests; both Clippy configurations
  with warnings denied and formatting pass.
- WAV round trips cover all encodings, quantization, overload policy and resampling.
- 24 MP3 mono/stereo, rate and bitrate combinations independently decode with
  exact intended gapless duration. `encoder-qa.json` records signal tolerances.
- Cancellation, failed/stale writes, exact saved revisions, undo/redo after save
  and session shutdown preserve source/destination state and clean temporary files.
- Actual egui inputs cover MP3 export, dirty close cancel/discard/save, guarded
  quit and choosing lossless WAV Save As when closing an edited MP3.
- Direct, headless and MCP export the same selected PCM bytes.
- Native release-app MCP test imports MP3, applies gain, refuses dirty quit,
  saves MP3 at 192 kbit/s / 48 kHz, reimports 8916 frames, retrieves a PNG and
  receives successful quit before the process exits. `native-save.json` retains
  metadata. `saved-mp3.png` was visually inspected against the mockup shell.

macOS/Linux native verification is deferred until a remote exists, as requested.
