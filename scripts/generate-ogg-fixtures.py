"""Generate owned Vorbis signals and independent FFmpeg PCM references."""
import argparse
import hashlib
import json
import math
import pathlib
import struct
import subprocess
import tempfile
import wave

parser = argparse.ArgumentParser()
parser.add_argument("--ffmpeg", required=True)
args = parser.parse_args()
output = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/audio"
version = subprocess.check_output([args.ffmpeg, "-version"], text=True).splitlines()[0]
records = []
with tempfile.TemporaryDirectory() as temp:
    for channels, rate in [(1, 22050), (1, 44100), (2, 48000), (6, 48000)]:
        name = f"vorbis-{channels}ch-{rate}"
        source = pathlib.Path(temp) / "source.wav"
        frames = 8192
        with wave.open(str(source), "wb") as writer:
            writer.setnchannels(channels)
            writer.setsampwidth(2)
            writer.setframerate(rate)
            values = [0 if frame < 500 or frame >= frames - 500 else
                      int(12000 * math.sin(2 * math.pi * (440 + channel * 173) * frame / rate))
                      for frame in range(frames) for channel in range(channels)]
            writer.writeframes(struct.pack("<" + "h" * len(values), *values))
        ogg = output / f"{name}.ogg"
        subprocess.run([args.ffmpeg, "-y", "-v", "error", "-i", str(source),
                        "-codec:a", "libvorbis", "-q:a", "4", "-map_metadata", "-1",
                        "-metadata", "title=WaveBob owned test signal", str(ogg)], check=True)
        reference = output / f"{name}.f32"
        # Ask FFmpeg to expose decoder padding rather than applying its automatic
        # skip (which omits a short final block on these streams). The first
        # lapped half-block is decoder delay; keep the known source duration.
        raw = subprocess.check_output([args.ffmpeg, "-v", "error", "-flags2", "+skip_manual",
                                       "-i", str(ogg), "-f", "f32le", "-codec:a", "pcm_f32le", "-"])
        data = ogg.read_bytes()
        packet = data.index(b"\x01vorbis")
        delay = (1 << (data[packet + 28] & 15)) // 2
        reference.write_bytes(raw[delay * channels * 4:(delay + frames) * channels * 4])
        records.append(dict(name=name, channels=channels, sample_rate=rate, source_frames=frames,
                            decoded_frames=reference.stat().st_size // (4 * channels),
                            decoder_delay_frames=delay,
                            decoder_padding_frames=len(raw) // (4 * channels) - delay - frames,
                            ogg_sha256=hashlib.sha256(ogg.read_bytes()).hexdigest(),
                            reference_sha256=hashlib.sha256(reference.read_bytes()).hexdigest()))
(output / "ogg-manifest.json").write_text(json.dumps(dict(generator=version, fixtures=records),
                                                    indent=2) + "\n", encoding="utf-8")
print(json.dumps(records, indent=2))
