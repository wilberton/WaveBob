# Owned audio fixtures

Generated deterministic sine/envelope signals with deliberate leading/trailing
silence. No third-party recordings are included. `manifest.json` records each MP3
and reference hash, layout, rate, source length and independently decoded length.
The `.f32` files are interleaved little-endian float32 PCM decoded by FFmpeg 7.1.

Regenerate with `scripts/generate-audio-fixtures.py --ffmpeg PATH_TO_FFMPEG`.
The original run used the FFmpeg 7.1 Windows binary from the pinned
`imageio-ffmpeg==0.6.0` wheel, solely as test tooling. It is not distributed with
WaveBob and is not required to run the checked-in tests or the application.

Coverage: mono/stereo CBR and VBR, MPEG-1 and MPEG-2 rates, with and without
Xing/Info/LAME and ID3 tags. Tagged files decode to exactly 8192 source frames.
Metadata-free files retain all 10368 decoded frames, including encoder delay and
padding; no silence is guessed away. Tests compare every sample against the
independent decoder (maximum error <0.002, RMSE <0.0001), and separately reject
corrupt/truncated input. PCM equality between lossy source and original signal
is not expected.
