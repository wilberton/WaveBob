# Ticket 18 verification

Windows, 24 October 2026.

- 117 ordinary GUI-enabled tests and 81 GUI-free tests pass; both Clippy variants
  deny warnings and formatting passes.
- Device-free WAV exports in all four encodings match offline mixed PCM within
  quantization tolerance. Fixtures include mixed rates/layouts, instance DSP,
  overlapping clips, pan/gain and silent range boundaries. Exact range/frame
  summaries, immutable snapshot export during later edits, destination refusal,
  source-path protection and cancellation are covered. Exports leave dirty state.
- MP3 mixes reimport with exact gapless stereo duration at all three output rates.
  Independent FFmpeg decoding verifies selected, resampled, crossfaded/reverberant
  mixes at 128/192/320 kbit/s. `encoder-mix-qa.json` records exact duration and
  zero-offset signal comparison: maximum RMS error 0.00222, maximum error 0.00510,
  correlation above 0.99999999. Limits are RMS <0.01, max <0.06, correlation >0.98,
  excluding 1024 edge frames. Ticket 13 separately covers every offered bitrate.
- Actual egui input exports a selected arrangement interval as MP3 without a
  device, retaining explicit format/range settings and dirty state.
- Direct commands, a real GUI-free headless process and MCP produce identical
  float-WAV mix bytes after MP3 import/edit and overlapping-clip arrangement.
- Native release-app MCP imports WAV/MP3, applies clip reverb, arranges containment,
  exports float WAV and resampled 192 kbit/s MP3, compares WAV samples with the
  mixer, reimports exactly 26460 stereo frames at 44.1 kHz and retrieves a PNG.
  `mix-reimport.json` retains summaries and `mix-reimport.png` was visually inspected
  against the mockup waveform shell, including selection status and panel handles.

FFmpeg is a QA-only independent decoder, not a runtime dependency. Native
macOS/Linux verification remains deferred by the user until a remote exists.
